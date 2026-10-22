//! A port of Pylint's `bare-except` (W0702): an `except:` clause that
//! names no exception type and does not re-raise.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "EXC001";

#[derive(Default)]
struct BareExcept;

impl Checker for BareExcept {
    fn info() -> Info {
        Info::new("bare-except", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "bare-except",
                "An except clause catches every exception.",
            )
            .pylint("W0702", "bare-except"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::ExceptHandler {
            return;
        }
        // A typed handler's first child is its type expression.
        let typed = cx
            .children(node)
            .next()
            .is_some_and(|first| !Context::is_statement(first));
        // Pylint's `_is_raising`: a top-level `raise` in the body is no finding.
        if !typed && !cx.children(node).any(|child| child.kind == NodeKind::Raise) {
            let header = Range {
                start: node.range.start,
                end: node.range.start,
            };
            cx.report(Diagnostic::new(
                CODE,
                "No exception type(s) specified",
                header,
            ));
        }
    }
}

export_checker!(BareExcept);
