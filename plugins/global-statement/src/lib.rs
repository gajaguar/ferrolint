//! A port of Pylint's `global-statement` (W0603): a `global` statement
//! inside a function or class. One at module level is Pylint's W0604,
//! which this plugin leaves alone.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "STM001";

#[derive(Default)]
struct GlobalStatement;

impl Checker for GlobalStatement {
    fn info() -> Info {
        Info::new("global-statement", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "global-statement",
                "A global statement rebinds module names from a function.",
            )
            .pylint("W0603", "global-statement"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind == NodeKind::Global && cx.scope(node).kind != ScopeKind::Module {
            cx.report(Diagnostic::new(
                CODE,
                "Using the global statement",
                node.range,
            ));
        }
    }
}

export_checker!(GlobalStatement);
