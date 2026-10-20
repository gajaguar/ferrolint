//! Control flow of functions and `try`: `W0101` unreachable, `R1710`
//! inconsistent-return-statements, `R1711` useless-return, `R0911`
//! too-many-return-statements, `R0913` too-many-arguments, `R0914`
//! too-many-locals, `W1113` keyword-arg-before-vararg and `E0701`
//! bad-except-order.
//!
//! shortcut: Pylint infers what a call calls and what a raised or caught
//! name stands for. Here a terminating call is `exit`, `quit`, `sys.exit`,
//! `os._exit`, `self.fail` of a `...TestCase`, a function of this file
//! annotated `NoReturn`/`Never`, or a name of `never-returning-functions`;
//! the exception hierarchy is the builtins plus the classes of this file.
//! Upgrade with a call and class inference.

use std::collections::{BTreeSet, HashMap, HashSet};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ElifElseClause, ExceptHandler, Expr, ExprCall, HasNodeIndex, Number, Parameters, Pattern, Stmt,
    StmtClassDef, StmtFunctionDef, StmtRaise, StmtTry,
};
use ruff_text_size::Ranged;

use super::classes::{Env, bases, is_classmethod, is_static};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const UNREACHABLE: &str = "W0101";
const INCONSISTENT: &str = "R1710";
const USELESS: &str = "R1711";
const RETURNS: &str = "R0911";
const ARGUMENTS: &str = "R0913";
const LOCALS: &str = "R0914";
const VARARG: &str = "W1113";
const EXCEPT_ORDER: &str = "E0701";

pub struct Flow;

impl Checker for Flow {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                UNREACHABLE,
                "unreachable",
                "Code after `return`, `raise`, `break`, `continue` or a call that never returns.",
            ),
            rule(
                INCONSISTENT,
                "inconsistent-return-statements",
                "A function whose `return` statements do not all return a value.",
            ),
            rule(
                USELESS,
                "useless-return",
                "A lone `return` or `return None` at the end of a function.",
            ),
            rule(
                RETURNS,
                "too-many-return-statements",
                "A function with more than `max-returns` return statements.",
            ),
            rule(
                ARGUMENTS,
                "too-many-arguments",
                "A function with more than `max-args` arguments.",
            ),
            rule(
                LOCALS,
                "too-many-locals",
                "A function with more than `max-locals` local variables.",
            ),
            rule(
                VARARG,
                "keyword-arg-before-vararg",
                "A parameter with a default before `*args`.",
            ),
            rule(
                EXCEPT_ORDER,
                "bad-except-order",
                "An `except` clause after one for an ancestor class, or after a bare `except`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        let ours = [
            UNREACHABLE,
            INCONSISTENT,
            USELESS,
            RETURNS,
            ARGUMENTS,
            LOCALS,
            VARARG,
            EXCEPT_ORDER,
        ];
        if !ours.iter().any(|id| enabled.contains(*id)) {
            return;
        }
        let env = Env::new(ctx);
        let mut assigned = HashMap::new();
        collect(&ctx.parsed.syntax().body, &mut assigned);
        let mut walk = Walk {
            env: &env,
            enabled,
            out,
            class: Vec::new(),
            assigned,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// One enclosing class, and whether it derives from a `...TestCase`.
struct Frame<'a> {
    class: &'a StmtClassDef,
    /// Whether the statement being walked is directly in the class body.
    direct: bool,
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    enabled: &'e BTreeSet<String>,
    out: &'e mut Vec<Diagnostic>,
    class: Vec<Frame<'a>>,
    /// The value assigned to each name, by the node id of the target;
    /// none for a name that unpacking binds.
    assigned: Assigned<'a>,
}

type Assigned<'a> = HashMap<u32, Option<&'a Expr>>;

fn collect<'a>(body: &'a [Stmt], into: &mut Assigned<'a>) {
    struct Collect<'a, 'm>(&'m mut Assigned<'a>);
    impl<'a> Visitor<'a> for Collect<'a, '_> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            if let Stmt::Assign(assign) = stmt {
                for target in &assign.targets {
                    let (names, value) = match target {
                        Expr::Name(_) => (vec![target], Some(&*assign.value)),
                        Expr::Tuple(t) => (t.elts.iter().collect(), None),
                        Expr::List(l) => (l.elts.iter().collect(), None),
                        _ => continue,
                    };
                    for name in names {
                        if let Expr::Name(n) = name
                            && let Some(id) = n.node_index().load().as_u32()
                        {
                            self.0.insert(id, value);
                        }
                    }
                }
            }
            visitor::walk_stmt(self, stmt);
        }
    }
    Collect(into).visit_body(body);
}

impl<'a> Walk<'a, '_> {
    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn number(&self, key: &str, default: usize) -> usize {
        self.env
            .ctx
            .option(key)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    fn function(&mut self, func: &'a StmtFunctionDef) {
        let ctx = self.env.ctx;
        let name = &func.name;
        let returns = own_returns(&func.body);
        let max = self.number("max-returns", 6);
        if self.on(RETURNS) && returns.len() > max {
            self.out.push(ctx.report(
                RETURNS,
                name,
                format!("Too many return statements ({}/{max})", returns.len()),
            ));
        }
        let ignored = self.arguments(func);
        if self.on(LOCALS) {
            let locals = own_locals(func);
            let max = self.number("max-locals", 15);
            let count = locals
                .len()
                .saturating_sub(usize::from(locals.contains("_")))
                .saturating_sub(ignored);
            if count > max {
                self.out.push(ctx.report(
                    LOCALS,
                    name,
                    format!("Too many local variables ({count}/{max})"),
                ));
            }
        }
        let p = &func.parameters;
        if self.on(VARARG)
            && p.vararg.is_some()
            && p.posonlyargs
                .iter()
                .chain(&p.args)
                .any(|a| a.default.is_some())
            && (p.posonlyargs.is_empty() || !p.args.is_empty())
        {
            self.out.push(ctx.report(
                VARARG,
                name,
                format!("Keyword argument before variable positional arguments list in the definition of {} function", func.name),
            ));
        }
        // the return rules run on `def`, never on `async def`
        if !func.is_async && (self.on(INCONSISTENT) || self.on(USELESS)) {
            let class = self.class.last().filter(|f| f.direct).map(|f| f.class);
            let ender = Ender {
                env: self.env,
                class,
                assigned: &self.assigned,
            };
            if self.on(INCONSISTENT)
                && returns.iter().any(|r| r.value.is_some())
                && !(returns.iter().all(|r| r.value.is_some())
                    && ender.stmts(&func.body, Wrap::None))
            {
                self.out.push(ctx.report(
                    INCONSISTENT,
                    name,
                    "Either all return statements in a function should return an expression, or none of them should.",
                ));
            }
            if self.on(USELESS) && returns.len() == 1 && useless_return(&func.body) {
                self.out.push(ctx.report(
                    USELESS,
                    name,
                    "Useless return at end of function or method",
                ));
            }
        }
    }

    /// R0913; returns how many arguments match `ignored-argument-names`.
    fn arguments(&mut self, func: &StmtFunctionDef) -> usize {
        let ctx = self.env.ctx;
        let in_class = self.class.last().is_some_and(|f| f.direct);
        let method = in_class && (is_classmethod(func) || !is_static(func));
        let p = &func.parameters;
        let mut positional: Vec<&str> = p
            .posonlyargs
            .iter()
            .chain(&p.args)
            .map(|a| a.parameter.name.as_str())
            .collect();
        if method && !positional.is_empty() {
            positional.remove(0);
        }
        let kwonly: Vec<&str> = p
            .kwonlyargs
            .iter()
            .map(|a| a.parameter.name.as_str())
            .collect();
        let pattern = ctx
            .option("ignored-argument-names")
            .unwrap_or("_.*|^ignored_|^unused_");
        let ignored = positional
            .iter()
            .chain(&kwonly)
            .filter(|n| rx_match(pattern, n))
            .count();
        let total = positional.len() + kwonly.len();
        let max = self.number("max-args", 5);
        if self.on(ARGUMENTS) && total > 0 && total - ignored > max {
            self.out.push(ctx.report(
                ARGUMENTS,
                &func.name,
                format!("Too many arguments ({}/{max})", total - ignored),
            ));
        }
        if total > 0 { ignored } else { 0 }
    }

    fn unreachable(&mut self, body: &'a [Stmt]) {
        let ctx = self.env.ctx;
        let ender = Ender {
            env: self.env,
            class: self.class.last().map(|f| f.class),
            assigned: &self.assigned,
        };
        for (i, stmt) in body.iter().enumerate() {
            let stops = match stmt {
                Stmt::Return(_) | Stmt::Raise(_) | Stmt::Break(_) | Stmt::Continue(_) => true,
                Stmt::Expr(e) => matches!(&*e.value, Expr::Call(call) if ender.terminating(call)),
                _ => false,
            };
            if !stops {
                continue;
            }
            let mut next = i + 1;
            // a `yield` that only marks a generator is not unreachable code
            if matches!(body.get(next), Some(Stmt::Expr(e)) if matches!(&*e.value, Expr::Yield(_)))
            {
                next += 1;
            }
            if let Some(stmt) = body.get(next) {
                let range = match stmt {
                    Stmt::FunctionDef(f) => f.name.range(),
                    Stmt::ClassDef(c) => c.name.range(),
                    other => other.range(),
                };
                self.out
                    .push(ctx.report_at(UNREACHABLE, range, "Unreachable code"));
            }
        }
    }

    fn handlers(&mut self, node: &StmtTry) {
        let ctx = self.env.ctx;
        let mut seen: Vec<Class<'a>> = Vec::new();
        for (index, handler) in node.handlers.iter().enumerate() {
            let ExceptHandler::ExceptHandler(h) = handler;
            let Some(type_) = &h.type_ else {
                if index + 1 < node.handlers.len() {
                    self.out.push(ctx.report_at(
                        EXCEPT_ORDER,
                        node.range(),
                        "Bad except clauses order (empty except clause should always appear last)",
                    ));
                }
                continue;
            };
            let parts: Vec<&Expr> = match &**type_ {
                Expr::Tuple(t) => t.elts.iter().collect(),
                other => vec![other],
            };
            let mut found = Vec::new();
            for part in parts {
                let Some(class) = self.class_of(part) else {
                    continue;
                };
                let ancestors = self.ancestors(class);
                for previous in &seen {
                    if ancestors.contains(previous) {
                        self.out.push(ctx.report(
                            EXCEPT_ORDER,
                            &**type_,
                            format!(
                                "Bad except clauses order ({} is an ancestor class of {})",
                                previous.name(),
                                class.name()
                            ),
                        ));
                    }
                }
                found.push(class);
            }
            seen.extend(found);
        }
    }

    /// The exception class `expr` names: a builtin or a class of this file.
    fn class_of(&self, expr: &Expr) -> Option<Class<'a>> {
        if let Some(class) = self.env.class_of(expr) {
            return Some(Class::Local(class));
        }
        let name = self.env.qualified(expr)?;
        builtin_exception(&name).map(Class::Builtin)
    }

    fn ancestors(&self, class: Class<'a>) -> Vec<Class<'a>> {
        let mut out = Vec::new();
        let mut todo = vec![class];
        while let Some(next) = todo.pop() {
            let parents: Vec<Class<'a>> = match next {
                Class::Builtin(name) => EXCEPTIONS
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, parents)| {
                        parents
                            .iter()
                            .map(|p| Class::Builtin(canonical(p)))
                            .collect()
                    })
                    .unwrap_or_default(),
                Class::Local(c) => bases(c).iter().filter_map(|b| self.class_of(b)).collect(),
            };
            for parent in parents {
                if !out.contains(&parent) && out.len() < 64 {
                    out.push(parent);
                    todo.push(parent);
                }
            }
        }
        out
    }
}

#[derive(Clone, Copy)]
enum Class<'a> {
    Builtin(&'static str),
    Local(&'a StmtClassDef),
}

impl PartialEq for Class<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Class::Builtin(a), Class::Builtin(b)) => a == b,
            (Class::Local(a), Class::Local(b)) => std::ptr::eq(*a, *b),
            _ => false,
        }
    }
}

impl Class<'_> {
    fn name(&self) -> String {
        match self {
            Class::Builtin(name) => (*name).to_owned(),
            Class::Local(class) => class.name.to_string(),
        }
    }
}

/// The builtin exception classes, each with its parents; `IOError` and
/// `EnvironmentError` are `OSError` under another name.
const EXCEPTIONS: &[(&str, &[&str])] = &[
    ("BaseException", &[]),
    ("BaseExceptionGroup", &["BaseException"]),
    ("GeneratorExit", &["BaseException"]),
    ("KeyboardInterrupt", &["BaseException"]),
    ("SystemExit", &["BaseException"]),
    ("Exception", &["BaseException"]),
    ("ExceptionGroup", &["BaseExceptionGroup", "Exception"]),
    ("ArithmeticError", &["Exception"]),
    ("FloatingPointError", &["ArithmeticError"]),
    ("OverflowError", &["ArithmeticError"]),
    ("ZeroDivisionError", &["ArithmeticError"]),
    ("AssertionError", &["Exception"]),
    ("AttributeError", &["Exception"]),
    ("BufferError", &["Exception"]),
    ("EOFError", &["Exception"]),
    ("ImportError", &["Exception"]),
    ("ModuleNotFoundError", &["ImportError"]),
    ("LookupError", &["Exception"]),
    ("IndexError", &["LookupError"]),
    ("KeyError", &["LookupError"]),
    ("MemoryError", &["Exception"]),
    ("NameError", &["Exception"]),
    ("UnboundLocalError", &["NameError"]),
    ("OSError", &["Exception"]),
    ("BlockingIOError", &["OSError"]),
    ("ChildProcessError", &["OSError"]),
    ("ConnectionError", &["OSError"]),
    ("BrokenPipeError", &["ConnectionError"]),
    ("ConnectionAbortedError", &["ConnectionError"]),
    ("ConnectionRefusedError", &["ConnectionError"]),
    ("ConnectionResetError", &["ConnectionError"]),
    ("FileExistsError", &["OSError"]),
    ("FileNotFoundError", &["OSError"]),
    ("InterruptedError", &["OSError"]),
    ("IsADirectoryError", &["OSError"]),
    ("NotADirectoryError", &["OSError"]),
    ("PermissionError", &["OSError"]),
    ("ProcessLookupError", &["OSError"]),
    ("TimeoutError", &["OSError"]),
    ("ReferenceError", &["Exception"]),
    ("RuntimeError", &["Exception"]),
    ("NotImplementedError", &["RuntimeError"]),
    ("PythonFinalizationError", &["RuntimeError"]),
    ("RecursionError", &["RuntimeError"]),
    ("StopAsyncIteration", &["Exception"]),
    ("StopIteration", &["Exception"]),
    ("SyntaxError", &["Exception"]),
    ("IndentationError", &["SyntaxError"]),
    ("TabError", &["IndentationError"]),
    ("SystemError", &["Exception"]),
    ("TypeError", &["Exception"]),
    ("ValueError", &["Exception"]),
    ("UnicodeError", &["ValueError"]),
    ("UnicodeDecodeError", &["UnicodeError"]),
    ("UnicodeEncodeError", &["UnicodeError"]),
    ("UnicodeTranslateError", &["UnicodeError"]),
    ("Warning", &["Exception"]),
    ("BytesWarning", &["Warning"]),
    ("DeprecationWarning", &["Warning"]),
    ("EncodingWarning", &["Warning"]),
    ("FutureWarning", &["Warning"]),
    ("ImportWarning", &["Warning"]),
    ("PendingDeprecationWarning", &["Warning"]),
    ("ResourceWarning", &["Warning"]),
    ("RuntimeWarning", &["Warning"]),
    ("SyntaxWarning", &["Warning"]),
    ("UnicodeWarning", &["Warning"]),
    ("UserWarning", &["Warning"]),
];

fn canonical(name: &str) -> &'static str {
    EXCEPTIONS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("", |(n, _)| n)
}

pub(super) fn builtin_exception(name: &str) -> Option<&'static str> {
    let name = if matches!(name, "IOError" | "EnvironmentError") {
        "OSError"
    } else {
        name
    };
    EXCEPTIONS.iter().find(|(n, _)| *n == name).map(|(n, _)| *n)
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_body(&mut self, body: &'a [Stmt]) {
        if self.on(UNREACHABLE) {
            self.unreachable(body);
        }
        visitor::walk_body(self, body);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                for decorator in &class.decorator_list {
                    self.visit_decorator(decorator);
                }
                self.class.push(Frame {
                    class,
                    direct: true,
                });
                self.visit_body(&class.body);
                self.class.pop();
            }
            Stmt::FunctionDef(func) => {
                self.function(func);
                // the body is not directly in a class body
                let direct = self
                    .class
                    .last_mut()
                    .map(|f| std::mem::replace(&mut f.direct, false));
                visitor::walk_stmt(self, stmt);
                if let (Some(frame), Some(direct)) = (self.class.last_mut(), direct) {
                    frame.direct = direct;
                }
            }
            Stmt::Try(node) => {
                if self.on(EXCEPT_ORDER) {
                    self.handlers(node);
                }
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// The `return` statements of a function body, outside nested functions.
pub(super) fn own_returns(body: &[Stmt]) -> Vec<&ruff_python_ast::StmtReturn> {
    struct Returns<'a>(Vec<&'a ruff_python_ast::StmtReturn>);
    impl<'a> Visitor<'a> for Returns<'a> {
        fn visit_stmt(&mut self, stmt: &'a Stmt) {
            match stmt {
                Stmt::FunctionDef(_) => {}
                Stmt::Return(r) => self.0.push(r),
                _ => visitor::walk_stmt(self, stmt),
            }
        }
        fn visit_expr(&mut self, _: &'a Expr) {}
    }
    let mut found = Returns(Vec::new());
    found.visit_body(body);
    found.0
}

/// Whether the single `return` of a function ends it with no value.
fn useless_return(body: &[Stmt]) -> bool {
    // astroid keeps the docstring out of the body
    let body = match body.first() {
        Some(Stmt::Expr(e)) if matches!(&*e.value, Expr::StringLiteral(_)) => &body[1..],
        _ => body,
    };
    let Some(mut last) = body.last() else {
        return false;
    };
    if body.len() == 1 && matches!(last, Stmt::Return(_)) {
        return false;
    }
    // astroid's `last_child` of an `if` or `try` is its last statement
    loop {
        let next = match last {
            Stmt::If(s) => s
                .elif_else_clauses
                .last()
                .map_or(s.body.last(), |clause| clause.body.last()),
            Stmt::Try(s) if !s.is_star => {
                if let Some(stmt) = s.finalbody.last().or(s.orelse.last()) {
                    Some(stmt)
                } else if let Some(ExceptHandler::ExceptHandler(h)) = s.handlers.last() {
                    h.body.last()
                } else {
                    s.body.last()
                }
            }
            _ => break,
        };
        match next {
            Some(stmt) => last = stmt,
            None => return false,
        }
    }
    matches!(last, Stmt::Return(r) if r.value.as_deref().is_none_or(|v| matches!(v, Expr::NoneLiteral(_))))
}

/// The exception that a `raise` raises, as far as it is known.
enum Raised {
    Class(String),
    Unknown,
    Uninferable,
}

/// What the nearest `try` or handler around a `raise` is.
#[derive(Clone, Copy)]
enum Wrap<'a> {
    None,
    Try(&'a StmtTry),
    Handler,
}

/// Pylint's `_is_node_return_ended`: whether a function body ends in a
/// `return`, a `raise` or a call that never returns on every path.
struct Ender<'e, 'a> {
    env: &'e Env<'a>,
    /// The class around the function, when it is a method.
    class: Option<&'a StmtClassDef>,
    assigned: &'e Assigned<'a>,
}

impl<'a> Ender<'_, 'a> {
    fn stmts(&self, body: &'a [Stmt], wrap: Wrap<'a>) -> bool {
        body.iter()
            .enumerate()
            .any(|(i, stmt)| self.stmt(stmt, &body[i + 1..], wrap))
    }

    fn stmt(&self, stmt: &'a Stmt, after: &'a [Stmt], wrap: Wrap<'a>) -> bool {
        match stmt {
            Stmt::Return(_) => true,
            Stmt::Raise(raise) => self.raise(raise, wrap),
            Stmt::If(s) => self.conditional(&s.body, &s.elif_else_clauses, after, wrap),
            Stmt::While(s) => {
                (const_truth(&s.test) == Some(true) && !exits_early(&s.body, &s.orelse))
                    || self.stmts(&s.orelse, wrap)
            }
            Stmt::Try(t) if !t.is_star => {
                let inner = Wrap::Try(t);
                (self.stmts(&t.body, inner)
                    || self.stmts(&t.orelse, inner)
                    || self.stmts(&t.finalbody, inner))
                    && t.handlers.iter().all(|h| {
                        let ExceptHandler::ExceptHandler(h) = h;
                        self.stmts(&h.body, Wrap::Handler)
                    })
            }
            Stmt::Assert(a)
                if matches!(const_truth(&a.test), Some(false)) && is_number_or_bool(&a.test) =>
            {
                true
            }
            Stmt::Expr(e) => self.expr(&e.value),
            Stmt::For(s) => {
                self.expr(&s.iter) || self.stmts(&s.body, wrap) || self.stmts(&s.orelse, wrap)
            }
            Stmt::With(s) => {
                s.items.iter().any(|item| self.expr(&item.context_expr))
                    || self.stmts(&s.body, wrap)
            }
            Stmt::Try(t) => {
                self.stmts(&t.body, wrap)
                    || self.stmts(&t.orelse, wrap)
                    || self.stmts(&t.finalbody, wrap)
                    || t.handlers.iter().any(|h| {
                        let ExceptHandler::ExceptHandler(h) = h;
                        self.stmts(&h.body, Wrap::Handler)
                    })
            }
            Stmt::Match(m) => m.cases.iter().any(|c| self.stmts(&c.body, wrap)),
            Stmt::FunctionDef(f) => self.stmts(&f.body, wrap),
            Stmt::ClassDef(c) => self.stmts(&c.body, wrap),
            Stmt::Assign(a) => self.expr(&a.value),
            Stmt::AugAssign(a) => self.expr(&a.value),
            Stmt::AnnAssign(a) => a.value.as_deref().is_some_and(|v| self.expr(v)),
            _ => false,
        }
    }

    /// Pylint's `_is_if_node_return_ended`; `rest` are the `elif` and
    /// `else` clauses.
    fn conditional(
        &self,
        body: &'a [Stmt],
        rest: &'a [ElifElseClause],
        after: &'a [Stmt],
        wrap: Wrap<'a>,
    ) -> bool {
        let returning = self.skip_functions(body, wrap);
        let Some(first) = rest.first() else {
            return after.iter().any(|s| matches!(s, Stmt::Return(_))) && returning;
        };
        let orelse = if first.test.is_some() {
            // an `elif` is an `if` alone in the `else` list
            self.conditional(&first.body, &rest[1..], &[], wrap)
        } else {
            self.skip_functions(&first.body, wrap)
        };
        returning && orelse
    }

    fn skip_functions(&self, body: &'a [Stmt], wrap: Wrap<'a>) -> bool {
        body.iter()
            .enumerate()
            .any(|(i, s)| !matches!(s, Stmt::FunctionDef(_)) && self.stmt(s, &body[i + 1..], wrap))
    }

    fn raise(&self, raise: &StmtRaise, wrap: Wrap<'a>) -> bool {
        let Some(exc) = &raise.exc else { return true };
        let Wrap::Try(node) = wrap else { return true };
        let name = match self.raised(exc) {
            Raised::Uninferable => return false,
            Raised::Unknown => return true,
            Raised::Class(name) => name,
        };
        let handling: Vec<_> = node
            .handlers
            .iter()
            .filter_map(|h| {
                let ExceptHandler::ExceptHandler(h) = h;
                let type_ = h.type_.as_deref()?;
                names_in(type_).contains(&name).then_some(h)
            })
            .collect();
        handling.is_empty() || handling.iter().any(|h| self.stmts(&h.body, Wrap::Handler))
    }

    /// What `raise expr` raises. A name that is not a class stands for a
    /// value that Pylint infers; assumed, as it usually succeeds.
    fn raised(&self, expr: &Expr) -> Raised {
        let target = match expr {
            Expr::Call(call) => &*call.func,
            other => other,
        };
        match target {
            Expr::Attribute(a) => Raised::Class(a.attr.to_string()),
            Expr::Name(n) => {
                let Some(binding) = self.env.binding(n) else {
                    return Raised::Uninferable;
                };
                match binding.kind {
                    BindingKind::ClassDef | BindingKind::Builtin => Raised::Class(binding.name),
                    // `queue.Empty` has two definitions, so Pylint infers neither
                    BindingKind::FromImport => match binding.origin {
                        Some(o) if matches!(o.as_str(), "queue.Empty" | "queue.Full") => {
                            Raised::Uninferable
                        }
                        Some(o) => o
                            .rsplit('.')
                            .next()
                            .map_or(Raised::Unknown, |n| Raised::Class(n.to_owned())),
                        None => Raised::Unknown,
                    },
                    // a variable is as good as what is assigned to it
                    BindingKind::Assignment => match binding
                        .node
                        .and_then(|id| self.assigned.get(&id))
                    {
                        Some(Some(value)) if matches!(value, Expr::Call(_)) => self.raised(value),
                        Some(None) => Raised::Uninferable,
                        _ => Raised::Unknown,
                    },
                    // `self` and `cls` are inferred as the class
                    BindingKind::Parameter if matches!(binding.name.as_str(), "self" | "cls") => {
                        Raised::Unknown
                    }
                    BindingKind::Parameter => Raised::Uninferable,
                    _ => Raised::Unknown,
                }
            }
            _ => Raised::Unknown,
        }
    }

    /// A call that never returns, anywhere in `expr` but inside a call.
    fn expr(&self, expr: &Expr) -> bool {
        struct Calls<'e, 'a, 'x> {
            ender: &'x Ender<'e, 'a>,
            found: bool,
        }
        impl<'b> Visitor<'b> for Calls<'_, '_, '_> {
            fn visit_expr(&mut self, expr: &'b Expr) {
                match expr {
                    Expr::Call(call) => {
                        self.found |=
                            self.ender.terminating(call) || self.ender.never_returns(call);
                    }
                    _ => visitor::walk_expr(self, expr),
                }
            }
        }
        let mut calls = Calls {
            ender: self,
            found: false,
        };
        calls.visit_expr(expr);
        calls.found
    }

    /// Pylint's `is_terminating_func`.
    fn terminating(&self, call: &ExprCall) -> bool {
        if !matches!(&*call.func, Expr::Name(_) | Expr::Attribute(_)) {
            return false;
        }
        if let Some(name) = self.env.qualified(&call.func)
            && (name.ends_with(".assert_never")
                || matches!(
                    name.as_str(),
                    "exit"
                        | "quit"
                        | "sys.exit"
                        | "os._exit"
                        | "os.abort"
                        | "pytest.fail"
                        | "pytest.skip"
                        | "pytest.exit"
                        | "pytest.xfail"
                        | "typing.assert_never"
                        | "typing_extensions.assert_never"
                ))
        {
            return true;
        }
        // `TestCase.fail` never returns, unless the class overrides it
        if let Expr::Attribute(a) = &*call.func
            && a.attr.as_str() == "fail"
            && matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self")
            && self.class.is_some_and(|c| {
                !self.env.defines(c, "fail", 0)
                    && bases(c)
                        .iter()
                        .any(|b| self.env.base_is(b, &|name| name.ends_with("TestCase")))
            })
        {
            return true;
        }
        matches!(&*call.func, Expr::Name(n) if self.env.func_of(n).is_some_and(|f| {
            !f.is_async
                && f.returns.as_deref().is_some_and(|r| {
                    self.env.qualified(r).is_some_and(|q| {
                        matches!(q.as_str(), "typing.NoReturn" | "typing.Never" | "typing_extensions.NoReturn" | "typing_extensions.Never")
                    })
                })
        }))
    }

    /// Pylint's `_is_function_def_never_returning`: a name of
    /// `never-returning-functions`, or a function annotated `NoReturn`.
    fn never_returns(&self, call: &ExprCall) -> bool {
        let listed = self
            .env
            .ctx
            .option("never-returning-functions")
            .unwrap_or("");
        if self
            .env
            .qualified(&call.func)
            .is_some_and(|q| listed.split(',').any(|n| n.trim() == q))
        {
            return true;
        }
        let Expr::Name(n) = &*call.func else {
            return false;
        };
        self.env.func_of(n).is_some_and(|f| {
            matches!(f.returns.as_deref(), Some(Expr::Name(r)) if matches!(r.id.as_str(), "NoReturn" | "Never"))
                || matches!(f.returns.as_deref(), Some(Expr::Attribute(r)) if matches!(r.attr.as_str(), "NoReturn" | "Never"))
        })
    }
}

fn is_number_or_bool(expr: &Expr) -> bool {
    matches!(expr, Expr::BooleanLiteral(_) | Expr::NumberLiteral(_))
}

/// All the names written inside `expr`.
fn names_in(expr: &Expr) -> Vec<String> {
    struct Names(Vec<String>);
    impl<'a> Visitor<'a> for Names {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Name(n) = expr {
                self.0.push(n.id.to_string());
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut names = Names(Vec::new());
    names.visit_expr(expr);
    names.0
}

/// The truth of a test that is written as a constant: a literal, a
/// container or a lambda, as astroid's `bool_value` gives it.
pub(super) fn const_truth(expr: &Expr) -> Option<bool> {
    Some(match expr {
        Expr::BooleanLiteral(b) => b.value,
        Expr::NoneLiteral(_) => false,
        Expr::EllipsisLiteral(_) | Expr::Lambda(_) => true,
        Expr::NumberLiteral(n) => match &n.value {
            Number::Int(i) => i.as_u64() != Some(0),
            Number::Float(f) => *f != 0.0,
            Number::Complex { real, imag } => *real != 0.0 || *imag != 0.0,
        },
        Expr::StringLiteral(s) => !s.value.to_str().is_empty(),
        Expr::BytesLiteral(b) => b.value.bytes().next().is_some(),
        Expr::Tuple(t) => !t.elts.is_empty(),
        Expr::List(l) => !l.elts.is_empty(),
        Expr::Set(s) => !s.elts.is_empty(),
        Expr::Dict(d) => !d.items.is_empty(),
        _ => return None,
    })
}

/// Pylint's `_loop_exits_early`: whether a `break` ends this loop (a
/// `break` in its own `else` ends the loop around it, which counts too).
pub(super) fn exits_early(body: &[Stmt], orelse: &[Stmt]) -> bool {
    fn scan(body: &[Stmt], nested: bool) -> bool {
        body.iter().any(|stmt| match stmt {
            Stmt::Break(_) => !nested,
            Stmt::For(s) => scan(&s.body, true) || scan(&s.orelse, nested),
            Stmt::While(s) => scan(&s.body, true) || scan(&s.orelse, nested),
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => false,
            other => super::classes::blocks(other)
                .iter()
                .any(|b| scan(b, nested)),
        })
    }
    scan(body, false) || scan(orelse, false)
}

/// The names a function binds: its parameters and every name its own scope
/// assigns, imports, defines, loops over, catches or captures.
struct Locals(HashSet<String>, HashSet<String>);
impl Locals {
    fn target(&mut self, target: &Expr) {
        match target {
            Expr::Name(n) => {
                self.0.insert(n.id.to_string());
            }
            Expr::Tuple(t) => t.elts.iter().for_each(|e| self.target(e)),
            Expr::List(l) => l.elts.iter().for_each(|e| self.target(e)),
            Expr::Starred(s) => self.target(&s.value),
            _ => {}
        }
    }
    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::MatchAs(p) => {
                if let Some(name) = &p.name {
                    self.0.insert(name.to_string());
                }
                if let Some(inner) = &p.pattern {
                    self.pattern(inner);
                }
            }
            Pattern::MatchStar(p) => {
                self.0.extend(p.name.iter().map(ToString::to_string));
            }
            Pattern::MatchMapping(p) => {
                self.0.extend(p.rest.iter().map(ToString::to_string));
                p.patterns.iter().for_each(|q| self.pattern(q));
            }
            Pattern::MatchSequence(p) => p.patterns.iter().for_each(|q| self.pattern(q)),
            Pattern::MatchOr(p) => p.patterns.iter().for_each(|q| self.pattern(q)),
            Pattern::MatchClass(p) => {
                p.arguments.patterns.iter().for_each(|q| self.pattern(q));
                p.arguments
                    .keywords
                    .iter()
                    .for_each(|k| self.pattern(&k.pattern));
            }
            Pattern::MatchValue(_) | Pattern::MatchSingleton(_) => {}
        }
    }
}
impl<'a> Visitor<'a> for Locals {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(f) => {
                self.0.insert(f.name.to_string());
                // decorators and defaults run in this scope
                f.decorator_list
                    .iter()
                    .for_each(|d| self.visit_decorator(d));
                return;
            }
            Stmt::ClassDef(c) => {
                self.0.insert(c.name.to_string());
                c.decorator_list
                    .iter()
                    .for_each(|d| self.visit_decorator(d));
                return;
            }
            Stmt::Assign(a) => a.targets.iter().for_each(|t| self.target(t)),
            Stmt::AugAssign(a) => self.target(&a.target),
            Stmt::AnnAssign(a) => self.target(&a.target),
            Stmt::For(f) => self.target(&f.target),
            Stmt::With(w) => w
                .items
                .iter()
                .filter_map(|i| i.optional_vars.as_deref())
                .for_each(|t| self.target(t)),
            Stmt::Delete(d) => d.targets.iter().for_each(|t| self.target(t)),
            Stmt::TypeAlias(t) => self.target(&t.name),
            Stmt::Try(t) => {
                for handler in &t.handlers {
                    let ExceptHandler::ExceptHandler(h) = handler;
                    self.0.extend(h.name.iter().map(ToString::to_string));
                }
            }
            Stmt::Import(i) => {
                for alias in &i.names {
                    let name = alias.asname.as_ref().map_or_else(
                        || alias.name.split('.').next().unwrap_or_default().to_owned(),
                        ToString::to_string,
                    );
                    self.0.insert(name);
                }
            }
            Stmt::ImportFrom(i) => {
                for alias in i.names.iter().filter(|a| &a.name != "*") {
                    self.0
                        .insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string());
                }
            }
            Stmt::Match(m) => m.cases.iter().for_each(|c| self.pattern(&c.pattern)),
            // a global is assigned in the module, not here
            Stmt::Global(g) => self.1.extend(g.names.iter().map(ToString::to_string)),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            // a lambda has a scope of its own
            Expr::Lambda(_) => {}
            Expr::Named(n) => {
                self.target(&n.target);
                visitor::walk_expr(self, expr);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

fn own_locals(func: &StmtFunctionDef) -> HashSet<String> {
    let mut locals = Locals(HashSet::new(), HashSet::new());
    let p: &Parameters = &func.parameters;
    locals.0.extend(p.iter().map(|x| x.name().to_string()));
    locals.visit_body(&func.body);
    let globals = locals.1;
    locals.0.retain(|name| !globals.contains(name));
    locals.0
}

/// Python's `re.match` of `pattern` on `text` for the patterns Pylint's
/// name options use: alternatives of literals, `.`, `.*`, `^` and `$`.
/// shortcut: any other regex syntax never matches; upgrade with a regex
/// engine when a configuration needs more.
fn rx_match(pattern: &str, text: &str) -> bool {
    fn at(pattern: &[char], text: &[char]) -> bool {
        match pattern {
            [] => true,
            ['$'] => text.is_empty(),
            ['.', '*', rest @ ..] => (0..=text.len()).any(|n| at(rest, &text[n..])),
            ['.', rest @ ..] => !text.is_empty() && at(rest, &text[1..]),
            [c, rest @ ..] if !"\\[](){}+?*|".contains(*c) => {
                text.first() == Some(c) && at(rest, &text[1..])
            }
            _ => false,
        }
    }
    let text: Vec<char> = text.chars().collect();
    pattern.split('|').any(|alt| {
        let alt: Vec<char> = alt.trim_start_matches('^').chars().collect();
        at(&alt, &text)
    })
}

#[cfg(test)]
mod tests {
    use super::rx_match;

    #[test]
    fn name_patterns() {
        let default = "_.*|^ignored_|^unused_";
        assert!(rx_match(default, "_"));
        assert!(rx_match(default, "_x"));
        assert!(rx_match(default, "unused_a"));
        assert!(!rx_match(default, "a_unused_"));
        assert!(!rx_match(default, "x"));
    }
}

/// Every id of the control-flow, statement and expression rules, for the
/// tests that compare a whole fixture with Pylint's findings.
#[cfg(test)]
pub(super) const BATCH_C: &[&str] = &[
    "E0701", "W0101", "W0105", "W0124", "W0125", "W0126", "W0135", "R1709", "R1726", "R1727",
    "R1738", "R1739", "W3601", "C0117", "C0200", "C0325", "R1713", "W4701", "E4702", "E4703",
    "W4801", "E0113", "R1905", "R1906", "E1901", "E1902", "E1903", "E1904", "E3102", "R1711",
    "R1710", "R0911", "W1113", "R0914", "R0913", "C0302", "R5102",
];

#[cfg(test)]
mod fixtures {
    use super::BATCH_C;
    use crate::native::classes::render;

    #[test]
    fn returns_and_unreachable_code() {
        let source = include_str!("fixtures/flow_returns_and_unreachable.py");
        let expected = include_str!("fixtures/flow_returns_and_unreachable.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }

    #[test]
    fn design_limits() {
        let source = include_str!("fixtures/flow_returns_and_unreachable.py");
        let expected = include_str!("fixtures/design_limits.expected").trim_end();
        let options = [("max-locals", "3"), ("max-args", "2"), ("max-returns", "2")];
        assert_eq!(render(source, &options, BATCH_C), expected);
    }

    #[test]
    fn exceptions_and_returns() {
        let source = include_str!("fixtures/flow_exceptions_and_returns.py");
        let expected = include_str!("fixtures/flow_exceptions_and_returns.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }
}
