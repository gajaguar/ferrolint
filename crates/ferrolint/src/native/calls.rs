//! Rules about one call or assignment: `E3701` invalid-field-call, `W1115`
//! non-str-assignment-to-dunder-name, `W1506` bad-thread-instantiation,
//! `W1503` redundant-unittest-assert, `W0715` raising-format-tuple, `W1114`
//! arguments-out-of-order and `W1117` kwarg-superseded-by-positional-arg.
//!
//! shortcut: Pylint infers what a call calls. Here a callee is a name bound
//! once to an undecorated function of this file, `self.method` of the
//! enclosing class, or a class of this file with an `__init__` (`W1114` and
//! `W1117`); a `self` is a `TestCase` when a base is named `...TestCase`
//! (`W1503`). Upgrade with a call inference.

use std::collections::{BTreeSet, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt, StmtAssign, StmtClassDef, StmtFunctionDef, StmtRaise};

use super::classes::{Env, Local, const_repr, is_static, locals, py_repr};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const FIELD: &str = "E3701";
const DUNDER_NAME: &str = "W1115";
const THREAD: &str = "W1506";
const UNITTEST: &str = "W1503";
const FORMAT_TUPLE: &str = "W0715";
const OUT_OF_ORDER: &str = "W1114";
const SUPERSEDED: &str = "W1117";

pub struct Calls;

impl Checker for Calls {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                FIELD,
                "invalid-field-call",
                "`dataclasses.field()` outside the value of a dataclass field.",
            ),
            rule(
                DUNDER_NAME,
                "non-str-assignment-to-dunder-name",
                "A value that is not a string assigned to `__name__`.",
            ),
            rule(
                THREAD,
                "bad-thread-instantiation",
                "`threading.Thread` built without its `target`.",
            ),
            rule(
                UNITTEST,
                "redundant-unittest-assert",
                "`assertTrue` or `assertEqual` over constants.",
            ),
            rule(
                FORMAT_TUPLE,
                "raising-format-tuple",
                "An exception built from a format string and arguments, not formatted.",
            ),
            rule(
                OUT_OF_ORDER,
                "arguments-out-of-order",
                "Positional arguments named like the parameters, in another order.",
            ),
            rule(
                SUPERSEDED,
                "kwarg-superseded-by-positional-arg",
                "A keyword argument named like a positional-only parameter of a function with `**kwargs`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            FIELD,
            DUNDER_NAME,
            THREAD,
            UNITTEST,
            FORMAT_TUPLE,
            OUT_OF_ORDER,
            SUPERSEDED,
        ];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            frames: Vec::new(),
            classes: Vec::new(),
            annotated: HashSet::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// What encloses the code under the walk, for `field()`.
enum Frame {
    /// A class body; `true` for a dataclass.
    Class(bool),
    /// The arguments of a call; `true` for `make_dataclass`.
    Call(bool),
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    frames: Vec<Frame>,
    /// The enclosing classes, with whether each derives from a `TestCase`.
    classes: Vec<(&'a StmtClassDef, bool)>,
    /// The values of the annotated assignments met, by address.
    annotated: HashSet<usize>,
}

fn address(expr: &Expr) -> usize {
    std::ptr::from_ref(expr) as usize
}

fn name_of(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(n) => Some(n.id.as_str()),
        _ => None,
    }
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    /// Whether `expr` names `target` (`dataclasses.field`) by an import.
    fn is(&self, expr: &Expr, target: &str) -> bool {
        self.env.qualified(expr).as_deref() == Some(target)
    }

    fn is_dataclass(class: &StmtClassDef) -> bool {
        class.decorator_list.iter().any(|decorator| {
            let expr = match &decorator.expression {
                Expr::Call(call) => &*call.func,
                other => other,
            };
            match expr {
                Expr::Name(n) => n.id.as_str() == "dataclass",
                Expr::Attribute(a) => a.attr.as_str() == "dataclass",
                _ => false,
            }
        })
    }

    fn call(&mut self, expr: &'a Expr, call: &'a ExprCall) {
        let ctx = self.env.ctx;
        if self.on(FIELD) && self.is(&call.func, "dataclasses.field") {
            let used = match self.frames.last() {
                Some(Frame::Call(make_dataclass)) => (!make_dataclass).then_some(
                    "it should be used within a dataclass or the make_dataclass() function.",
                ),
                Some(Frame::Class(true)) if self.annotated.contains(&address(expr)) => None,
                Some(Frame::Class(true)) => {
                    Some("it should be the value of an assignment within a dataclass.")
                }
                _ => Some("it should be used within a dataclass or the make_dataclass() function."),
            };
            if let Some(why) = used {
                self.out
                    .push(ctx.report(FIELD, call, format!("Invalid usage of field(), {why}")));
            }
        }
        if self.on(THREAD) && self.is(&call.func, "threading.Thread") {
            let target = call
                .arguments
                .keywords
                .iter()
                .any(|k| k.arg.as_ref().is_some_and(|a| a == "target"));
            if !target && call.arguments.args.len() < 2 {
                self.out.push(ctx.report(
                    THREAD,
                    call,
                    "threading.Thread needs the target function",
                ));
            }
        }
        if self.on(UNITTEST) {
            self.unittest(call);
        }
        if self.on(OUT_OF_ORDER) || self.on(SUPERSEDED) {
            self.arguments(call);
        }
    }

    fn unittest(&mut self, call: &ExprCall) {
        let Expr::Attribute(method) = &*call.func else {
            return;
        };
        let Expr::Name(receiver) = &*method.value else {
            return;
        };
        let is_self = receiver.id.as_str() == "self"
            && self
                .env
                .binding(receiver)
                .is_some_and(|b| b.kind == BindingKind::Parameter);
        if !is_self || !self.classes.last().is_some_and(|(_, testcase)| *testcase) {
            return;
        }
        let args = &call.arguments.args;
        let shown = match (method.attr.as_str(), args.as_slice()) {
            ("assertTrue" | "assertFalse", [first, ..]) => const_repr(first, "Ellipsis"),
            ("assertEqual" | "assertNotEqual", [first, second, ..]) => {
                match (
                    const_repr(first, "Ellipsis"),
                    const_repr(second, "Ellipsis"),
                ) {
                    (Some(a), Some(b)) => Some(format!("({a}, {b})")),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(shown) = shown {
            self.out.push(self.env.ctx.report(
                UNITTEST,
                call,
                format!(
                    "Redundant use of {} with constant value {shown}",
                    method.attr
                ),
            ));
        }
    }

    /// The function a call calls, and whether it is a method (`self`
    /// leads its parameters).
    fn callee(&self, call: &ExprCall) -> Option<(&'a StmtFunctionDef, bool)> {
        match &*call.func {
            Expr::Name(name) => {
                if let Some(func) = self.env.func_of(name) {
                    return func.decorator_list.is_empty().then_some((func, false));
                }
                let class = self.env.class_of(&call.func)?;
                let plain = class.decorator_list.is_empty()
                    && class
                        .arguments
                        .as_ref()
                        .is_none_or(|a| a.keywords.is_empty());
                let init = own_method(class, "__init__")?;
                (plain && own_method(class, "__new__").is_none()).then_some((init, true))
            }
            Expr::Attribute(attribute) => {
                let Expr::Name(receiver) = &*attribute.value else {
                    return None;
                };
                let is_self = receiver.id.as_str() == "self"
                    && self
                        .env
                        .binding(receiver)
                        .is_some_and(|b| b.kind == BindingKind::Parameter);
                let (class, _) = self.classes.last()?;
                is_self
                    .then(|| own_method(class, attribute.attr.as_str()).map(|func| (func, true)))?
            }
            _ => None,
        }
    }

    fn arguments(&mut self, call: &ExprCall) {
        let Some((func, in_class)) = self.callee(call) else {
            return;
        };
        let args = &call.arguments;
        if args.args.iter().any(|a| matches!(a, Expr::Starred(_)))
            || args.keywords.iter().any(|k| k.arg.is_none())
        {
            return;
        }
        let p = &func.parameters;
        let mut params: Vec<&str> = p
            .posonlyargs
            .iter()
            .chain(&p.args)
            .map(|a| a.parameter.name.as_str())
            .collect();
        if params.iter().collect::<HashSet<_>>().len() != params.len() {
            return;
        }
        if self.on(OUT_OF_ORDER) {
            if in_class && params.first() == Some(&"self") {
                params.remove(0);
            }
            let positional: Option<Vec<&str>> = args.args.iter().map(name_of).collect();
            let keyword: Option<Vec<&str>> =
                args.keywords.iter().map(|k| name_of(&k.value)).collect();
            if let (Some(positional), Some(keyword)) = (positional, keyword) {
                let given: HashSet<&str> = positional.iter().chain(&keyword).copied().collect();
                let wanted: HashSet<&str> = params.iter().copied().collect();
                if given == wanted && positional[..] != params[..positional.len().min(params.len())]
                {
                    self.out.push(self.env.ctx.report(
                        OUT_OF_ORDER,
                        call,
                        "Positional arguments appear to be out of order",
                    ));
                }
            }
        }
        if self.on(SUPERSEDED)
            && let Some(kwarg) = &p.kwarg
        {
            // Pylint keys the keywords by name, so a repeated one counts once.
            let mut seen = HashSet::new();
            for keyword in args
                .keywords
                .iter()
                .filter_map(|k| k.arg.as_ref())
                .filter(|k| seen.insert(k.as_str()))
            {
                if p.posonlyargs
                    .iter()
                    .any(|a| a.parameter.name.as_str() == keyword.as_str())
                {
                    self.out.push(self.env.ctx.report(
                        SUPERSEDED,
                        call,
                        format!(
                            "{} will be included in {} since a positional-only parameter with this name already exists",
                            py_repr(keyword.as_str()),
                            py_repr(&format!("**{}", kwarg.name))
                        ),
                    ));
                }
            }
        }
    }

    fn dunder_name(&mut self, assign: &StmtAssign) {
        let Some(Expr::Attribute(target)) = assign.targets.first() else {
            return;
        };
        let known_not_str = matches!(
            &*assign.value,
            Expr::NumberLiteral(_)
                | Expr::BooleanLiteral(_)
                | Expr::NoneLiteral(_)
                | Expr::EllipsisLiteral(_)
                | Expr::BytesLiteral(_)
                | Expr::Lambda(_)
                | Expr::List(_)
                | Expr::Tuple(_)
                | Expr::Set(_)
                | Expr::Dict(_)
        );
        if target.attr.as_str() == "__name__" && known_not_str {
            self.out.push(self.env.ctx.report(
                DUNDER_NAME,
                assign,
                "Non-string value assigned to __name__",
            ));
        }
    }

    fn raise(&mut self, raise: &StmtRaise) {
        if let Some(Expr::Call(call)) = raise.exc.as_deref()
            && let [Expr::StringLiteral(message), _, ..] = &call.arguments.args[..]
        {
            let text = message.value.to_str();
            if text.contains('%') || (text.contains('{') && text.contains('}')) {
                self.out.push(self.env.ctx.report(
                    FORMAT_TUPLE,
                    raise,
                    "Exception arguments suggest string formatting might be intended",
                ));
            }
        }
    }
}

/// The method `name` the class binds once in its body, undecorated or static.
fn own_method<'a>(class: &'a StmtClassDef, name: &str) -> Option<&'a StmtFunctionDef> {
    let bound: Vec<Local> = locals(&class.body)
        .into_iter()
        .filter(|(n, _)| n == name)
        .map(|(_, k)| k)
        .collect();
    if bound != [Local::Function] {
        return None;
    }
    super::classes::methods(&class.body)
        .into_iter()
        .find(|f| f.name.as_str() == name)
        .filter(|f| f.decorator_list.is_empty() || is_static(f))
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(arguments) = &class.arguments {
                    self.visit_arguments(arguments);
                }
                let testcase = class
                    .arguments
                    .iter()
                    .flat_map(|a| &a.args)
                    .any(|base| self.env.base_is(base, &|name| name.ends_with("TestCase")));
                self.frames.push(Frame::Class(Self::is_dataclass(class)));
                self.classes.push((class, testcase));
                self.visit_body(&class.body);
                self.classes.pop();
                self.frames.pop();
            }
            Stmt::AnnAssign(assign) => {
                if let Some(value) = &assign.value {
                    self.annotated.insert(address(value));
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Assign(assign) => {
                if self.on(DUNDER_NAME) {
                    self.dunder_name(assign);
                }
                visitor::walk_stmt(self, stmt);
            }
            Stmt::Raise(raise) => {
                if self.on(FORMAT_TUPLE) {
                    self.raise(raise);
                }
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            self.call(expr, call);
            let make_dataclass = matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "make_dataclass")
                && self.is(&call.func, "dataclasses.make_dataclass");
            self.frames.push(Frame::Call(make_dataclass));
            visitor::walk_expr(self, expr);
            self.frames.pop();
        } else {
            visitor::walk_expr(self, expr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::classes::render;

    #[test]
    fn single_call_and_assignment_rules() {
        let source = include_str!("fixtures/single_call_and_assignment_rules.py");
        let expected =
            include_str!("fixtures/single_call_and_assignment_rules.expected").trim_end();
        assert_eq!(
            render(source, &[], &["W1115", "W1506", "W1503", "W0715", "E3701"]),
            expected
        );
    }

    #[test]
    fn call_arguments() {
        let source = include_str!("fixtures/call_arguments.py");
        let expected = include_str!("fixtures/call_arguments.expected").trim_end();
        assert_eq!(render(source, &[], &["W1114", "W1117"]), expected);
    }
}
