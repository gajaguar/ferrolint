//! The kind of value a call of a function gives back, as Pylint infers it from the
//! function's `return` statements: `f(...) == []` is `C1803` when `f` returns lists.
//!
//! shortcut: the function is one of the file or one the file imports by name from the
//! project; every `return` gives a list, tuple, dict, set, string or bytes display, a call
//! of the builtin of one of them (or of `defaultdict`, `OrderedDict` or `Counter` imported
//! from `collections`), a conditional of two of the same kind, or a name whose every
//! binding in the function is a plain assignment of one. A parameter, an augmented
//! assignment, a loop target and the like make a name unknown, where Pylint follows the
//! flow (and the arguments of the call). A decorated, async or generator function is not
//! read, nor are methods. Upgrade with a call inference.

use std::collections::HashMap;

use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandler, Expr, ExprContext, Parameters, Pattern, Stmt, StmtFunctionDef,
};

use super::booleaness::Kind;
use super::flow::own_returns;
use super::statements::is_generator;

/// What the function returns, or the `element`th item of the tuple it returns.
pub(super) fn return_kind(
    func: &StmtFunctionDef,
    module: &[Stmt],
    element: Option<usize>,
) -> Option<Kind> {
    if !func.decorator_list.is_empty() || func.is_async || is_generator(func) {
        return None;
    }
    let returns = own_returns(&func.body);
    if returns.is_empty() {
        return None;
    }
    let mut locals = Locals(HashMap::new());
    locals.parameters(&func.parameters);
    locals.visit_body(&func.body);
    let mut found = None;
    for statement in returns {
        // A bare `return` gives `None`, a type of its own.
        let value = statement.value.as_deref()?;
        let value = match (element, value) {
            (None, value) => value,
            (Some(at), Expr::Tuple(tuple)) if !tuple.elts.iter().any(Expr::is_starred_expr) => {
                tuple.elts.get(at)?
            }
            _ => return None,
        };
        let kind = locals.kind(value, module, 4)?;
        if *found.get_or_insert(kind) != kind {
            return None;
        }
    }
    found
}

enum Binding<'a> {
    Value(&'a Expr),
    Other,
}

/// The bindings of each name of a function, outside the functions and lambdas inside it.
struct Locals<'a>(HashMap<&'a str, Vec<Binding<'a>>>);

impl<'a> Locals<'a> {
    fn add(&mut self, name: &'a str, binding: Binding<'a>) {
        self.0.entry(name).or_default().push(binding);
    }

    fn parameters(&mut self, parameters: &'a Parameters) {
        for parameter in parameters {
            self.add(parameter.name().as_str(), Binding::Other);
        }
    }

    fn kind(&self, expr: &Expr, module: &[Stmt], depth: u8) -> Option<Kind> {
        match expr {
            Expr::List(_) => Some(Kind::List),
            Expr::Tuple(_) => Some(Kind::Tuple),
            Expr::Dict(_) => Some(Kind::Dict),
            Expr::Set(_) => Some(Kind::Set),
            Expr::StringLiteral(_) => Some(Kind::Str),
            Expr::BytesLiteral(_) => Some(Kind::Bytes),
            Expr::If(branches) => {
                let body = self.kind(&branches.body, module, depth)?;
                (body == self.kind(&branches.orelse, module, depth)?).then_some(body)
            }
            Expr::Call(call) => {
                let Expr::Name(function) = &*call.func else {
                    return None;
                };
                let name = function.id.as_str();
                if self.0.contains_key(name) {
                    return None;
                }
                match name {
                    "list" => Some(Kind::List),
                    "tuple" => Some(Kind::Tuple),
                    "dict" => Some(Kind::Dict),
                    "set" => Some(Kind::Set),
                    "str" => Some(Kind::Str),
                    "defaultdict" | "OrderedDict" | "Counter" if from_collections(module, name) => {
                        Some(Kind::Dict)
                    }
                    _ => None,
                }
            }
            Expr::Name(name) if depth > 0 => {
                let mut kinds = self
                    .0
                    .get(name.id.as_str())?
                    .iter()
                    .map(|binding| match binding {
                        Binding::Value(value) => self.kind(value, module, depth - 1),
                        Binding::Other => None,
                    });
                let first = kinds.next()??;
                kinds.all(|kind| kind == Some(first)).then_some(first)
            }
            _ => None,
        }
    }
}

/// Whether the module imports `name` from `collections`, without an alias.
fn from_collections(module: &[Stmt], name: &str) -> bool {
    module.iter().any(|stmt| match stmt {
        Stmt::ImportFrom(import) => {
            import.level == 0
                && import
                    .module
                    .as_ref()
                    .is_some_and(|m| m.as_str() == "collections")
                && import
                    .names
                    .iter()
                    .any(|alias| alias.name.as_str() == name && alias.asname.is_none())
        }
        _ => false,
    })
}

impl<'a> Visitor<'a> for Locals<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(def) => self.add(def.name.as_str(), Binding::Other),
            Stmt::ClassDef(def) => self.add(def.name.as_str(), Binding::Other),
            Stmt::Assign(assign) => {
                if let [Expr::Name(target)] = &*assign.targets {
                    self.add(target.id.as_str(), Binding::Value(&assign.value));
                    self.visit_expr(&assign.value);
                } else {
                    visitor::walk_stmt(self, stmt);
                }
            }
            Stmt::AnnAssign(assign) => match (&*assign.target, &assign.value) {
                (Expr::Name(target), Some(value)) => {
                    self.add(target.id.as_str(), Binding::Value(value));
                    self.visit_expr(value);
                }
                _ => visitor::walk_stmt(self, stmt),
            },
            Stmt::Import(import) => {
                for alias in &import.names {
                    let bound = alias.asname.as_ref().unwrap_or(&alias.name);
                    self.add(bound.split('.').next().unwrap_or_default(), Binding::Other);
                }
            }
            Stmt::ImportFrom(import) => {
                for alias in &import.names {
                    self.add(
                        alias.asname.as_ref().unwrap_or(&alias.name).as_str(),
                        Binding::Other,
                    );
                }
            }
            Stmt::Global(global) => {
                for name in &global.names {
                    self.add(name.as_str(), Binding::Other);
                }
            }
            Stmt::Nonlocal(nonlocal) => {
                for name in &nonlocal.names {
                    self.add(name.as_str(), Binding::Other);
                }
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            self.add(name.as_str(), Binding::Other);
        }
        self.visit_body(&handler.body);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        match pattern {
            Pattern::MatchAs(p) => {
                if let Some(name) = &p.name {
                    self.add(name.as_str(), Binding::Other);
                }
            }
            Pattern::MatchStar(p) => {
                if let Some(name) = &p.name {
                    self.add(name.as_str(), Binding::Other);
                }
            }
            Pattern::MatchMapping(p) => {
                if let Some(name) = &p.rest {
                    self.add(name.as_str(), Binding::Other);
                }
            }
            _ => {}
        }
        visitor::walk_pattern(self, pattern);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Lambda(_) => {}
            Expr::Name(name) if name.ctx != ExprContext::Load => {
                self.add(name.id.as_str(), Binding::Other);
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}
