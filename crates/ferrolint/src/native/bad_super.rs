//! `E1003` bad-super-call.
//!
//! shortcut: a first argument that is not a class the project holds (a
//! parameter, a call, a base that cannot be read) is skipped, where Pylint
//! reports the name of an argument it cannot infer; upgrade with inference.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprCall, Stmt, StmtClassDef};

use super::classes::Env;
use super::hierarchy::{Class, Hierarchy};
use super::{Checker, Ctx, rule};

const BAD_SUPER: &str = "E1003";

pub struct BadSuper;

impl Checker for BadSuper {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            BAD_SUPER,
            "bad-super-call",
            "A class other than the current one or its ancestors is the first argument of `super()`.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if !enabled.contains(BAD_SUPER) {
            return;
        }
        let env = Env::new(ctx);
        let hierarchy = Hierarchy::new(&env);
        let mut walk = Walk {
            ctx,
            hierarchy: &hierarchy,
            out,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'e, 'o> {
    ctx: &'a Ctx<'a>,
    hierarchy: &'e Hierarchy<'a, 'e>,
    out: &'o mut Vec<Diagnostic>,
}

impl<'a> Visitor<'a> for Walk<'a, '_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::ClassDef(class) = stmt {
            for method in &class.body {
                if let Stmt::FunctionDef(func) = method {
                    let mut calls = Calls { class, walk: self };
                    calls.visit_body(&func.body);
                    for decorator in &func.decorator_list {
                        calls.visit_decorator(decorator);
                    }
                    calls.visit_parameters(&func.parameters);
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// The calls of one method, not those of the classes nested in it.
struct Calls<'w, 'a, 'e, 'o> {
    class: &'a StmtClassDef,
    walk: &'w mut Walk<'a, 'e, 'o>,
}

impl<'a> Visitor<'a> for Calls<'_, 'a, '_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if !matches!(stmt, Stmt::ClassDef(_)) {
            visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr
            && let Expr::Attribute(attribute) = &*call.func
            && let Expr::Call(sup) = &*attribute.value
        {
            self.check(sup);
        }
        visitor::walk_expr(self, expr);
    }
}

impl Calls<'_, '_, '_, '_> {
    fn check(&mut self, sup: &ExprCall) {
        let is_super = matches!(&*sup.func, Expr::Name(name) if name.id.as_str() == "super");
        let Some(first) = sup.arguments.args.first().filter(|_| is_super) else {
            return;
        };
        let shown = match first {
            Expr::Call(call) if matches!(&*call.func, Expr::Name(n) if n.id.as_str() == "type") => {
                "type".to_owned()
            }
            Expr::Attribute(a)
                if a.attr.as_str() == "__class__"
                    && matches!(sup.arguments.args.get(1), Some(Expr::Name(n)) if n.id.as_str() == "self") =>
            {
                "self.__class__".to_owned()
            }
            _ => {
                let hierarchy = self.walk.hierarchy;
                let current = Class::Local(self.class);
                let Some(target) = hierarchy.resolve(&current, first) else {
                    return;
                };
                let Some(mro) = hierarchy.mro(&current) else {
                    return;
                };
                let key = target.key();
                if mro.iter().any(|class| class.key() == key) {
                    return;
                }
                match target.def() {
                    Some(def) => def.name.to_string(),
                    None => return,
                }
            }
        };
        let message = format!("Bad first argument '{shown}' given to super()");
        self.walk
            .out
            .push(self.walk.ctx.report(BAD_SUPER, sup, message));
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn bad_super_call() {
        let source = include_str!("fixtures/bad_super.py");
        let expected = include_str!("fixtures/bad_super.expected").trim_end();
        assert_eq!(render(source, &[], &["E1003"]), expected);
    }
}
