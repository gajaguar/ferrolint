//! A port of Pylint's `undefined-variable` (E0602): a name read that no
//! scope and no builtin defines. A module with a star import is left
//! alone, since it may define anything.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "NAM001";

/// Names Python defines in every module without a binding in the source.
const MODULE_GLOBALS: &[&str] = &[
    "__annotations__",
    "__builtins__",
    "__cached__",
    "__class__",
    "__dict__",
    "__doc__",
    "__file__",
    "__loader__",
    "__module__",
    "__name__",
    "__package__",
    "__path__",
    "__qualname__",
    "__spec__",
];

#[derive(Default)]
struct UndefinedVariable {
    star_import: bool,
    undefined: Vec<(String, Range)>,
}

impl Checker for UndefinedVariable {
    fn info() -> Info {
        Info::new("undefined-variable", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "undefined-variable",
                "A name is read that nothing defines.",
            )
            .severity(Severity::Error)
            .pylint("E0602", "undefined-variable"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        match node.kind {
            NodeKind::Alias if node.name == Some("*") => self.star_import = true,
            NodeKind::Name if node.context == ExprContext::Load => {
                let Some(name) = node.name else {
                    return;
                };
                if MODULE_GLOBALS.contains(&name) || cx.resolve(node).is_some() {
                    return;
                }
                self.undefined.push((name.to_owned(), node.range));
            }
            _ => {}
        }
    }

    fn end_file(&mut self, cx: &mut Context<'_>) {
        if self.star_import {
            return;
        }
        for (name, range) in self.undefined.drain(..) {
            cx.report(Diagnostic::new(
                CODE,
                format!("Undefined variable '{name}'"),
                range,
            ));
        }
    }
}

export_checker!(UndefinedVariable);
