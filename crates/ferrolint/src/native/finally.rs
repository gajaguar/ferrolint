//! `W0134` return-in-finally and `W0137` break-in-finally.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Stmt, StmtTry};

use super::{Checker, Ctx, rule};

const RETURN: &str = "W0134";
const BREAK: &str = "W0137";

pub struct Finally;

impl Checker for Finally {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                RETURN,
                "return-in-finally",
                "`return` statement in a `finally` block.",
            ),
            rule(
                BREAK,
                "break-in-finally",
                "`break` statement in a `finally` block that leaves its loop.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(RETURN) && !enabled.contains(BREAK) {
            return;
        }
        let mut walk = Walk {
            ctx,
            enabled,
            out,
            finally: 0,
            loops: Vec::new(),
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'c> {
    ctx: &'a Ctx<'c>,
    enabled: &'a BTreeSet<String>,
    out: &'a mut Vec<Diagnostic>,
    /// How many `finally` blocks enclose the statement, within its function.
    finally: usize,
    /// For each enclosing loop, the `finally` depth it started at.
    loops: Vec<usize>,
}

impl Walk<'_, '_> {
    /// Run `body` as the start of a new function or class: what encloses it
    /// does not apply inside.
    fn fresh(&mut self, body: impl FnOnce(&mut Self)) {
        let (finally, loops) = (self.finally, std::mem::take(&mut self.loops));
        self.finally = 0;
        body(self);
        self.finally = finally;
        self.loops = loops;
    }
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                self.fresh(|walk| visitor::walk_stmt(walk, stmt));
            }
            Stmt::Try(StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                ..
            }) => {
                self.visit_body(body);
                for handler in handlers {
                    self.visit_except_handler(handler);
                }
                self.visit_body(orelse);
                self.finally += 1;
                self.visit_body(finalbody);
                self.finally -= 1;
            }
            Stmt::For(_) | Stmt::While(_) => {
                self.loops.push(self.finally);
                visitor::walk_stmt(self, stmt);
                self.loops.pop();
            }
            Stmt::Return(_) if self.finally > 0 && self.enabled.contains(RETURN) => {
                self.out.push(self.ctx.report(
                    RETURN,
                    stmt,
                    "'return' shouldn't be used inside a 'finally' block",
                ));
            }
            // A `break` leaves a loop that started outside the `finally`.
            Stmt::Break(_)
                if self.enabled.contains(BREAK)
                    && self.loops.last().is_some_and(|&start| start < self.finally) =>
            {
                self.out.push(self.ctx.report(
                    BREAK,
                    stmt,
                    "'break' shouldn't be used inside a 'finally' block",
                ));
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::lint;

    #[test]
    fn return_and_break_in_finally() {
        let source = "\
def f():
    try:
        pass
    finally:
        return 1

for _ in []:
    try:
        pass
    finally:
        break

try:
    pass
finally:
    for _ in []:
        break
    def g():
        return 2
";
        let found = lint(source);
        assert_eq!(found, [(5, "W0134".to_owned()), (11, "W0137".to_owned())]);
    }
}
