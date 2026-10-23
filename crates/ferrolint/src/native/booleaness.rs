//! Comparisons that a truth test replaces and the literals that cannot be
//! used where they sit: `C1803` use-implicit-booleaness-not-comparison,
//! `C1804` use-implicit-booleaness-not-comparison-to-string, `C1805`
//! use-implicit-booleaness-not-comparison-to-zero, `E1143` unhashable-member
//! and `E1144` invalid-slice-step.
//!
//! shortcut: Pylint infers the type of the compared or subscripted value.
//! Here a value has a type only when it is written as a list, tuple, dict,
//! set, string or bytes literal, is a call of the builtin `list`, `tuple`,
//! `dict`, `set` or `str`, or is a name whose every binding is a plain or
//! annotated assignment (or an element of a tuple unpacking) to one of
//! those, all of the same type. Anything else stays silent; upgrade with an
//! inference of values.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    CmpOp, Comprehension, Expr, ExprCall, ExprCompare, ExprContext, ExprDict, ExprSet,
    ExprSubscript, Number, Stmt, UnaryOp,
};

use super::astring::as_string;
use super::classes::Env;
use super::inferred::assigned_values;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const NOT_COMPARISON: &str = "C1803";
const TO_STRING: &str = "C1804";
const TO_ZERO: &str = "C1805";
const UNHASHABLE: &str = "E1143";
const SLICE_STEP: &str = "E1144";

pub struct Booleaness;

impl Checker for Booleaness {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                NOT_COMPARISON,
                "use-implicit-booleaness-not-comparison",
                "A comparison of a value with an empty list, tuple or dict literal.",
            ),
            rule(
                TO_STRING,
                "use-implicit-booleaness-not-comparison-to-string",
                "An `==`, `!=`, `is` or `is not` comparison with an empty string; off unless enabled.",
            ),
            rule(
                TO_ZERO,
                "use-implicit-booleaness-not-comparison-to-zero",
                "An `==`, `!=`, `is` or `is not` comparison with 0; off unless enabled.",
            ),
            rule(
                UNHASHABLE,
                "unhashable-member",
                "A list, dict or set used as a dict key or set member.",
            ),
            rule(
                SLICE_STEP,
                "invalid-slice-step",
                "A slice with a step of 0 on a sequence.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let on: Vec<&str> = [NOT_COMPARISON, TO_STRING, TO_ZERO, UNHASHABLE, SLICE_STEP]
            .into_iter()
            .filter(|id| enabled.contains(*id))
            .collect();
        if on.is_empty() {
            return;
        }
        let body = &ctx.parsed.syntax().body;
        Walk {
            env: Env::new(ctx),
            assigned: assigned_values(body),
            on,
            boolean: false,
            out,
        }
        .visit_body(body);
    }
}

/// What a value is known to be.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    List,
    Tuple,
    Dict,
    Set,
    Str,
    Bytes,
}

struct Walk<'a, 'o> {
    env: Env<'a>,
    /// The value of each name written as an assignment target, by node id.
    assigned: HashMap<u32, &'a Expr>,
    on: Vec<&'a str>,
    /// Whether the expression visited next is used for its truth.
    boolean: bool,
    out: &'o mut Vec<Diagnostic>,
}

/// Whether `expr` is the number 0 (`0`, `0.0` or `0j`, not `False`).
fn is_zero(expr: &Expr) -> bool {
    let Expr::NumberLiteral(number) = expr else {
        return false;
    };
    match &number.value {
        Number::Int(int) => int.as_u8() == Some(0),
        Number::Float(float) => *float == 0.0,
        Number::Complex { real, imag } => *real == 0.0 && *imag == 0.0,
    }
}

/// The operand beside the one `matches`, the left one tried first.
fn opposite<'e>(
    left: &'e Expr,
    right: &'e Expr,
    matches: impl Fn(&Expr) -> bool,
) -> Option<&'e Expr> {
    if matches(left) {
        Some(right)
    } else {
        matches(right).then_some(left)
    }
}

/// The empty list, tuple or dict literal's `(description, source)`.
fn empty_literal(expr: &Expr) -> Option<(&'static str, &'static str)> {
    match expr {
        Expr::List(list) if list.elts.is_empty() => Some(("list", "[]")),
        Expr::Tuple(tuple) if tuple.elts.is_empty() => Some(("tuple", "()")),
        Expr::Dict(dict) if dict.items.is_empty() => Some(("dict", "{}")),
        _ => None,
    }
}

/// `name` as Pylint suggests it for a comparison: `not name`, or for `!=`
/// (and `is not` where the message has it) `name` in a truth test and
/// `bool(name)` elsewhere.
fn suggestion(name: &str, redundant: bool, boolean: bool) -> String {
    match (redundant, boolean) {
        (false, _) => format!("not {name}"),
        (true, true) => name.to_owned(),
        (true, false) => format!("bool({name})"),
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.on.contains(&id)
    }

    fn text(&self, expr: &Expr) -> String {
        as_string(self.env.ctx.source, expr)
    }

    fn kind(&self, expr: &'a Expr, depth: u8) -> Option<Kind> {
        match expr {
            Expr::List(_) => Some(Kind::List),
            Expr::Tuple(_) => Some(Kind::Tuple),
            Expr::Dict(_) => Some(Kind::Dict),
            Expr::Set(_) => Some(Kind::Set),
            Expr::StringLiteral(_) => Some(Kind::Str),
            Expr::BytesLiteral(_) => Some(Kind::Bytes),
            Expr::Call(call) => match self.env.qualified(&call.func)?.as_str() {
                "list" => Some(Kind::List),
                "tuple" => Some(Kind::Tuple),
                "dict" => Some(Kind::Dict),
                "set" => Some(Kind::Set),
                "str" => Some(Kind::Str),
                _ => None,
            },
            Expr::Name(name) if depth > 0 => {
                // Pylint infers every binding of the name and needs one type.
                let mut kinds = self
                    .env
                    .ctx
                    .model
                    .same_name(self.env.binding(name)?.id)
                    .into_iter()
                    .map(|b| {
                        matches!(b.kind, BindingKind::Assignment | BindingKind::Annotation)
                            .then_some(())
                            .and_then(|()| self.kind(self.assigned.get(&b.node?)?, depth - 1))
                    });
                let first = kinds.next()??;
                kinds.all(|k| k == Some(first)).then_some(first)
            }
            _ => None,
        }
    }

    fn unhashable(&self, expr: &'a Expr) -> bool {
        matches!(
            self.kind(expr, 4),
            Some(Kind::List | Kind::Dict | Kind::Set)
        )
    }

    /// A subexpression used for its truth.
    fn test(&mut self, expr: &'a Expr) {
        self.boolean = true;
        self.visit_expr(expr);
    }

    fn compare(&mut self, expr: &'a Expr, compare: &'a ExprCompare, boolean: bool) {
        let ([op], [left, right]) = (&*compare.ops, &*compare.operands) else {
            return;
        };
        let ctx = self.env.ctx;
        // C1803
        if self.on(NOT_COMPARISON)
            && matches!(
                op,
                CmpOp::Eq | CmpOp::NotEq | CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE
            )
            && let Some(((description, literal), target)) =
                match (empty_literal(left), empty_literal(right)) {
                    (Some(found), None) => Some((found, right)),
                    (None, Some(found)) => Some((found, left)),
                    _ => None,
                }
            && self.kind(target, 4).is_some()
        {
            let name = match target {
                Expr::Call(call) => format!("{}(...)", self.text(&call.func)),
                Expr::Name(_) | Expr::Attribute(_) => self.text(target),
                _ => "x".to_owned(),
            };
            let operator = op.as_str();
            let message = format!(
                "\"{name} {operator} {literal}\" can be simplified to \"{}\", if it is strictly a sequence, as an empty {description} is falsey",
                suggestion(&name, *op == CmpOp::NotEq, boolean)
            );
            self.out.push(ctx.report(NOT_COMPARISON, expr, message));
        }
        if !matches!(op, CmpOp::Eq | CmpOp::NotEq | CmpOp::Is | CmpOp::IsNot) {
            return;
        }
        let redundant = matches!(op, CmpOp::NotEq | CmpOp::IsNot);
        // C1805
        if self.on(TO_ZERO)
            && let Some(operand) = opposite(left, right, is_zero)
        {
            let message = format!(
                "\"{} {} {}\" can be simplified to \"{}\", if it is strictly an int, as 0 is falsey",
                self.text(left),
                op.as_str(),
                self.text(right),
                suggestion(&self.text(operand), redundant, boolean)
            );
            self.out.push(ctx.report(TO_ZERO, expr, message));
        }
        // C1804
        let is_empty =
            |e: &Expr| matches!(e, Expr::StringLiteral(s) if s.value.to_str().is_empty());
        if self.on(TO_STRING)
            && let Some(operand) = opposite(left, right, is_empty)
        {
            let message = format!(
                "\"{}\" can be simplified to \"{}\", if it is strictly a string, as an empty string is falsey",
                self.text(expr),
                suggestion(&self.text(operand), redundant, boolean)
            );
            self.out.push(ctx.report(TO_STRING, expr, message));
        }
    }

    // E1143 and E1144
    fn subscript(&mut self, subscript: &'a ExprSubscript) {
        let ctx = self.env.ctx;
        if self.on(UNHASHABLE)
            && let Expr::Dict(_) = &*subscript.value
            && self.unhashable(&subscript.slice)
        {
            let message = format!(
                "'{}' is unhashable and can't be used as a key in a dict",
                self.text(&subscript.slice)
            );
            self.out
                .push(ctx.report(UNHASHABLE, &*subscript.value, message));
        }
        if !self.on(SLICE_STEP) {
            return;
        }
        let Expr::Slice(slice) = &*subscript.slice else {
            return;
        };
        let Some(step) = slice.step.as_deref() else {
            return;
        };
        // `False` is a zero too
        if !(is_zero(step) || matches!(step, Expr::BooleanLiteral(b) if !b.value)) {
            return;
        }
        // only a list takes a slice assigned or deleted
        let sequence = match self.kind(&subscript.value, 4) {
            Some(Kind::List) => true,
            Some(Kind::Tuple | Kind::Str | Kind::Bytes) => subscript.ctx == ExprContext::Load,
            _ => false,
        };
        if sequence {
            self.out
                .push(ctx.report(SLICE_STEP, step, "Slice step cannot be 0"));
        }
    }

    fn unhashable_member(&mut self, member: &'a Expr, role: &str, container: &str) {
        if self.unhashable(member) {
            let message = format!(
                "'{}' is unhashable and can't be used as a {role} in a {container}",
                self.text(member)
            );
            self.out
                .push(self.env.ctx.report(UNHASHABLE, member, message));
        }
    }

    fn dict(&mut self, dict: &'a ExprDict) {
        for key in dict.items.iter().filter_map(|item| item.key.as_ref()) {
            self.unhashable_member(key, "key", "dict");
        }
    }

    fn set(&mut self, set: &'a ExprSet) {
        for member in &set.elts {
            self.unhashable_member(member, "member", "set");
        }
    }

    /// The arguments of a call, where `bool(...)`, `any(...)`/`all(...)` of a
    /// generator and `filter(...)` of a lambda use what they take for its truth.
    fn call(&mut self, call: &'a ExprCall) {
        self.visit_expr(&call.func);
        let callee = match &*call.func {
            Expr::Name(name) => name.id.as_str(),
            _ => "",
        };
        for arg in &call.arguments.args {
            match (callee, arg) {
                ("bool", _) => self.test(arg),
                ("any" | "all", Expr::Generator(generator)) => {
                    for comprehension in &generator.generators {
                        self.visit_comprehension(comprehension);
                    }
                    self.test(&generator.elt);
                }
                ("filter", Expr::Lambda(lambda)) => {
                    if let Some(parameters) = &lambda.parameters {
                        self.visit_parameters(parameters);
                    }
                    self.test(&lambda.body);
                }
                _ => self.visit_expr(arg),
            }
        }
        for keyword in &call.arguments.keywords {
            self.visit_expr(&keyword.value);
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::If(node) => {
                self.test(&node.test);
                self.visit_body(&node.body);
                for clause in &node.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        self.test(test);
                    }
                    self.visit_body(&clause.body);
                }
            }
            Stmt::While(node) => {
                self.test(&node.test);
                self.visit_body(&node.body);
                self.visit_body(&node.orelse);
            }
            Stmt::Assert(node) => {
                self.test(&node.test);
                if let Some(msg) = &node.msg {
                    self.visit_expr(msg);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        self.visit_expr(&comprehension.iter);
        self.visit_expr(&comprehension.target);
        for condition in &comprehension.ifs {
            self.test(condition);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        let boolean = std::mem::take(&mut self.boolean);
        match expr {
            Expr::Compare(compare) => self.compare(expr, compare, boolean),
            Expr::Subscript(subscript) => self.subscript(subscript),
            Expr::Dict(dict) if self.on(UNHASHABLE) => self.dict(dict),
            Expr::Set(set) if self.on(UNHASHABLE) => self.set(set),
            _ => {}
        }
        match expr {
            // an operand of `and`/`or` is used for its truth when the operation is
            Expr::BoolOp(operation) => {
                for value in &operation.values {
                    self.boolean = boolean;
                    self.visit_expr(value);
                }
            }
            Expr::UnaryOp(unary) if unary.op == UnaryOp::Not => self.test(&unary.operand),
            Expr::If(ternary) => {
                self.test(&ternary.test);
                self.visit_expr(&ternary.body);
                self.visit_expr(&ternary.orelse);
            }
            Expr::Call(call) => self.call(call),
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    const ALL: &[&str] = &["C1803", "C1804", "C1805", "E1143", "E1144"];

    /// The fixtures' findings are Pylint 4.1.3's own, with every message enabled.
    macro_rules! fixture {
        ($name:ident) => {
            #[test]
            fn $name() {
                let source =
                    include_str!(concat!("fixtures/booleaness_", stringify!($name), ".py"));
                let expected = include_str!(concat!(
                    "fixtures/booleaness_",
                    stringify!($name),
                    ".expected"
                ));
                assert_eq!(render(source, &[], ALL), expected.trim_end());
            }
        };
    }

    fixture!(comparison);
    fixture!(zero_string);
    fixture!(unhashable);
    fixture!(slice_step);
}
