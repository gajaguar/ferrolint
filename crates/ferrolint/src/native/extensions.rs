//! The messages of Pylint's bundled extensions that one walk of the syntax
//! decides: `W0149` while-used, `C3401` dict-init-mutate, `R3501`
//! consider-refactoring-into-while-condition, `R5601`
//! confusing-consecutive-elif and `R6106` consider-math-not-float. Each
//! reports only when `load-plugins` names its `pylint.extensions` module.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{CmpOp, ElifElseClause, Expr, Number, Stmt};
use ruff_text_size::{Ranged, TextRange};

use super::astring::{as_string, as_string_in_statement};
use super::classes::Env;
use super::{Checker, Ctx, rule};

const WHILE_USED: &str = "W0149";
const DICT_INIT: &str = "C3401";
const WHILE_CONDITION: &str = "R3501";
const CONFUSING_ELIF: &str = "R5601";
const MATH_NOT_FLOAT: &str = "R6106";

/// Whether the configuration loads `pylint.extensions.<module>`.
pub(super) fn loaded(ctx: &Ctx<'_>, module: &str) -> bool {
    listed(ctx.option("load-plugins"), module)
}

/// Whether a `load-plugins` value names `pylint.extensions.<module>`.
pub(super) fn listed(load_plugins: Option<&str>, module: &str) -> bool {
    load_plugins.is_some_and(|list| {
        list.split(',')
            .any(|name| name.trim().strip_prefix("pylint.extensions.") == Some(module))
    })
}

/// The `pylint.extensions` module a rule's summary says it needs.
pub fn needed(rule: &RuleInfo) -> Option<&str> {
    let (_, rest) = rule.summary.split_once("needs `pylint.extensions.")?;
    rest.split('`').next()
}

pub struct Extensions;

impl Checker for Extensions {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                WHILE_USED,
                "while-used",
                "A `while` loop; needs `pylint.extensions.while_used`.",
            ),
            rule(
                DICT_INIT,
                "dict-init-mutate",
                "A dict literal assigned, then filled by the subscript assignments that follow; needs `pylint.extensions.dict_init_mutate`.",
            ),
            rule(
                WHILE_CONDITION,
                "consider-refactoring-into-while-condition",
                "A `while True:` loop that starts with `if ...: break`; needs `pylint.extensions.consider_refactoring_into_while_condition`.",
            ),
            rule(
                CONFUSING_ELIF,
                "confusing-consecutive-elif",
                "An `elif` right after an indented block that ends with an `if`; needs `pylint.extensions.confusing_elif`.",
            ),
            rule(
                MATH_NOT_FLOAT,
                "consider-math-not-float",
                "`float('inf')` or `float('nan')` where `math.inf` or `math.nan` reads better; needs `pylint.extensions.code_style`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let on: Vec<&str> = [
            (WHILE_USED, "while_used"),
            (DICT_INIT, "dict_init_mutate"),
            (WHILE_CONDITION, "consider_refactoring_into_while_condition"),
            (CONFUSING_ELIF, "confusing_elif"),
            (MATH_NOT_FLOAT, "code_style"),
        ]
        .into_iter()
        .filter(|(id, module)| enabled.contains(*id) && loaded(ctx, module))
        .map(|(id, _)| id)
        .collect();
        if on.is_empty() {
            return;
        }
        let math = on.contains(&MATH_NOT_FLOAT).then(|| Env::new(ctx));
        Walk { ctx, on, math, out }.visit_body(&ctx.parsed.syntax().body);
    }
}

/// An `if` as astroid sees it: the `if` statement, or one `elif` of its
/// chain with the clauses after it as its `orelse`.
#[derive(Clone, Copy)]
struct AstroidIf<'a> {
    body: &'a [Stmt],
    test: &'a Expr,
    clauses: &'a [ElifElseClause],
    range: TextRange,
}

/// The first node of an `orelse`.
enum Else<'a> {
    Empty,
    /// An `if`, and whether it is the whole `orelse` (an `elif`, or an
    /// `else` holding that `if` alone).
    If(AstroidIf<'a>, bool),
    Other,
}

impl<'a> AstroidIf<'a> {
    fn of(stmt: &'a Stmt) -> Option<Self> {
        let Stmt::If(stmt_if) = stmt else { return None };
        Some(Self {
            body: &stmt_if.body,
            test: &stmt_if.test,
            clauses: &stmt_if.elif_else_clauses,
            range: stmt_if.range(),
        })
    }

    fn elif(&self) -> Option<Self> {
        let first = self.clauses.first()?;
        Some(Self {
            body: &first.body,
            test: first.test.as_ref()?,
            clauses: &self.clauses[1..],
            range: first.range(),
        })
    }

    fn orelse(&self) -> Else<'a> {
        if let Some(elif) = self.elif() {
            return Else::If(elif, true);
        }
        let Some(first) = self.clauses.first() else {
            return Else::Empty;
        };
        match first.body.first().and_then(Self::of) {
            Some(node) => Else::If(node, first.body.len() == 1),
            None => Else::Other,
        }
    }

    /// Whether the chain of `orelse` ifs ends without an `else`.
    fn has_no_else(self) -> bool {
        let mut node = self;
        loop {
            match node.orelse() {
                Else::If(next, _) => node = next,
                Else::Empty => return true,
                Else::Other => return false,
            }
        }
    }
}

struct Walk<'a, 'o> {
    ctx: &'a Ctx<'a>,
    /// The rules to report.
    on: Vec<&'a str>,
    math: Option<Env<'a>>,
    out: &'o mut Vec<Diagnostic>,
}

impl<'a> Walk<'a, '_> {
    fn text(&self, expr: &Expr) -> String {
        as_string(self.ctx.source, expr)
    }

    fn text_in_statement(&self, expr: &Expr) -> String {
        as_string_in_statement(self.ctx.source, expr)
    }

    /// `d[key] = value` with `name` for `d`: the key and the value.
    fn mutation(stmt: &'a Stmt, name: &str) -> Option<(&'a Expr, &'a Expr)> {
        let Stmt::Assign(assign) = stmt else {
            return None;
        };
        let [Expr::Subscript(target)] = &*assign.targets else {
            return None;
        };
        matches!(&*target.value, Expr::Name(n) if n.id.as_str() == name)
            .then_some((&*target.slice, &*assign.value))
    }

    // C3401
    fn dict_init(&mut self, body: &'a [Stmt], at: usize) {
        let Stmt::Assign(assign) = &body[at] else {
            return;
        };
        let ([Expr::Name(target)], Expr::Dict(dict)) = (&*assign.targets, &*assign.value) else {
            return;
        };
        let name = target.id.as_str();
        if body
            .get(at + 1)
            .is_none_or(|next| Self::mutation(next, name).is_none())
        {
            return;
        }
        // astroid keeps a `**x` entry's key as a node that prints `**`
        let literal = dict.items.iter().map(|item| match &item.key {
            Some(key) => format!("{}: {}", self.text(key), self.text(&item.value)),
            None => format!("**: {}", self.text(&item.value)),
        });
        let mutations = body[at + 1..]
            .iter()
            .map_while(|stmt| Self::mutation(stmt, name))
            .map(|(key, value)| format!("{}: {}", self.text(key), self.text_in_statement(value)));
        let suggestion = truncated(literal.chain(mutations));
        let message = format!(
            "Declare all known key/values when initializing the dictionary: {name} = {suggestion}"
        );
        self.out
            .push(self.ctx.report(DICT_INIT, &body[at], message));
    }

    // R3501
    fn while_condition(&mut self, stmt: &'a Stmt, test: &'a Expr, body: &'a [Stmt]) {
        if !truthy_const(test) {
            return;
        }
        let leading = body.iter().map_while(AstroidIf::of);
        let mut candidates: Vec<AstroidIf<'a>> = Vec::new();
        let mut tainted = false;
        for node in leading {
            if tainted || !matches!(node.body.first(), Some(Stmt::Break(_))) {
                break;
            }
            candidates.push(node);
            let mut current = node;
            loop {
                match current.orelse() {
                    Else::Empty => break,
                    Else::Other => {
                        tainted = true;
                        break;
                    }
                    Else::If(next, _) => {
                        candidates.push(next);
                        current = next;
                    }
                }
            }
        }
        candidates.retain(|node| matches!(node.body.first(), Some(Stmt::Break(_))));
        let msg = match candidates.as_slice() {
            [] => return,
            [one] => self.not_condition(one.test),
            many => many
                .iter()
                .map(|node| format!("({})", self.not_condition(node.test)))
                .collect::<Vec<_>>()
                .join(" and "),
        };
        let message = format!(
            "Consider using 'while {msg}' instead of 'while {}:' an 'if', and a 'break'",
            self.text_in_statement(test)
        );
        self.out
            .push(self.ctx.report(WHILE_CONDITION, stmt, message));
    }

    /// Pylint's `not_condition_as_string`.
    fn not_condition(&self, test: &Expr) -> String {
        match test {
            Expr::UnaryOp(unary) => self.text(&unary.operand),
            Expr::BoolOp(_) => format!("not ({})", self.text(test)),
            Expr::Compare(compare) => {
                let lower = |e: &Expr| {
                    let printed = self.text(e);
                    if matches!(
                        e,
                        Expr::Lambda(_)
                            | Expr::UnaryOp(_)
                            | Expr::BoolOp(_)
                            | Expr::If(_)
                            | Expr::Named(_)
                    ) {
                        format!("({printed})")
                    } else {
                        printed
                    }
                };
                let inverse = match compare.ops[0] {
                    CmpOp::Eq => "!=",
                    CmpOp::NotEq => "==",
                    CmpOp::Lt => ">=",
                    CmpOp::Gt => "<=",
                    CmpOp::LtE => ">",
                    CmpOp::GtE => "<",
                    CmpOp::In => "not in",
                    CmpOp::NotIn => "in",
                    CmpOp::Is => "is not",
                    CmpOp::IsNot => "is",
                };
                format!(
                    "{} {inverse} {}",
                    lower(&compare.operands[0]),
                    lower(&compare.operands[1])
                )
            }
            _ => format!("not {}", self.text_in_statement(test)),
        }
    }

    // R5601
    fn confusing_elif(&mut self, stmt: &'a Stmt) {
        let mut node = AstroidIf::of(stmt);
        while let Some(current) = node {
            if let Else::If(next, true) = current.orelse()
                && current
                    .body
                    .last()
                    .and_then(AstroidIf::of)
                    .is_some_and(AstroidIf::has_no_else)
            {
                self.out.push(self.ctx.report_at(
                    CONFUSING_ELIF,
                    next.range,
                    "Consecutive elif with differing indentation level, consider creating a function to separate the inner elif",
                ));
            }
            node = current.elif();
        }
    }

    // R6106
    fn math_not_float(&mut self, call: &'a Expr) {
        let Expr::Call(inner) = call else { return };
        let Some(env) = &self.math else { return };
        if !matches!(
            env.qualified(&inner.func).as_deref(),
            Some("float" | "builtins.float")
        ) {
            return;
        }
        let Some(Expr::StringLiteral(arg)) = inner.arguments.args.first() else {
            return;
        };
        let value = arg.value.to_str().to_lowercase();
        if !arg
            .value
            .to_str()
            .chars()
            .any(|c| c.is_alphabetic() && c.to_lowercase().ne(['e']))
        {
            return;
        }
        let name = if value.contains("nan") {
            "nan"
        } else if value.contains("inf") {
            "inf"
        } else {
            closest(&value)
        };
        let minus = if name == "inf" && value.starts_with('-') {
            "-"
        } else {
            ""
        };
        let message = format!("Consider {minus}math.{name} instead of {}", self.text(call));
        self.out
            .push(self.ctx.report(MATH_NOT_FLOAT, call, message));
    }
}

/// Pylint's `truncated_dict_suggestion`: the items up to 64 characters, in
/// braces.
fn truncated(items: impl Iterator<Item = String>) -> String {
    let mut kept: Vec<String> = Vec::new();
    for item in items {
        if !kept.is_empty() && kept.join(", ").chars().count() >= 64 {
            break;
        }
        kept.push(item);
    }
    let suggestion = kept.join(", ");
    let more = if suggestion.chars().count() > 64 {
        ", ... "
    } else {
        ""
    };
    format!("{{{suggestion}{more}}}")
}

/// Whether `bool(value)` is true for a constant `while` test.
fn truthy_const(test: &Expr) -> bool {
    match test {
        Expr::BooleanLiteral(b) => b.value,
        Expr::EllipsisLiteral(_) => true,
        Expr::StringLiteral(s) => !s.value.to_str().is_empty(),
        Expr::BytesLiteral(b) => b.value.bytes().next().is_some(),
        Expr::NumberLiteral(n) => match &n.value {
            Number::Int(i) => i.to_string() != "0",
            Number::Float(f) => *f != 0.0,
            Number::Complex { real, imag } => *real != 0.0 || *imag != 0.0,
        },
        _ => false,
    }
}

/// `difflib.get_close_matches(value, ["inf", "nan"], n=1, cutoff=0)[0]`.
fn closest(value: &str) -> &'static str {
    let value: Vec<char> = value.chars().collect();
    let score = |word: &str| matching(&word.chars().collect::<Vec<_>>(), &value);
    // equal ratios tie to the greater word
    if score("nan") >= score("inf") {
        "nan"
    } else {
        "inf"
    }
}

/// The characters `difflib.SequenceMatcher` matches between `a` and `b`.
fn matching(a: &[char], b: &[char]) -> usize {
    let (mut at, mut from, mut size) = (0, 0, 0);
    for i in 0..a.len() {
        for j in 0..b.len() {
            let run = a[i..]
                .iter()
                .zip(&b[j..])
                .take_while(|(x, y)| x == y)
                .count();
            if run > size {
                (at, from, size) = (i, j, run);
            }
        }
    }
    if size == 0 {
        return 0;
    }
    size + matching(&a[..at], &b[..from]) + matching(&a[at + size..], &b[from + size..])
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        if self.on.contains(&DICT_INIT) {
            for at in 0..body.len() {
                self.dict_init(body, at);
            }
        }
        visitor::walk_body(self, body);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::While(node) => {
                if self.on.contains(&WHILE_USED) {
                    self.out
                        .push(self.ctx.report(WHILE_USED, stmt, "Used `while` loop"));
                }
                if self.on.contains(&WHILE_CONDITION) {
                    self.while_condition(stmt, &node.test, &node.body);
                }
            }
            Stmt::If(_) if self.on.contains(&CONFUSING_ELIF) => self.confusing_elif(stmt),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if matches!(expr, Expr::Call(_)) {
            self.math_not_float(expr);
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const ALL: &[&str] = &["W0149", "C3401", "R3501", "R5601", "R6106"];
    const PLUGINS: &str = "pylint.extensions.while_used, pylint.extensions.dict_init_mutate, \
        pylint.extensions.consider_refactoring_into_while_condition, \
        pylint.extensions.confusing_elif, pylint.extensions.code_style";

    fn run(source: &str) -> String {
        render(source, &[("load-plugins", PLUGINS)], ALL)
    }

    /// The fixture's findings are Pylint 4.1.3's own, with every extension loaded.
    macro_rules! fixture {
        ($name:ident) => {
            #[test]
            fn $name() {
                let source =
                    include_str!(concat!("fixtures/extensions_", stringify!($name), ".py"));
                let expected = include_str!(concat!(
                    "fixtures/extensions_",
                    stringify!($name),
                    ".expected"
                ));
                assert_eq!(run(source), expected.trim_end());
            }
        };
    }

    fixture!(loops);
    fixture!(dict_init);
    fixture!(elif);
    fixture!(math_float);
    fixture!(blocks);
    fixture!(as_string);

    #[test]
    fn silent_without_the_extension() {
        let source = "while True:\n    if x:\n        break\nd = {}\nd[1] = 2\nfloat('inf')\n";
        assert_eq!(render(source, &[], ALL), "");
        let other = [("load-plugins", "pylint.extensions.docparams")];
        assert_eq!(render(source, &other, ALL), "");
        let only = [("load-plugins", "pylint.extensions.while_used")];
        assert_eq!(render(source, &only, ALL), "1:W0149:Used `while` loop");
    }

    #[test]
    fn math_not_float_needs_the_builtin() {
        let source = "def float(x):\n    return x\n\n\nfloat('inf')\n";
        assert_eq!(run(source), "");
        let source = "import builtins\nbuiltins.float('inf')\n";
        assert_eq!(
            run(source),
            "2:R6106:Consider math.inf instead of builtins.float('inf')"
        );
    }
}
