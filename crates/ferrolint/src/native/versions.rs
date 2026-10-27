//! `W2601` to `W2607`: syntax newer than the configured `py-version`.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{
    Decorator, ExceptHandler, Expr, ExprGenerator, ExprListComp, ExprSetComp, Stmt, StmtRaise,
    StmtTry, TypeParam,
};
use ruff_text_size::Ranged;

use super::classes::Env;
use super::{Checker, Ctx, rule};

const F_STRING: &str = "W2601";
const FINAL: &str = "W2602";
const GROUPS: &str = "W2603";
const GENERIC: &str = "W2604";
const WALRUS: &str = "W2605";
const POSONLY: &str = "W2606";
const UNPACKING: &str = "W2607";

pub struct Versions;

impl Checker for Versions {
    fn rules(&self) -> Vec<RuleInfo> {
        let all = [
            (
                F_STRING,
                "using-f-string-in-unsupported-version",
                "F-strings",
            ),
            (
                FINAL,
                "using-final-decorator-in-unsupported-version",
                "typing.final",
            ),
            (
                GROUPS,
                "using-exception-groups-in-unsupported-version",
                "Exception groups",
            ),
            (
                GENERIC,
                "using-generic-type-syntax-in-unsupported-version",
                "Generic type syntax",
            ),
            (
                WALRUS,
                "using-assignment-expression-in-unsupported-version",
                "Assignment expressions",
            ),
            (
                POSONLY,
                "using-positional-only-args-in-unsupported-version",
                "Positional-only arguments",
            ),
            (
                UNPACKING,
                "using-comprehension-unpacking-in-unsupported-version",
                "Unpacking in comprehensions",
            ),
        ];
        all.iter()
            .map(|(id, symbol, what)| {
                rule(
                    id,
                    symbol,
                    &format!("{what} used below the `py-version` that supports them."),
                )
            })
            .collect()
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        // without a configured version there is nothing to compare against
        let Some(version) = ctx.py_version else {
            return;
        };
        let since = [
            (F_STRING, (3, 6)),
            (FINAL, (3, 8)),
            (GROUPS, (3, 11)),
            (GENERIC, (3, 12)),
            (WALRUS, (3, 8)),
            (POSONLY, (3, 8)),
            (UNPACKING, (3, 15)),
        ];
        let active = since
            .into_iter()
            .filter(|(id, since)| enabled.contains(*id) && version < *since)
            .map(|(id, _)| id)
            .collect();
        let mut walk = Walk {
            ctx,
            env: Env::new(ctx),
            out,
            active,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

struct Walk<'a, 'c> {
    ctx: &'a Ctx<'c>,
    env: Env<'a>,
    out: &'a mut Vec<Diagnostic>,
    /// The enabled rules whose version is above `py-version`.
    active: BTreeSet<&'static str>,
}

impl Walk<'_, '_> {
    fn push(&mut self, id: &str, node: &impl Ranged, what: &str) {
        let message =
            format!("{what} not supported by all versions included in the py-version setting");
        self.out.push(self.ctx.report(id, node, message));
    }

    fn is_group(expr: &Expr) -> bool {
        matches!(expr, Expr::Name(name) if name.id.as_str() == "ExceptionGroup")
    }
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Try(StmtTry { is_star: true, .. }) if self.active.contains(GROUPS) => {
                self.push(GROUPS, stmt, "Exception groups are");
            }
            Stmt::Raise(StmtRaise { exc: Some(exc), .. })
                if self.active.contains(GROUPS)
                    && matches!(&**exc, Expr::Call(call) if Self::is_group(&call.func)) =>
            {
                self.push(GROUPS, stmt, "Exception groups are");
            }
            Stmt::FunctionDef(func)
                if self.active.contains(POSONLY) && !func.parameters.posonlyargs.is_empty() =>
            {
                // Pylint reports the arguments node, on the line of the `def`
                self.push(POSONLY, &func.name, "Positional-only arguments are");
            }
            Stmt::TypeAlias(_) if self.active.contains(GENERIC) => {
                self.push(GENERIC, stmt, "Generic type syntax (PEP 695) is");
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_except_handler(&mut self, handler: &'a ExceptHandler) {
        let ExceptHandler::ExceptHandler(inner) = handler;
        if self.active.contains(GROUPS) && inner.type_.as_deref().is_some_and(Self::is_group) {
            self.push(GROUPS, handler, "Exception groups are");
        }
        visitor::walk_except_handler(self, handler);
    }

    fn visit_decorator(&mut self, decorator: &'a Decorator) {
        let name = self.env.qualified(&decorator.expression);
        if self.active.contains(FINAL) && name.as_deref() == Some("typing.final") {
            self.push(FINAL, &decorator.expression, "typing.final is");
        }
        visitor::walk_decorator(self, decorator);
    }

    fn visit_type_param(&mut self, param: &'a TypeParam) {
        // Pylint has no visitor for ParamSpec
        if self.active.contains(GENERIC) && !matches!(param, TypeParam::ParamSpec(_)) {
            self.push(GENERIC, param, "Generic type syntax (PEP 695) is");
        }
        visitor::walk_type_param(self, param);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::FString(_) if self.active.contains(F_STRING) => {
                self.push(F_STRING, expr, "F-strings are");
            }
            Expr::Lambda(lambda)
                if self.active.contains(POSONLY)
                    && lambda
                        .parameters
                        .as_ref()
                        .is_some_and(|p| !p.posonlyargs.is_empty()) =>
            {
                self.push(POSONLY, expr, "Positional-only arguments are");
            }
            Expr::Named(_) if self.active.contains(WALRUS) => {
                self.push(WALRUS, expr, "Assignment expression is");
            }
            Expr::ListComp(ExprListComp { elt, .. })
            | Expr::SetComp(ExprSetComp { elt, .. })
            | Expr::Generator(ExprGenerator { elt, .. })
                if self.active.contains(UNPACKING) && elt.is_starred_expr() =>
            {
                self.push(UNPACKING, expr, "Unpacking in comprehensions is");
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrolint_core::LineIndex;

    fn render(source: &str, version: Option<(u32, u32)>) -> String {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let index = LineIndex::new(source);
        let flat = crate::ast::flatten(source, &index, parsed.syntax());
        let model =
            crate::semantic::Model::build(parsed.syntax(), &flat.nodes, source, &index, None);
        let ctx = Ctx {
            source,
            index: &index,
            parsed: &parsed,
            flat: &flat,
            model: &model,
            options: &[],
            py_version: version,
            module: "mod",
            path: std::path::Path::new("mod.py"),
            roots: &[],
            types: None,
        };
        let enabled = Versions.rules().into_iter().map(|r| r.code).collect();
        let mut out = Vec::new();
        Versions.check(&ctx, &enabled, &mut out);
        let mut found: Vec<_> = out
            .iter()
            .map(|d| (d.range.start.line, d.code.clone(), d.message.clone()))
            .collect();
        found.sort();
        found
            .iter()
            .map(|(line, code, message)| format!("{line}:{code}:{message}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn below_every_threshold() {
        let source = include_str!("fixtures/versions.py");
        let expected = include_str!("fixtures/versions.expected").trim_end();
        assert_eq!(render(source, Some((3, 5))), expected);
    }

    #[test]
    fn newer_version_reports_only_what_is_still_unsupported() {
        let rendered = render(include_str!("fixtures/versions.py"), Some((3, 11)));
        assert!(rendered.contains("W2604"));
        assert!(
            rendered
                .lines()
                .all(|l| l.contains("W2604") || l.contains("W2607"))
        );
    }

    #[test]
    fn unset_version_reports_nothing() {
        assert_eq!(render(include_str!("fixtures/versions.py"), None), "");
    }
}
