//! The import statements of a file and what Pylint looks at around each: the
//! guards that silence a missing module (`TYPE_CHECKING`, a `sys.version_info`
//! test, a `try` that catches `ImportError`, `contextlib.suppress`) and the
//! fallback blocks. The resolution itself needs the whole run and is in
//! `project_phase`.
//!
//! shortcut: a name is taken for `ImportError`, `TYPE_CHECKING`, `typing`,
//! `contextlib` or `suppress` by its spelling and the file's own imports,
//! where Pylint infers it; upgrade with the contract's type inference.

use std::collections::HashSet;

use ferrolint_core::{LineIndex, Range};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{ExceptHandler, Expr, Stmt, StmtTry};
use ruff_text_size::Ranged;

/// One `import` or `from ... import` statement.
#[allow(clippy::struct_excessive_bools)]
pub struct Import {
    pub range: Range,
    /// `from`: the module as written (empty for `from . import x`) and the dots.
    pub from: Option<(String, u32)>,
    /// The dotted names of `import a.b, c`, or the imported names of a `from`.
    pub names: Vec<String>,
    /// Under an `if TYPE_CHECKING:` (any branch of it).
    pub type_checking: bool,
    /// Directly under an `if` that tests `sys.version_info` or `six.PY2/3`.
    pub sys_guard: bool,
    /// In a `try` that catches `ImportError`, or a `suppress(ImportError)`.
    pub ignores_import_error: bool,
    /// In a `try`/`except` that handles an import failure, or beside a
    /// fallback import.
    pub fallback: bool,
}

pub fn scan(body: &[Stmt], source: &str, index: &LineIndex) -> Vec<Import> {
    let mut scan = Scan {
        source,
        index,
        typing: HashSet::new(),
        contextlib: HashSet::new(),
        suppress: HashSet::new(),
        type_checking: None,
        frames: Vec::new(),
        suppressing: 0,
        guarded: 0,
        out: Vec::new(),
    };
    scan.bindings(body);
    scan.body(body, false);
    scan.out
}

/// The `try` or `except` clause nearest to a statement.
enum Frame {
    Body { ignores: bool, fallback: bool },
    Handler { fallback: bool },
}

struct Scan<'a> {
    source: &'a str,
    index: &'a LineIndex,
    /// The names that stand for the `typing` module.
    typing: HashSet<String>,
    contextlib: HashSet<String>,
    /// The names that stand for `contextlib.suppress`.
    suppress: HashSet<String>,
    /// Whether the first binding of `TYPE_CHECKING` is `typing`'s (or `False`).
    type_checking: Option<bool>,
    frames: Vec<Frame>,
    suppressing: usize,
    guarded: usize,
    out: Vec<Import>,
}

impl Scan<'_> {
    /// The names the file's imports and `TYPE_CHECKING` assignments give the
    /// helpers above, in source order.
    fn bindings(&mut self, body: &[Stmt]) {
        for stmt in body {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let local = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                        match alias.name.as_str() {
                            "typing" => self.typing.insert(local),
                            "contextlib" => self.contextlib.insert(local),
                            _ => false,
                        };
                    }
                }
                Stmt::ImportFrom(import) if import.level == 0 => {
                    let module = import.module.as_ref().map(ToString::to_string);
                    for alias in &import.names {
                        let local = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                        match (module.as_deref(), alias.name.as_str()) {
                            (Some("typing"), "TYPE_CHECKING") if local == "TYPE_CHECKING" => {
                                self.type_checking.get_or_insert(true);
                            }
                            (Some("contextlib"), "suppress") => {
                                self.suppress.insert(local);
                            }
                            _ => {}
                        }
                    }
                }
                Stmt::Assign(assign) => {
                    let named = assign
                        .targets
                        .iter()
                        .any(|t| matches!(t, Expr::Name(n) if n.id.as_str() == "TYPE_CHECKING"));
                    if named {
                        let falsy = matches!(&*assign.value, Expr::BooleanLiteral(b) if !b.value);
                        self.type_checking.get_or_insert(falsy);
                    }
                }
                Stmt::If(s) => {
                    self.bindings(&s.body);
                    s.elif_else_clauses
                        .iter()
                        .for_each(|c| self.bindings(&c.body));
                }
                Stmt::Try(t) => {
                    self.bindings(&t.body);
                    for ExceptHandler::ExceptHandler(h) in &t.handlers {
                        self.bindings(&h.body);
                    }
                    self.bindings(&t.orelse);
                    self.bindings(&t.finalbody);
                }
                Stmt::FunctionDef(f) => self.bindings(&f.body),
                Stmt::ClassDef(c) => self.bindings(&c.body),
                Stmt::With(w) => self.bindings(&w.body),
                _ => {}
            }
        }
    }

    fn is_type_checking(&self, test: &Expr) -> bool {
        match test {
            Expr::Name(n) => n.id.as_str() == "TYPE_CHECKING" && self.type_checking == Some(true),
            Expr::Attribute(a) => {
                a.attr.as_str() == "TYPE_CHECKING"
                    && matches!(&*a.value, Expr::Name(n) if self.typing.contains(n.id.as_str()))
            }
            _ => false,
        }
    }

    fn body(&mut self, body: &[Stmt], guard: bool) {
        for stmt in body {
            self.stmt(stmt, guard);
        }
    }

    fn stmt(&mut self, stmt: &Stmt, guard: bool) {
        match stmt {
            Stmt::Import(import) => {
                let names = import.names.iter().map(|a| a.name.to_string()).collect();
                self.push(stmt, None, names, guard);
            }
            Stmt::ImportFrom(import) => {
                let module = import.module.as_ref().map(ToString::to_string);
                let names = import.names.iter().map(|a| a.name.to_string()).collect();
                let from = (module.unwrap_or_default(), import.level);
                self.push(stmt, Some(from), names, guard);
            }
            Stmt::If(s) => {
                let mut tests = vec![&*s.test];
                self.branch(&s.body, &tests);
                for clause in &s.elif_else_clauses {
                    if let Some(test) = &clause.test {
                        tests.push(test);
                    }
                    self.branch(&clause.body, &tests);
                }
            }
            Stmt::Try(t) => self.try_(t),
            Stmt::With(w) => {
                let suppresses = w
                    .items
                    .iter()
                    .any(|item| self.suppresses_import_error(&item.context_expr));
                self.suppressing += usize::from(suppresses);
                self.body(&w.body, false);
                self.suppressing -= usize::from(suppresses);
            }
            Stmt::For(f) => {
                self.body(&f.body, false);
                self.body(&f.orelse, false);
            }
            Stmt::While(w) => {
                self.body(&w.body, false);
                self.body(&w.orelse, false);
            }
            Stmt::FunctionDef(f) => self.body(&f.body, false),
            Stmt::ClassDef(c) => self.body(&c.body, false),
            Stmt::Match(m) => m.cases.iter().for_each(|case| self.body(&case.body, false)),
            _ => {}
        }
    }

    /// One branch of an `if`, under the tests of the `if` and its `elif`s so far.
    fn branch(&mut self, body: &[Stmt], tests: &[&Expr]) {
        let type_checking = tests.iter().any(|test| self.is_type_checking(test));
        let guard = tests.last().is_some_and(|test| is_sys_guard(test));
        self.guarded += usize::from(type_checking);
        self.body(body, guard);
        self.guarded -= usize::from(type_checking);
    }

    fn try_(&mut self, t: &StmtTry) {
        let catching = |names: &[&str]| {
            t.handlers
                .iter()
                .any(|ExceptHandler::ExceptHandler(h)| catches(h.type_.as_deref(), names))
        };
        let ignores = catching(&["ImportError"]);
        let either = catching(&["ImportError", "ModuleNotFoundError"]);
        let is_import = |stmt: &Stmt| matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_));
        let handler_imports = t
            .handlers
            .iter()
            .any(|ExceptHandler::ExceptHandler(h)| h.body.iter().any(is_import));
        let in_body = t.body.iter().any(is_import);
        self.frames.push(Frame::Body {
            ignores,
            fallback: either || handler_imports,
        });
        self.body(&t.body, false);
        self.body(&t.orelse, false);
        self.body(&t.finalbody, false);
        self.frames.pop();
        self.frames.push(Frame::Handler {
            fallback: either || in_body,
        });
        for ExceptHandler::ExceptHandler(h) in &t.handlers {
            self.body(&h.body, false);
        }
        self.frames.pop();
    }

    /// Whether `expr` is a call of `contextlib.suppress` that names `ImportError`.
    fn suppresses_import_error(&self, expr: &Expr) -> bool {
        let Expr::Call(call) = expr else {
            return false;
        };
        let is_suppress = match &*call.func {
            Expr::Name(n) => self.suppress.contains(n.id.as_str()),
            Expr::Attribute(a) => {
                a.attr.as_str() == "suppress"
                    && matches!(&*a.value, Expr::Name(n) if self.contextlib.contains(n.id.as_str()))
            }
            _ => false,
        };
        let named = |arg: &Expr| matches!(arg, Expr::Name(n) if n.id.as_str() == "ImportError");
        is_suppress
            && call.arguments.args.iter().any(|arg| match arg {
                Expr::Tuple(tuple) => tuple.elts.iter().any(named),
                other => named(other),
            })
    }

    fn push(&mut self, stmt: &Stmt, from: Option<(String, u32)>, names: Vec<String>, guard: bool) {
        let (ignores, fallback) = match self.frames.last() {
            Some(Frame::Body { ignores, fallback }) => (*ignores, *fallback),
            Some(Frame::Handler { fallback }) => (false, *fallback),
            None => (false, false),
        };
        let range = stmt.range();
        self.out.push(Import {
            range: self
                .index
                .range(self.source, range.start().to_u32(), range.end().to_u32()),
            from,
            names,
            type_checking: self.guarded > 0,
            sys_guard: guard,
            ignores_import_error: ignores || self.suppressing > 0,
            fallback,
        });
    }
}

/// Whether the `except` clause with this `type_` names one of `names`, as
/// astroid's `ExceptHandler.catch` reads it: any `Name` inside the expression.
fn catches(type_: Option<&Expr>, names: &[&str]) -> bool {
    struct Names<'n>(&'n [&'n str], bool);
    impl<'a> Visitor<'a> for Names<'_> {
        fn visit_expr(&mut self, expr: &'a Expr) {
            if let Expr::Name(n) = expr {
                self.1 |= self.0.contains(&n.id.as_str());
            }
            visitor::walk_expr(self, expr);
        }
    }
    let mut found = Names(names, false);
    if let Some(expr) = type_ {
        found.visit_expr(expr);
    }
    found.1
}

/// Pylint's `is_sys_guard`: a comparison of `sys.version_info` (or a part of
/// it), or `six.PY2` / `six.PY3`.
fn is_sys_guard(test: &Expr) -> bool {
    let dotted = |expr: &Expr| match expr {
        Expr::Attribute(a) => match &*a.value {
            Expr::Name(n) => Some(format!("{}.{}", n.id, a.attr)),
            _ => None,
        },
        _ => None,
    };
    match test {
        Expr::Compare(c) => {
            let value = match c.operands.first() {
                Some(Expr::Subscript(s)) => &*s.value,
                Some(other) => other,
                None => return false,
            };
            dotted(value).is_some_and(|name| name == "sys.version_info")
        }
        Expr::Attribute(_) => {
            dotted(test).is_some_and(|name| name == "six.PY2" || name == "six.PY3")
        }
        _ => false,
    }
}
