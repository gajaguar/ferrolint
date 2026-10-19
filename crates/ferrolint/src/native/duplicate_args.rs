//! `E0108` duplicate-argument-name.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Stmt, StmtFunctionDef};

use super::{Checker, Ctx, rule};

const DUPLICATE: &str = "E0108";

pub struct DuplicateArgs;

impl Checker for DuplicateArgs {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![rule(
            DUPLICATE,
            "duplicate-argument-name",
            "A function definition names two of its arguments alike.",
        )]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if enabled.contains(DUPLICATE) {
            Walk { ctx, out }.visit_body(&ctx.parsed.syntax().body);
        }
    }
}

struct Walk<'a, 'o> {
    ctx: &'a Ctx<'a>,
    out: &'o mut Vec<Diagnostic>,
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::FunctionDef(StmtFunctionDef { parameters, .. }) = stmt {
            let mut seen = BTreeSet::new();
            // `iter_non_variadic_params` and the variadics, in signature order
            for parameter in parameters.iter().map(|p| p.name()) {
                if !seen.insert(parameter.as_str()) {
                    let message = format!(
                        "Duplicate argument name '{parameter}' in function definition"
                    );
                    self.out.push(self.ctx.report(DUPLICATE, parameter, message));
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::classes::render;

    #[test]
    fn duplicate_argument_name() {
        let source = include_str!("fixtures/duplicate_args.py");
        let expected = include_str!("fixtures/duplicate_args.expected").trim_end();
        assert_eq!(render(source, &[], &["E0108"]), expected);
    }
}
