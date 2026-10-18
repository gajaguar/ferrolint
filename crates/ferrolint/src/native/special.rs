//! Special methods that return the wrong kind of value: `E0301`
//! non-iterator-returned, `E0306` invalid-repr-returned, `E0310`
//! invalid-length-hint-returned, `E0311` invalid-format-returned, `E0312`
//! invalid-getnewargs-returned and `E0313` invalid-getnewargs-ex-returned.
//!
//! Pylint judges the one value a method returns, inferred. shortcut: here
//! that value is known only when it is a literal (a constant, a display, a
//! lambda, `NotImplemented`), a signed number, an f-string with no
//! interpolation, `self` of a class that derives from nothing but this file's
//! classes, or a generator; a name, a call or an operation is unknown and
//! reports nothing. Upgrade with a value inference.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Expr, FStringPartRef, Number, Stmt, StmtClassDef, StmtFunctionDef, StmtReturn, UnaryOp,
};
use ruff_text_size::Ranged;

use super::classes::{Env, is_classmethod, is_static, methods};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const ITER: &str = "E0301";
const REPR: &str = "E0306";
const LENGTH_HINT: &str = "E0310";
const FORMAT: &str = "E0311";
const GETNEWARGS: &str = "E0312";
const GETNEWARGS_EX: &str = "E0313";

pub struct Special;

impl Checker for Special {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                ITER,
                "non-iterator-returned",
                "`__iter__` returns something with no `__next__`.",
            ),
            rule(
                REPR,
                "invalid-repr-returned",
                "`__repr__` returns something that is not a string.",
            ),
            rule(
                LENGTH_HINT,
                "invalid-length-hint-returned",
                "`__length_hint__` returns something that is not a non-negative integer.",
            ),
            rule(
                FORMAT,
                "invalid-format-returned",
                "`__format__` returns something that is not a string.",
            ),
            rule(
                GETNEWARGS,
                "invalid-getnewargs-returned",
                "`__getnewargs__` returns something that is not a tuple.",
            ),
            rule(
                GETNEWARGS_EX,
                "invalid-getnewargs-ex-returned",
                "`__getnewargs_ex__` returns something that is not a (tuple, dict) tuple.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [ITER, REPR, LENGTH_HINT, FORMAT, GETNEWARGS, GETNEWARGS_EX];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// What a method is known to return.
enum Val<'a> {
    None,
    Bool,
    /// An integer; `true` when it is negative.
    Int(bool),
    Float,
    Complex,
    Str,
    Bytes,
    Ellipsis,
    Tuple(&'a [Expr]),
    List,
    Set,
    Dict,
    Lambda,
    NotImplemented,
    /// `self` of a class with no foreign base; `true` when it has `__next__`.
    Instance(bool),
    Generator,
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
}

/// The `return` statements of a body, not those of nested functions.
struct Returns<'a>(Vec<&'a StmtReturn>);

impl<'a> Visitor<'a> for Returns<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Return(r) => self.0.push(r),
            Stmt::FunctionDef(_) => {}
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// Whether the body yields outside nested functions and lambdas.
struct Yields(bool);

impl<'a> Visitor<'a> for Yields {
    // Pylint does not look for a `yield` inside a `return`.
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if !matches!(stmt, Stmt::FunctionDef(_) | Stmt::Return(_)) {
            visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(_) | Expr::YieldFrom(_) => self.0 = true,
            Expr::Lambda(_) => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn is_docstring(stmt: &Stmt) -> bool {
    matches!(stmt, Stmt::Expr(e) if matches!(&*e.value, Expr::StringLiteral(_)))
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// The value `method` returns, or `None` when Pylint's inference would
    /// not settle on one that this port can tell.
    fn returned(&self, class: &StmtClassDef, method: &'a StmtFunctionDef) -> Option<Val<'a>> {
        let mut yields = Yields(false);
        yields.visit_body(&method.body);
        if yields.0 {
            return Some(Val::Generator);
        }
        let mut returns = Returns(Vec::new());
        returns.visit_body(&method.body);
        match returns.0.as_slice() {
            // A body with no `return` gives `None`, unless it is a stub or
            // abstract (a placeholder, or starting with `raise`).
            [] => {
                let rest = match method.body.first() {
                    Some(first) if is_docstring(first) => &method.body[1..],
                    _ => &method.body[..],
                };
                let stub = matches!(rest, [Stmt::Pass(_)])
                    || matches!(rest, [Stmt::Expr(e)] if matches!(&*e.value, Expr::EllipsisLiteral(_)));
                let placeholder = matches!(rest.first(), Some(Stmt::Raise(_) | Stmt::Pass(_)))
                    || method.decorator_list.iter().any(|d| {
                        matches!(
                            self.env.qualified(&d.expression).as_deref(),
                            Some("abc.abstractmethod" | "abc.abstractproperty")
                        )
                    });
                (!rest.is_empty() && !stub && !placeholder).then_some(Val::None)
            }
            [only] => match &only.value {
                None => Some(Val::None),
                Some(value) => self.classify(class, method, value),
            },
            // Several returns make the inferred value ambiguous.
            _ => None,
        }
    }

    fn classify(
        &self,
        class: &StmtClassDef,
        method: &'a StmtFunctionDef,
        expr: &'a Expr,
    ) -> Option<Val<'a>> {
        Some(match expr {
            Expr::NoneLiteral(_) => Val::None,
            Expr::BooleanLiteral(_) => Val::Bool,
            Expr::NumberLiteral(n) => match n.value {
                Number::Int(_) => Val::Int(false),
                Number::Float(_) => Val::Float,
                Number::Complex { .. } => Val::Complex,
            },
            Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd) => {
                let negative = matches!(u.op, UnaryOp::USub);
                match &*u.operand {
                    Expr::NumberLiteral(n) => match &n.value {
                        Number::Int(i) => Val::Int(negative && i.as_u8() != Some(0)),
                        Number::Float(_) => Val::Float,
                        Number::Complex { .. } => Val::Complex,
                    },
                    Expr::BooleanLiteral(b) => Val::Int(negative && b.value),
                    _ => return None,
                }
            }
            Expr::StringLiteral(_) => Val::Str,
            // An f-string without interpolation is a constant to Pylint.
            Expr::FString(f) => {
                let constant = f.value.iter().all(|part| match part {
                    FStringPartRef::Literal(_) => true,
                    FStringPartRef::FString(s) => s.elements.interpolations().next().is_none(),
                });
                if !constant {
                    return None;
                }
                Val::Str
            }
            Expr::BytesLiteral(_) => Val::Bytes,
            Expr::EllipsisLiteral(_) => Val::Ellipsis,
            Expr::Tuple(t) if !t.elts.iter().any(|e| matches!(e, Expr::Starred(_))) => {
                Val::Tuple(&t.elts)
            }
            Expr::List(l) if !l.elts.iter().any(|e| matches!(e, Expr::Starred(_))) => Val::List,
            Expr::Set(s) if !s.elts.iter().any(|e| matches!(e, Expr::Starred(_))) => Val::Set,
            Expr::Dict(d) if d.items.iter().all(|item| item.key.is_some()) => Val::Dict,
            Expr::Lambda(_) => Val::Lambda,
            Expr::Name(name) => {
                let binding = self.env.binding(name)?;
                match binding.kind {
                    BindingKind::Builtin if name.id.as_str() == "NotImplemented" => {
                        Val::NotImplemented
                    }
                    BindingKind::Parameter
                        if self.env.ctx.model.is_sole_binding(binding.id)
                            && !is_static(method)
                            && !is_classmethod(method)
                            && method
                                .parameters
                                .posonlyargs
                                .iter()
                                .chain(&method.parameters.args)
                                .next()
                                .is_some_and(|first| {
                                    first.parameter.name.as_str() == name.id.as_str()
                                })
                            && self.env.plain(class, 0) =>
                    {
                        Val::Instance(self.env.defines(class, "__next__", 0))
                    }
                    _ => return None,
                }
            }
            _ => return None,
        })
    }

    fn method(&mut self, class: &StmtClassDef, method: &'a StmtFunctionDef) {
        let (id, text): (&str, &str) = match method.name.as_str() {
            "__iter__" => (ITER, "__iter__ returns non-iterator"),
            "__repr__" => (REPR, "__repr__ does not return str"),
            "__length_hint__" => (
                LENGTH_HINT,
                "__length_hint__ does not return non-negative integer",
            ),
            "__format__" => (FORMAT, "__format__ does not return str"),
            "__getnewargs__" => (GETNEWARGS, "__getnewargs__ does not return a tuple"),
            "__getnewargs_ex__" => (
                GETNEWARGS_EX,
                "__getnewargs_ex__ does not return a tuple containing (tuple, dict)",
            ),
            _ => return,
        };
        if !self.on(id) {
            return;
        }
        let Some(value) = self.returned(class, method) else {
            return;
        };
        let bad = match id {
            ITER => !matches!(value, Val::Generator | Val::Instance(true)),
            REPR | FORMAT => !matches!(value, Val::Str),
            LENGTH_HINT => !matches!(value, Val::Int(false) | Val::Bool),
            GETNEWARGS => !matches!(value, Val::Tuple(_)),
            _ => match value {
                Val::Tuple(items) => {
                    // Pylint infers a name or a call; any other item that is
                    // not the right display is an error.
                    items.len() != 2
                        || items.iter().enumerate().any(|(at, item)| match item {
                            Expr::Name(_) | Expr::Call(_) => false,
                            Expr::Tuple(_) => at != 0,
                            Expr::Dict(_) => at != 1,
                            _ => true,
                        })
                }
                _ => true,
            },
        };
        if bad {
            self.out
                .push(self.env.ctx.report_at(id, method.name.range(), text));
        }
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            for method in methods(&class.body) {
                self.method(class, method);
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    #[test]
    fn special_methods_returning_the_wrong_kind() {
        let source = include_str!("fixtures/special_methods_returning_the_wrong_kind.py");
        let expected =
            include_str!("fixtures/special_methods_returning_the_wrong_kind.expected").trim_end();
        assert_eq!(
            render(
                source,
                &[],
                &["E0301", "E0306", "E0310", "E0311", "E0312", "E0313"]
            ),
            expected
        );
    }
}
