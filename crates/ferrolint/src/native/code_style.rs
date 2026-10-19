//! The messages of `pylint.extensions.code_style` (`R6101`, `R6102`, `R6103`,
//! `R6105`) and `pylint.extensions.bad_builtin` (`W0141`). Each reports only
//! when `load-plugins` names its module; `R6105` is also off by default, as
//! in Pylint (see `selection.rs`).

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Comprehension, Expr, ExprDict, ExprName, Stmt, UnaryOp};
use ruff_text_size::Ranged;

use super::astring::{as_string, as_string_in_statement};
use super::classes::Env;
use super::{Checker, Ctx, rule};

const NAMEDTUPLE_OR_DATACLASS: &str = "R6101";
const TUPLE: &str = "R6102";
const ASSIGNMENT_EXPR: &str = "R6103";
const TYPING_NAMEDTUPLE: &str = "R6105";
const BAD_BUILTIN: &str = "W0141";

/// Whether the configuration loads `pylint.extensions.<module>`.
fn loaded(ctx: &Ctx<'_>, module: &str) -> bool {
    ctx.option("load-plugins").is_some_and(|list| {
        list.split(',')
            .any(|name| name.trim().strip_prefix("pylint.extensions.") == Some(module))
    })
}

pub struct CodeStyle;

impl Checker for CodeStyle {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                NAMEDTUPLE_OR_DATACLASS,
                "consider-using-namedtuple-or-dataclass",
                "A module-level dict whose values are dicts with a common key, or lists and tuples of one length; needs `pylint.extensions.code_style`.",
            ),
            rule(
                TUPLE,
                "consider-using-tuple",
                "A list literal as the iterable of a `for` or a comprehension; needs `pylint.extensions.code_style`.",
            ),
            rule(
                ASSIGNMENT_EXPR,
                "consider-using-assignment-expr",
                "An assignment right before an `if` that tests the name; needs `pylint.extensions.code_style`.",
            ),
            rule(
                TYPING_NAMEDTUPLE,
                "prefer-typing-namedtuple",
                "A `collections.namedtuple` call; off by default, needs `pylint.extensions.code_style`.",
            ),
            rule(
                BAD_BUILTIN,
                "bad-builtin",
                "A call to a builtin that `bad-functions` lists (`map` and `filter` by default); needs `pylint.extensions.bad_builtin`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let on: HashSet<&str> = [
            (NAMEDTUPLE_OR_DATACLASS, "code_style"),
            (TUPLE, "code_style"),
            (ASSIGNMENT_EXPR, "code_style"),
            (TYPING_NAMEDTUPLE, "code_style"),
            (BAD_BUILTIN, "bad_builtin"),
        ]
        .into_iter()
        .filter(|(id, module)| enabled.contains(*id) && loaded(ctx, module))
        .map(|(id, _)| id)
        .collect();
        if on.is_empty() {
            return;
        }
        let bad_functions = ctx
            .option("bad-functions")
            .unwrap_or("map,filter")
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        let int = |key| ctx.option(key).and_then(|v| v.trim().parse::<usize>().ok());
        let max_length = int("max-line-length-suggestions")
            .filter(|&n| n != 0)
            .or_else(|| int("max-line-length"))
            .unwrap_or(100);
        let namedtuple = on.contains(TYPING_NAMEDTUPLE).then(|| Env::new(ctx));
        Walk {
            ctx,
            on,
            bad_functions,
            max_length,
            namedtuple,
            depth: 0,
            out,
        }
        .visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'o> {
    ctx: &'a Ctx<'a>,
    on: HashSet<&'a str>,
    bad_functions: Vec<&'a str>,
    max_length: usize,
    namedtuple: Option<Env<'a>>,
    /// How many statements enclose the one being visited; 0 is the module.
    depth: usize,
    out: &'o mut Vec<Diagnostic>,
}

impl<'a> Walk<'a, '_> {
    // R6101
    fn dict_values(&mut self, dict: &ExprDict) {
        let values: Vec<&Expr> = dict.iter_values().collect();
        if values.len() < 2 {
            return;
        }
        let consider = if values.iter().all(|v| v.is_dict_expr()) {
            Self::common_str_key(&values)
        } else if values.iter().all(|v| v.is_list_expr() || v.is_tuple_expr()) {
            Self::same_length(&values)
        } else {
            false
        };
        if consider {
            self.out.push(self.ctx.report(
                NAMEDTUPLE_OR_DATACLASS,
                dict,
                "Consider using namedtuple or dataclass for dictionary values",
            ));
        }
    }

    /// Every key of the sub-dicts is a string and one key is in all of them.
    /// shortcut: a key is a string only when written as a literal, where
    /// Pylint infers a name or an expression; upgrade with a constant fold.
    fn common_str_key(values: &[&Expr]) -> bool {
        let mut common: Option<BTreeSet<String>> = None;
        for value in values {
            let Expr::Dict(sub) = value else { return false };
            let mut keys = BTreeSet::new();
            for item in &sub.items {
                let Some(Expr::StringLiteral(key)) = &item.key else {
                    return false;
                };
                keys.insert(key.value.to_str().to_owned());
            }
            common = Some(match common {
                None => keys,
                Some(seen) => seen.intersection(&keys).cloned().collect(),
            });
        }
        common.is_some_and(|keys| !keys.is_empty())
    }

    /// The lists or tuples are not empty, have one length and are not all
    /// made of dicts.
    fn same_length(values: &[&Expr]) -> bool {
        let elts = |value: &Expr| match value {
            Expr::List(list) => list.elts.clone(),
            Expr::Tuple(tuple) => tuple.elts.clone(),
            _ => Vec::new(),
        };
        let first = elts(values[0]).len();
        first > 0
            && values.iter().all(|v| {
                let items = elts(v);
                items.len() == first && !items.iter().all(Expr::is_dict_expr)
            })
    }

    // R6103
    fn assignment_expr(&mut self, body: &'a [Stmt], at: usize) {
        let Stmt::If(node) = &body[at] else { return };
        if at == 0 || self.ctx.py_version.is_some_and(|v| v < (3, 8)) {
            return;
        }
        let name = match &*node.test {
            Expr::Name(name) => name,
            Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => match &*unary.operand {
                Expr::Name(name) => name,
                _ => return,
            },
            Expr::Compare(compare) if compare.ops.len() == 1 => match &compare.operands[0] {
                Expr::Name(name) => name,
                _ => return,
            },
            _ => return,
        };
        let Some(value) = self.assigned_value(&body[at - 1], name.id.as_str()) else {
            return;
        };
        if Self::prefers_match(node, body.get(at + 1), name.id.as_str()) {
            return;
        }
        let text = as_string(self.ctx.source, &node.test).replacen(
            name.id.as_str(),
            &format!(
                "({} := {})",
                name.id,
                as_string_in_statement(self.ctx.source, value)
            ),
            1,
        );
        let suggestion = format!("if {text}:");
        let start = usize::from(node.start());
        let column = start - self.ctx.source[..start].rfind('\n').map_or(0, |i| i + 1);
        let length = suggestion.chars().count();
        if length + column > self.max_length || length > self.max_length {
            return;
        }
        self.out.push(self.ctx.report(
            ASSIGNMENT_EXPR,
            name,
            format!("Use '{suggestion}' instead"),
        ));
    }

    /// The value of a one-line `name = value` or `name: T = value`.
    fn assigned_value<'s>(&self, stmt: &'s Stmt, name: &str) -> Option<&'s Expr> {
        let (target, value) = match stmt {
            Stmt::Assign(assign) => match &*assign.targets {
                [target] => (target, &*assign.value),
                _ => return None,
            },
            Stmt::AnnAssign(assign) => (&*assign.target, assign.value.as_deref()?),
            _ => return None,
        };
        let one_line = !self.ctx.source[stmt.range()].contains('\n');
        matches!(target, Expr::Name(n) if n.id.as_str() == name && one_line).then_some(value)
    }

    /// Whether the next `if` tests the same name, so a `match` fits better.
    fn prefers_match(node: &ruff_python_ast::StmtIf, next: Option<&Stmt>, name: &str) -> bool {
        if !matches!(*node.test, Expr::Compare(_)) {
            return false;
        }
        let tested = |test: &Expr| match test {
            Expr::Name(n) => n.id.as_str() == name,
            Expr::Compare(c) => matches!(&c.operands[0], Expr::Name(n) if n.id.as_str() == name),
            _ => false,
        };
        // an `elif`, or an `else` holding one `if` alone, is the whole `orelse`
        match node.elif_else_clauses.first() {
            Some(clause) if clause.test.is_some() => clause.test.as_ref().is_some_and(tested),
            Some(clause) if matches!(&*clause.body, [Stmt::If(_)]) => {
                matches!(&*clause.body, [Stmt::If(inner)] if tested(&inner.test))
            }
            _ => matches!(next, Some(Stmt::If(inner)) if tested(&inner.test)),
        }
    }

    // R6101
    fn is_final(annotation: &Expr) -> bool {
        let annotation = match annotation {
            Expr::Subscript(subscript) => &*subscript.value,
            other => other,
        };
        match annotation {
            Expr::Name(name) => name.id.as_str() == "Final",
            Expr::Attribute(attribute) => attribute.attr.as_str() == "Final",
            _ => false,
        }
    }

    // W0141
    fn bad_builtin(&mut self, call: &Expr, name: &ExprName) {
        let id = name.id.as_str();
        if !self.bad_functions.contains(&id) {
            return;
        }
        let Some(node) = ruff_python_ast::HasNodeIndex::node_index(name)
            .load()
            .as_u32()
        else {
            return;
        };
        if self.ctx.model.is_frame_or_module_local(node, id) {
            return;
        }
        let hint = matches!(id, "map" | "filter")
            .then_some(". Using a list comprehension can be clearer.")
            .unwrap_or_default();
        self.out.push(self.ctx.report(
            BAD_BUILTIN,
            call,
            format!("Used builtin function '{id}'{hint}"),
        ));
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        if self.on.contains(ASSIGNMENT_EXPR) {
            for at in 0..body.len() {
                self.assignment_expr(body, at);
            }
        }
        visitor::walk_body(self, body);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if self.on.contains(NAMEDTUPLE_OR_DATACLASS) {
            let found = match stmt {
                Stmt::Assign(assign) if self.depth == 0 => Some(&*assign.value),
                Stmt::AnnAssign(assign)
                    if self.depth == 0
                        || matches!(&*assign.target, Expr::Name(_))
                            && Self::is_final(&assign.annotation) =>
                {
                    assign.value.as_deref()
                }
                _ => None,
            };
            if let Some(Expr::Dict(dict)) = found {
                self.dict_values(dict);
            }
        }
        if let Stmt::For(node) = stmt
            && !node.is_async
            && self.on.contains(TUPLE)
            && node.iter.is_list_expr()
        {
            self.out.push(self.ctx.report(
                TUPLE,
                &*node.iter,
                "Consider using an in-place tuple instead of list",
            ));
        }
        self.depth += 1;
        visitor::walk_stmt(self, stmt);
        self.depth -= 1;
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        if self.on.contains(TUPLE) && comprehension.iter.is_list_expr() {
            self.out.push(self.ctx.report(
                TUPLE,
                &comprehension.iter,
                "Consider using an in-place tuple instead of list",
            ));
        }
        visitor::walk_comprehension(self, comprehension);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            if let Some(env) = &self.namedtuple
                && env.qualified(&call.func).as_deref() == Some("collections.namedtuple")
                && self.ctx.py_version.is_none_or(|v| v >= (3, 6))
            {
                self.out.push(self.ctx.report(
                    TYPING_NAMEDTUPLE,
                    expr,
                    "Prefer 'typing.NamedTuple' over 'collections.namedtuple'",
                ));
            }
            if self.on.contains(BAD_BUILTIN)
                && let Expr::Name(name) = &*call.func
            {
                self.bad_builtin(expr, name);
            }
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const ALL: &[&str] = &["R6101", "R6102", "R6103", "R6105", "W0141"];
    const PLUGINS: &str = "pylint.extensions.code_style, pylint.extensions.bad_builtin";

    /// The fixture's findings are Pylint 4.1.3's own, with both extensions
    /// loaded.
    macro_rules! fixture {
        ($name:ident) => {
            #[test]
            fn $name() {
                let source = include_str!(concat!("fixtures/", stringify!($name), ".py"));
                let expected = include_str!(concat!("fixtures/", stringify!($name), ".expected"));
                let options = [("load-plugins", PLUGINS)];
                assert_eq!(render(source, &options, ALL), expected.trim_end());
            }
        };
    }

    fixture!(code_style_tuple);
    fixture!(code_style_dict);
    fixture!(code_style_namedtuple);
    fixture!(code_style_assign_expr);
    fixture!(bad_builtin);

    #[test]
    fn assignment_expr_suggestions_follow_the_length_option() {
        let source = include_str!("fixtures/code_style_assign_expr.py");
        let expected = include_str!("fixtures/code_style_assign_expr.options.expected");
        let options = [
            ("load-plugins", PLUGINS),
            ("max-line-length-suggestions", "60"),
        ];
        assert_eq!(render(source, &options, ALL), expected.trim_end());
    }

    #[test]
    fn bad_functions_replaces_the_default_list() {
        let source = include_str!("fixtures/bad_builtin.py");
        let expected = include_str!("fixtures/bad_builtin.options.expected");
        let options = [
            ("load-plugins", PLUGINS),
            ("bad-functions", "map,input,reduce,len"),
        ];
        assert_eq!(render(source, &options, ALL), expected.trim_end());
    }

    #[test]
    fn silent_without_the_extension() {
        let source = "for a in [1]:\n    pass\nx = map(str, [])\nfrom collections import namedtuple\nnamedtuple('P', 'a')\n";
        assert_eq!(render(source, &[], ALL), "");
        let other = [("load-plugins", "pylint.extensions.docparams")];
        assert_eq!(render(source, &other, ALL), "");
        let code = [("load-plugins", "pylint.extensions.code_style")];
        assert_eq!(
            render(source, &code, ALL),
            "1:R6102:Consider using an in-place tuple instead of list\n\
             5:R6105:Prefer 'typing.NamedTuple' over 'collections.namedtuple'"
        );
        let bad = [("load-plugins", "pylint.extensions.bad_builtin")];
        assert_eq!(
            render(source, &bad, ALL),
            "3:W0141:Used builtin function 'map'. Using a list comprehension can be clearer."
        );
    }
}
