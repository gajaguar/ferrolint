//! `W0135` contextmanager-generator-missing-cleanup.
//!
//! shortcut: Pylint infers what a `with` item is. Here it is the call of a
//! generator function of this file, by name or as `self.method()` of the
//! enclosing class. Upgrade with a call inference.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    ExceptHandler, Expr, Stmt, StmtClassDef, StmtFunctionDef, StmtTry, StmtWith,
};

use super::classes::{Env, Local, blocks, locals, methods};
use super::{Checker, Ctx, rule};

const MISSING_CLEANUP: &str = "W0135";

pub struct ContextManagers;

impl Checker for ContextManagers {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            MISSING_CLEANUP,
            "contextmanager-generator-missing-cleanup",
            "A generator that yields inside a context manager that does not clean up on `GeneratorExit`.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(MISSING_CLEANUP) {
            return;
        }
        let env = Env::new(ctx);
        let mut walk = Walk {
            env: &env,
            out,
            classes: Vec::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e> {
    env: &'e Env<'a>,
    out: &'e mut Vec<Diagnostic>,
    classes: Vec<&'a StmtClassDef>,
}

/// Everything of one kind in a subtree, nested functions included.
#[derive(Default)]
struct Found<'a> {
    withs: Vec<&'a StmtWith>,
    tries: Vec<&'a StmtTry>,
    /// For each `yield`, whether it yields nothing or a constant.
    yields: Vec<bool>,
}

impl<'a> Visitor<'a> for Found<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::With(w) => self.withs.push(w),
            Stmt::Try(t) if !t.is_star => self.tries.push(t),
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Yield(y) => self.yields.push(y.value.as_deref().is_none_or(is_constant)),
            Expr::YieldFrom(_) => self.yields.push(false),
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

fn is_constant(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::NumberLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::BooleanLiteral(_)
            | Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
    )
}

fn found(body: &[Stmt]) -> Found<'_> {
    let mut found = Found::default();
    found.visit_body(body);
    found
}

impl<'a> Walk<'a, '_> {
    /// The generator function that a `with` item calls.
    fn callee(&self, expr: &Expr) -> Option<&'a StmtFunctionDef> {
        let Expr::Call(call) = expr else { return None };
        let func = match &*call.func {
            Expr::Name(name) => self.env.func_of(name)?,
            Expr::Attribute(a) if matches!(&*a.value, Expr::Name(n) if n.id.as_str() == "self") => {
                let class = self.classes.last()?;
                let bound: Vec<Local> = locals(&class.body)
                    .into_iter()
                    .filter(|(n, _)| n == a.attr.as_str())
                    .map(|(_, k)| k)
                    .collect();
                if bound != [Local::Function] {
                    return None;
                }
                methods(&class.body)
                    .into_iter()
                    .find(|f| f.name.as_str() == a.attr.as_str())?
            }
            _ => return None,
        };
        (!found(&func.body).yields.is_empty()).then_some(func)
    }

    fn check_function(&mut self, func: &'a StmtFunctionDef) {
        let inside = found(&func.body);
        // the `yield`s inside the `with` statements of the function
        let yields: Vec<bool> = inside
            .withs
            .iter()
            .flat_map(|w| found(&w.body).yields)
            .collect();
        if yields.is_empty() {
            return;
        }
        for with in &inside.withs {
            for item in &with.items {
                if item.optional_vars.is_some()
                    && let Some(callee) = self.callee(&item.context_expr)
                    && fails_cleanup(self.env, callee, &yields)
                {
                    self.out.push(self.env.ctx.report(
                        MISSING_CLEANUP,
                        *with,
                        format!(
                            "The context used in function '{}' will not be exited.",
                            func.name
                        ),
                    ));
                }
            }
        }
    }
}

/// Pylint's `_node_fails_contextmanager_cleanup`.
fn fails_cleanup(env: &Env<'_>, callee: &StmtFunctionDef, caller_yields: &[bool]) -> bool {
    if caller_yields.iter().any(|&plain| plain) {
        return false;
    }
    let inside = found(&callee.body);
    // a lone `yield` that nothing follows leaves nothing to clean up
    if inside.yields.len() == 1 && last_statement(&callee.body) {
        return false;
    }
    let with_yield: Vec<&&StmtTry> = inside
        .tries
        .iter()
        .filter(|t| {
            contains_yield(&t.body)
                || t.handlers.iter().any(|h| {
                    let ExceptHandler::ExceptHandler(h) = h;
                    contains_yield(&h.body)
                })
                || contains_yield(&t.orelse)
                || contains_yield(&t.finalbody)
        })
        .collect();
    if with_yield.is_empty() {
        return true;
    }
    if with_yield.iter().all(|t| !t.finalbody.is_empty()) {
        return false;
    }
    !with_yield.iter().all(|t| {
        t.handlers.iter().any(|h| {
            let ExceptHandler::ExceptHandler(h) = h;
            h.type_.as_deref().is_none_or(|type_| {
                env.qualified(type_)
                    .is_some_and(|q| matches!(q.as_str(), "GeneratorExit" | "Exception"))
            })
        })
    })
}

fn contains_yield(body: &[Stmt]) -> bool {
    !found(body).yields.is_empty()
}

/// Whether the statement that holds the `yield` is the last of its list,
/// and so on up to the function.
fn last_statement(body: &[Stmt]) -> bool {
    let Some(at) = body
        .iter()
        .position(|s| contains_yield(std::slice::from_ref(s)))
    else {
        return true;
    };
    at + 1 == body.len()
        && blocks(&body[at])
            .into_iter()
            .find(|block| contains_yield(block))
            .is_none_or(last_statement)
}

impl<'a> Visitor<'a> for Walk<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::ClassDef(class) => {
                self.classes.push(class);
                visitor::walk_stmt(self, stmt);
                self.classes.pop();
            }
            Stmt::FunctionDef(func) => {
                self.check_function(func);
                visitor::walk_stmt(self, stmt);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;
    use crate::native::flow::BATCH_C;

    #[test]
    fn contexts_that_are_not_exited() {
        let source = include_str!("fixtures/contextmanager_generators.py");
        let expected = include_str!("fixtures/contextmanager_generators.expected").trim_end();
        assert_eq!(render(source, &[], BATCH_C), expected);
    }
}
