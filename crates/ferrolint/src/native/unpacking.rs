//! `W0632` unbalanced-tuple-unpacking and `W0644` unbalanced-dict-unpacking:
//! an assignment `a, b = value` or a loop `for a, b in value.items()` whose
//! number of targets differs from the number of values that `value` holds.
//!
//! shortcut: Pylint infers `value`. Here it is a tuple, list, set or dict
//! display, a name bound once to one, a call of a function of the file with
//! one `return` of one, or `.keys()`, `.values()` and `.items()` of a dict
//! display so reached, or a slice of the `*args` of the enclosing function,
//! which holds no value; any other value is not judged. Upgrade with inference.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, ExprCall, ExprDict, Number, Stmt, StmtAssign, StmtClassDef, StmtFor, StmtFunctionDef,
};
use ruff_text_size::Ranged;

use super::abstract_methods::class_is_abstract;
use super::astring::as_string;
use super::calls::function_called;
use super::classes::Env;
use super::flow::own_returns;
use super::hierarchy::Hierarchy;
use super::inferred::assigned_values;
use super::statements::is_generator;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const TUPLE: &str = "W0632";
const DICT: &str = "W0644";

/// How far a name or a call is followed to the display it holds.
const DEPTH: u8 = 8;

pub struct Unpacking;

impl Checker for Unpacking {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                TUPLE,
                "unbalanced-tuple-unpacking",
                "An unpacking of a tuple, list or set display into another number of names.",
            ),
            rule(
                DICT,
                "unbalanced-dict-unpacking",
                "An unpacking of a dict display, or of its keys, values or items, into another number of names.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(TUPLE) && !enabled.contains(DICT) {
            return;
        }
        let env = Env::new(ctx);
        let hierarchy = Hierarchy::new(&env);
        let body = &ctx.parsed.syntax().body;
        let mut walk = Walk {
            env: &env,
            hierarchy: &hierarchy,
            enabled,
            out,
            assigned: assigned_values(body),
            classes: Vec::new(),
            functions: Vec::new(),
        };
        walk.visit_body(body);
    }
}

/// What a value is, for the count of the names it unpacks into.
enum Shape<'a> {
    /// A tuple, list or set display.
    /// `None` for the empty tuple of a sliced `*args`.
    Sequence(Option<&'a Expr>, usize),
    /// A dict display, or one of its views.
    Dict(View, &'a ExprDict),
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Whole,
    Keys,
    Values,
    Items,
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    hierarchy: &'e Hierarchy<'a, 'e>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    assigned: HashMap<u32, &'a Expr>,
    /// The enclosing classes, with whether each is abstract.
    classes: Vec<(&'a StmtClassDef, bool)>,
    /// The enclosing functions, innermost last.
    functions: Vec<&'a StmtFunctionDef>,
}

/// Whether a dict display has only `key: value` items, no `**other`.
fn plain(dict: &ExprDict) -> bool {
    dict.items.iter().all(|item| item.key.is_some())
}

/// The number of names a display holds, if it is one that is counted.
fn count(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Tuple(t) => Some(t.elts.len()),
        Expr::List(l) => Some(l.elts.len()),
        Expr::Set(s) => Some(s.elts.len()),
        Expr::Dict(d) => Some(d.items.len()),
        _ => None,
    }
}

/// Pylint's `_get_value_length`: the values `expr` unpacks into.
fn length(expr: &Expr) -> i64 {
    let size = |n: usize| i64::try_from(n).unwrap_or(i64::MAX);
    if let Some(n) = count(expr) {
        return size(n);
    }
    match expr {
        Expr::StringLiteral(s) => size(s.value.to_str().chars().count()),
        Expr::BytesLiteral(b) => size(b.value.bytes().count()),
        Expr::Subscript(s) => match &*s.slice {
            Expr::Slice(slice) => {
                let int = |e: Option<&Expr>| match e {
                    Some(Expr::NumberLiteral(n)) => match &n.value {
                        Number::Int(i) => i.as_i64(),
                        _ => None,
                    },
                    _ => None,
                };
                match (
                    int(slice.lower.as_deref()),
                    int(slice.upper.as_deref()),
                    slice.step.as_deref(),
                ) {
                    (Some(lower), Some(upper), None) => upper - lower,
                    // a step that is 0 or not an int counts as one value
                    (Some(lower), Some(upper), step) => match int(step) {
                        Some(step @ 1..) => (upper - lower + step - 1).div_euclid(step),
                        _ => 1,
                    },
                    _ => 1,
                }
            }
            _ => 1,
        },
        _ => 1,
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn line(&self, expr: &impl Ranged) -> u32 {
        self.env.ctx.index.line(expr.start().to_u32())
    }

    /// The display `expr` stands for, when Pylint infers a single one.
    fn display(&self, expr: &'a Expr, depth: u8) -> Option<&'a Expr> {
        if depth == 0 {
            return None;
        }
        match expr {
            Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) | Expr::Dict(_) => Some(expr),
            Expr::Name(name) => {
                let binding = self.env.binding(name)?;
                if binding.kind != BindingKind::Assignment
                    || !self.env.ctx.model.is_sole_binding(binding.id)
                {
                    return None;
                }
                self.display(self.assigned.get(&binding.node?)?, depth - 1)
            }
            Expr::Call(call) => {
                let func = function_called(self.env, &self.classes, call)?;
                if func.is_async || is_generator(func) {
                    return None;
                }
                match own_returns(&func.body)[..] {
                    [ret] => self.display(ret.value.as_deref()?, depth - 1),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// Whether `expr` slices the `*args` of the innermost function: Pylint
    /// infers an empty tuple, whatever the bounds.
    fn sliced_varargs(&self, expr: &Expr) -> bool {
        let Expr::Subscript(sub) = expr else {
            return false;
        };
        let (Expr::Name(name), Expr::Slice(_), Some(func)) =
            (&*sub.value, &*sub.slice, self.functions.last())
        else {
            return false;
        };
        func.parameters
            .vararg
            .as_ref()
            .is_some_and(|v| v.name.as_str() == name.id.as_str())
            && self
                .env
                .binding(name)
                .is_some_and(|b| b.kind == BindingKind::Parameter)
    }

    fn shape(&self, expr: &'a Expr) -> Option<Shape<'a>> {
        if self.sliced_varargs(expr) {
            return Some(Shape::Sequence(None, 0));
        }
        if let Expr::Call(ExprCall {
            func, arguments, ..
        }) = expr
            && let Expr::Attribute(attribute) = &**func
            && arguments.is_empty()
            && let Some(Expr::Dict(dict)) = self.display(&attribute.value, DEPTH)
            && plain(dict)
        {
            let view = match attribute.attr.as_str() {
                "keys" => View::Keys,
                "values" => View::Values,
                "items" => View::Items,
                _ => return self.sequence(expr),
            };
            return Some(Shape::Dict(view, dict));
        }
        self.sequence(expr)
    }

    fn sequence(&self, expr: &'a Expr) -> Option<Shape<'a>> {
        let display = self.display(expr, DEPTH)?;
        match display {
            Expr::Dict(dict) if plain(dict) => Some(Shape::Dict(View::Whole, dict)),
            Expr::Tuple(_) | Expr::List(_) | Expr::Set(_) => {
                let elts = match display {
                    Expr::Tuple(t) => &t.elts,
                    Expr::List(l) => &l.elts,
                    Expr::Set(s) => &s.elts,
                    _ => return None,
                };
                // astroid expands a starred element when it infers the display
                (!elts.iter().any(Expr::is_starred_expr))
                    .then_some(Shape::Sequence(Some(display), elts.len()))
            }
            _ => None,
        }
    }

    fn assign(&mut self, assign: &'a StmtAssign) {
        let targets = match assign.targets.first() {
            Some(Expr::Tuple(t)) => &t.elts,
            Some(Expr::List(l)) => &l.elts,
            _ => return,
        };
        if targets.iter().any(Expr::is_starred_expr)
            || self.classes.iter().any(|(_, abstract_)| *abstract_)
        {
            return;
        }
        let ctx = self.env.ctx;
        let Some(shape) = self.shape(&assign.value) else {
            return;
        };
        let (id, details, values) = match shape {
            Shape::Sequence(display, values) => {
                let details = match display {
                    None => String::new(),
                    Some(d) if self.line(d) == self.line(assign) => {
                        format!("'{}'", as_string(ctx.source, d))
                    }
                    Some(d) => format!("defined at line {}", self.line(d)),
                };
                (TUPLE, format!("sequence {details}"), values)
            }
            Shape::Dict(_, dict) => (DICT, as_string(ctx.source, &assign.value), dict.items.len()),
        };
        if targets.len() != values && self.on(id) {
            let message = unbalanced(id, &details, targets.len(), values);
            self.out.push(ctx.report(id, assign, message));
        }
    }

    fn for_loop(&mut self, node: &'a StmtFor) {
        let Expr::Tuple(target) = &*node.target else {
            return;
        };
        let Some(Shape::Dict(view @ (View::Keys | View::Values | View::Items), dict)) =
            self.shape(&node.iter)
        else {
            return;
        };
        if dict.items.is_empty() || !self.on(DICT) {
            return;
        }
        let starred = target.elts.iter().any(Expr::is_starred_expr);
        let wrong = if view == View::Items {
            // each item unpacks as a pair; a starred target is ambiguous
            (target.elts.len() != 2 && !starred).then_some(2)
        } else {
            dict.items.iter().find_map(|item| {
                let value = if view == View::Keys {
                    item.key.as_ref()?
                } else {
                    &item.value
                };
                let length = length(value);
                let targets = i64::try_from(target.elts.len()).unwrap_or(i64::MAX);
                let valid_star = starred && length >= targets;
                (targets != length && !valid_star).then_some(length)
            })
        };
        if let Some(values) = wrong {
            let ctx = self.env.ctx;
            let details = as_string(ctx.source, &node.iter);
            let values = usize::try_from(values).unwrap_or(0);
            let message = unbalanced(DICT, &details, target.elts.len(), values);
            self.out.push(ctx.report(DICT, node, message));
        }
    }
}

fn unbalanced(id: &str, details: &str, targets: usize, values: usize) -> String {
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    let kind = if id == TUPLE { "tuple" } else { "dict" };
    format!(
        "Possible unbalanced {kind} unpacking with {details}: left side has {targets} label{}, right side has {values} value{}",
        plural(targets),
        plural(values)
    )
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                let abstract_ = class_is_abstract(self.env, self.hierarchy, class);
                self.classes.push((class, abstract_));
                visitor::walk_stmt(self, stmt);
                self.classes.pop();
                return;
            }
            Stmt::FunctionDef(func) => {
                self.functions.push(func);
                visitor::walk_stmt(self, stmt);
                self.functions.pop();
                return;
            }
            Stmt::Assign(assign) => self.assign(assign),
            Stmt::For(node) if !node.is_async => self.for_loop(node),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    #[test]
    fn unbalanced_unpacking() {
        let source = include_str!("fixtures/unbalanced_unpacking.py");
        let expected = include_str!("fixtures/unbalanced_unpacking.expected").trim_end();
        assert_eq!(render(source, &[], &["W0632", "W0644"]), expected);
    }
}
