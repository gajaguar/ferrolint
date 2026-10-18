//! Pylint's exceptions checker, for what Ruff lacks: `W0718`
//! broad-exception-caught and `W0719` broad-exception-raised (both read
//! `overgeneral-exceptions`), `E0702` raising-bad-type, `E0705`
//! bad-exception-cause, `E0710` raising-non-exception, `E0712`
//! catching-non-exception and `W0716` wrong-exception-operation.
//!
//! shortcut: Pylint infers what a `raise` or an `except` names. Here a name
//! is a class when it is a builtin, a class of the file or of the project
//! (found through `import`, also behind an alias `Name = Exception`), and
//! `raise` and `except` judge only that and the literals. `E0710`,
//! `E0712` and `E0705` need every base of the class to be known (a class of
//! the file or a builtin); a class with a base from an installed package is
//! not judged. `E0702` takes the type of a literal, a signed number, a
//! function of the file or a lambda; a name bound to a constant is not
//! followed. `W0716` needs both operands to be classes of the file or
//! builtins, or tuples written out. `E0712` leaves a `|` of classes alone.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{ExceptHandler, Expr, Number, Operator, Stmt, StmtRaise, StmtTry, UnaryOp};
use ruff_text_size::Ranged;

use super::classes::{Env, bases, const_repr};
use super::flow::builtin_exception;
use super::misc::csv;
use super::project::Project;
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const CAUGHT: &str = "W0718";
const RAISED: &str = "W0719";
const BAD_TYPE: &str = "E0702";
const BAD_CAUSE: &str = "E0705";
const NON_EXCEPTION: &str = "E0710";
const CATCHING: &str = "E0712";
const OPERATION: &str = "W0716";
const ALL: [&str; 7] = [
    CAUGHT,
    RAISED,
    BAD_TYPE,
    BAD_CAUSE,
    NON_EXCEPTION,
    CATCHING,
    OPERATION,
];

pub struct Exceptions;

impl Checker for Exceptions {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                CAUGHT,
                "broad-exception-caught",
                "An `except` of a class in `overgeneral-exceptions`.",
            ),
            rule(
                RAISED,
                "broad-exception-raised",
                "A `raise` of a class in `overgeneral-exceptions`.",
            ),
            rule(
                BAD_TYPE,
                "raising-bad-type",
                "A `raise` of something that is not a class or an instance.",
            ),
            rule(
                BAD_CAUSE,
                "bad-exception-cause",
                "A `raise ... from` cause that is not an exception nor `None`.",
            ),
            rule(
                NON_EXCEPTION,
                "raising-non-exception",
                "A `raise` of a class that does not derive from `BaseException`.",
            ),
            rule(
                CATCHING,
                "catching-non-exception",
                "An `except` of something that does not derive from `Exception`.",
            ),
            rule(
                OPERATION,
                "wrong-exception-operation",
                "An operation instead of a tuple in an `except`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !ALL.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut aliases = HashMap::new();
        for stmt in &ctx.parsed.syntax().body {
            if let Stmt::Assign(assign) = stmt
                && let [Expr::Name(target)] = &assign.targets[..]
                && matches!(&*assign.value, Expr::Name(_) | Expr::Attribute(_))
            {
                aliases.insert(target.id.as_str(), &*assign.value);
            }
        }
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            aliases,
            overgeneral: csv(
                ctx,
                "overgeneral-exceptions",
                "builtins.BaseException,builtins.Exception",
            ),
            project: Project::new(ctx.path, ctx.module),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Inherit {
    Yes,
    No,
    Unknown,
}

struct Walk<'a, 'c> {
    env: &'c Env<'a>,
    enabled: &'c BTreeSet<String>,
    out: &'c mut Vec<Diagnostic>,
    /// `Alias = Exception` at the top level.
    aliases: HashMap<&'a str, &'a Expr>,
    overgeneral: Vec<String>,
    project: Project,
}

/// The builtin classes that are not exceptions.
fn plain_builtin(name: &str) -> bool {
    matches!(
        name,
        "object"
            | "int"
            | "str"
            | "float"
            | "bool"
            | "bytes"
            | "bytearray"
            | "complex"
            | "list"
            | "dict"
            | "set"
            | "frozenset"
            | "tuple"
            | "type"
            | "range"
            | "slice"
            | "property"
            | "super"
            | "memoryview"
            | "staticmethod"
            | "classmethod"
            | "enumerate"
            | "zip"
            | "map"
            | "filter"
            | "reversed"
    )
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn report(&mut self, id: &str, node: &impl Ranged, message: String) {
        let diagnostic = self.env.ctx.report(id, node, message);
        self.out.push(diagnostic);
    }

    /// The qualified name and the name of the class `expr` stands for.
    fn class_name(&self, expr: &'a Expr, depth: u8) -> Option<(String, String)> {
        if depth > 8 {
            return None;
        }
        if let Expr::Name(name) = expr {
            if let Some(class) = self.env.class_of(expr) {
                return Some((
                    format!("{}.{}", self.env.ctx.module, class.name),
                    class.name.to_string(),
                ));
            }
            let binding = self.env.binding(name)?;
            if binding.kind == BindingKind::Assignment
                && self.env.ctx.model.is_sole_binding(binding.id)
            {
                return self.class_name(self.aliases.get(name.id.as_str())?, depth + 1);
            }
        }
        let qualified = self.env.qualified(expr)?;
        let qualified = self.project.absolute(&qualified);
        if !qualified.contains('.') {
            // A builtin: `Exception` is `builtins.Exception`, `IOError` is `OSError`.
            let qualified = match qualified.as_str() {
                "IOError" | "EnvironmentError" => "OSError".to_owned(),
                _ => qualified,
            };
            return Some((format!("builtins.{qualified}"), qualified));
        }
        let name = qualified.rsplit('.').next().unwrap_or_default().to_owned();
        match self.project.class(&qualified) {
            Some(class) => Some((class.qname(), name)),
            None => Some((qualified, name)),
        }
    }

    fn inherits(&self, class: &'a ruff_python_ast::StmtClassDef, depth: u8) -> Inherit {
        if depth > 8 {
            return Inherit::Unknown;
        }
        let mut unknown = false;
        for base in bases(class) {
            match self.base_inherits(base, depth) {
                Inherit::Yes => return Inherit::Yes,
                Inherit::Unknown => unknown = true,
                Inherit::No => {}
            }
        }
        if unknown {
            Inherit::Unknown
        } else {
            Inherit::No
        }
    }

    fn base_inherits(&self, base: &'a Expr, depth: u8) -> Inherit {
        if let Some(class) = self.env.class_of(base) {
            return self.inherits(class, depth + 1);
        }
        let Some(qualified) = self.env.qualified(base) else {
            return Inherit::Unknown;
        };
        let name = qualified.strip_prefix("builtins.").unwrap_or(&qualified);
        if builtin_exception(name).is_some() {
            Inherit::Yes
        } else if plain_builtin(name) {
            Inherit::No
        } else {
            Inherit::Unknown
        }
    }

    /// Whether `expr` names a class that is known not to derive from
    /// `BaseException`, and its name.
    fn plain_class(&self, expr: &'a Expr) -> Option<String> {
        if let Some(class) = self.env.class_of(expr) {
            return (self.inherits(class, 0) == Inherit::No).then(|| class.name.to_string());
        }
        let Expr::Name(name) = expr else { return None };
        let binding = self.env.binding(name)?;
        (binding.kind == BindingKind::Builtin && plain_builtin(&binding.name))
            .then_some(binding.name)
    }

    fn try_stmt(&mut self, t: &'a StmtTry) {
        for handler in &t.handlers {
            let ExceptHandler::ExceptHandler(h) = handler;
            let Some(type_) = &h.type_ else { continue };
            let parts: Vec<&'a Expr> = match &**type_ {
                Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                Expr::List(list) => list.elts.iter().collect(),
                other => vec![other],
            };
            let raises = h.body.iter().any(|stmt| matches!(stmt, Stmt::Raise(_)));
            for part in parts {
                if self.on(CAUGHT)
                    && !raises
                    && let Some((qname, name)) = self.class_name(part, 0)
                    && self.overgeneral.contains(&qname)
                {
                    self.report(
                        CAUGHT,
                        &**type_,
                        format!("Catching too general exception {name}"),
                    );
                }
                if self.on(CATCHING) {
                    let shown = match part {
                        Expr::NoneLiteral(_)
                        | Expr::NumberLiteral(_)
                        | Expr::StringLiteral(_)
                        | Expr::BooleanLiteral(_) => const_repr(part, "Ellipsis"),
                        // `A | B` is a union type, not a class.
                        Expr::BinOp(b)
                            if b.op == Operator::BitOr
                                && self.tuple_like(&b.left) == Some(false)
                                && self.tuple_like(&b.right) == Some(false) =>
                        {
                            Some(self.env.ctx.source[part.range()].to_owned())
                        }
                        other => self.plain_class(other),
                    };
                    if let Some(shown) = shown {
                        self.report(
                            CATCHING,
                            &**type_,
                            format!("Catching an exception which doesn't inherit from Exception: {shown}"),
                        );
                    }
                }
            }
            if self.on(OPERATION) {
                self.operation(type_);
            }
        }
    }

    /// An operand of `except A + B`: whether Pylint takes it as a tuple.
    fn tuple_like(&self, expr: &'a Expr) -> Option<bool> {
        match expr {
            Expr::Tuple(_) => Some(true),
            other => (self.plain_class(other).is_some()
                || self
                    .class_name(other, 0)
                    .is_some_and(|(q, _)| q.starts_with("builtins.")))
            .then_some(false),
        }
    }

    fn operation(&mut self, type_: &'a Expr) {
        let source = self.env.ctx.source;
        let text = |e: &Expr| source[e.range()].to_owned();
        let suggestion = match type_ {
            Expr::Compare(c) => {
                let operands: Vec<String> = c.operands.iter().map(&text).collect();
                format!("({}, {})", operands[0], operands[1..].join(", "))
            }
            Expr::BinOp(b) => {
                let (left, right) = (self.tuple_like(&b.left), self.tuple_like(&b.right));
                match (left, right) {
                    (Some(true), Some(true)) if b.op == Operator::Add => return,
                    (Some(true), Some(true)) => format!("({} + {})", text(&b.left), text(&b.right)),
                    (Some(_), Some(_)) => format!("({}, {})", text(&b.left), text(&b.right)),
                    _ => return,
                }
            }
            _ => return,
        };
        self.report(
            OPERATION,
            type_,
            format!("Invalid exception operation. Did you mean '{suggestion}' instead?"),
        );
    }

    fn raise(&mut self, raise: &'a StmtRaise) {
        let Some(exc) = &raise.exc else { return };
        if self.on(BAD_CAUSE)
            && let Some(cause) = &raise.cause
        {
            let bad = match &**cause {
                Expr::StringLiteral(_)
                | Expr::NumberLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::EllipsisLiteral(_) => true,
                Expr::Call(call) => {
                    self.plain_class(&call.func).is_some()
                        && self.env.class_of(&call.func).is_some()
                }
                _ => false,
            };
            if bad {
                self.report(
                    BAD_CAUSE,
                    raise,
                    "Exception cause set to something which is not an exception, nor None"
                        .to_owned(),
                );
            }
        }
        let callee = match &**exc {
            Expr::Call(call) => &*call.func,
            other => other,
        };
        if self.on(RAISED)
            && matches!(callee, Expr::Name(_))
            && let Some((qname, name)) = self.class_name(callee, 0)
            && self.overgeneral.contains(&qname)
        {
            self.report(
                RAISED,
                raise,
                format!("Raising too general exception: {name}"),
            );
        }
        if self.on(NON_EXCEPTION)
            && matches!(callee, Expr::Name(_))
            && self.plain_class(callee).is_some()
        {
            self.report(
                NON_EXCEPTION,
                raise,
                "Raising a class which doesn't inherit from BaseException".to_owned(),
            );
        }
        if self.on(BAD_TYPE)
            && let Some(shown) = self.raised_type(exc)
        {
            self.report(
                BAD_TYPE,
                raise,
                format!("Raising {shown} while only classes or instances are allowed"),
            );
        }
    }

    /// The name Pylint gives the type of a raised value that is no exception.
    fn raised_type(&self, exc: &'a Expr) -> Option<String> {
        let name = |text: &str| Some(text.to_owned());
        match exc {
            Expr::StringLiteral(_) => name("str"),
            Expr::BytesLiteral(_) => name("bytes"),
            Expr::BooleanLiteral(_) => name("bool"),
            Expr::NoneLiteral(_) => name("NoneType"),
            Expr::EllipsisLiteral(_) => name("ellipsis"),
            Expr::NumberLiteral(n) => name(number_type(&n.value)),
            Expr::UnaryOp(u) if matches!(u.op, UnaryOp::USub | UnaryOp::UAdd) => {
                match &*u.operand {
                    Expr::NumberLiteral(n) => name(number_type(&n.value)),
                    _ => None,
                }
            }
            Expr::Tuple(_) => name("tuple"),
            Expr::List(_) => name("list"),
            Expr::Dict(_) => name("dict"),
            Expr::Set(_) => name("set"),
            Expr::Lambda(_) => name("<lambda>"),
            Expr::Name(n) if n.id.as_str() == "NotImplemented" => name("NotImplementedType"),
            Expr::Name(n) => self.env.func_of(n).map(|f| f.name.to_string()),
            _ => None,
        }
    }
}

fn number_type(number: &Number) -> &'static str {
    match number {
        Number::Int(_) => "int",
        Number::Float(_) => "float",
        Number::Complex { .. } => "complex",
    }
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Try(t) => self.try_stmt(t),
            Stmt::Raise(r) => self.raise(r),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    const IDS: [&str; 7] = [
        "W0718", "W0719", "E0702", "E0705", "E0710", "E0712", "W0716",
    ];

    #[test]
    fn exceptions() {
        let source = include_str!("fixtures/exceptions.py");
        let expected = include_str!("fixtures/exceptions.expected").trim_end();
        assert_eq!(render(source, &[], &IDS), expected);
    }

    #[test]
    fn overgeneral_exceptions_option() {
        let source = include_str!("fixtures/exceptions_option.py");
        let expected = include_str!("fixtures/exceptions_option.expected").trim_end();
        let options = [(
            "overgeneral-exceptions",
            "builtins.ValueError,mod.Custom,builtins.OSError",
        )];
        assert_eq!(render(source, &options, &IDS), expected);
    }
}
