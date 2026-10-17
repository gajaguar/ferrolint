//! A port of Pylint's `not-callable` (E1102): a call whose callee the
//! host's type inference resolves to an instance of a builtin type that
//! cannot be called, such as `x = 1; x()`.
//!
//! The plugin asks the `inference` interface of contract 0.2.1 and
//! reports nothing when the answer is unresolved, so a callee it cannot
//! follow (a parameter, an attribute, a value from another module) never
//! yields a finding. Unlike Pylint it does not look
//! at user classes, so an instance of a class without `__call__` passes.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "CAL001";

/// Builtin types whose instances have no `__call__`.
const NOT_CALLABLE: &[&str] = &[
    "NoneType",
    "bool",
    "bytearray",
    "bytes",
    "complex",
    "dict",
    "ellipsis",
    "float",
    "frozenset",
    "int",
    "list",
    "range",
    "set",
    "slice",
    "str",
    "tuple",
];

#[derive(Default)]
struct NotCallable;

impl Checker for NotCallable {
    fn info() -> Info {
        Info::new("not-callable", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "not-callable",
                "A call of a value whose inferred type cannot be called.",
            )
            .severity(Severity::Error)
            .pylint("E1102", "not-callable"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::Call {
            return;
        }
        let Some(callee) = cx.children(node).next() else {
            return;
        };
        let inferred = cx.infer(callee);
        if inferred.resolved && NOT_CALLABLE.contains(&inferred.name.as_str()) {
            let message = format!("{} is not callable", cx.text(callee));
            cx.report(Diagnostic::new(CODE, message, node.range));
        }
    }
}

export_checker!(NotCallable);
