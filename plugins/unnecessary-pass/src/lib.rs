//! A port of Pylint's `unnecessary-pass` (W0107): a `pass` in a block
//! that has other statements or a docstring. Comes with a fix.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "STM003";

#[derive(Default)]
struct UnnecessaryPass;

impl Checker for UnnecessaryPass {
    fn info() -> Info {
        Info::new("unnecessary-pass", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "unnecessary-pass",
                "A pass statement in a block that needs none.",
            )
            .pylint("W0107", "unnecessary-pass"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::Pass {
            return;
        }
        let Some(parent) = cx.parent(node) else {
            return;
        };
        // Only blocks whose statements all belong to one body; `for`,
        // `while` and `try` keep several bodies under one node.
        let single_body = matches!(
            parent.kind,
            NodeKind::FunctionDef
                | NodeKind::ClassDef
                | NodeKind::If
                | NodeKind::ElifElseClause
                | NodeKind::With
                | NodeKind::ExceptHandler
                | NodeKind::MatchCase
        );
        let statements = cx
            .children(parent)
            .filter(|child| Context::is_statement(child))
            .count();
        if single_body && statements > 1 {
            let fix = Fix::delete(node.range, "Remove the pass statement");
            cx.report(Diagnostic::new(CODE, "Unnecessary pass statement", node.range).fix(fix));
        }
    }
}

export_checker!(UnnecessaryPass);
