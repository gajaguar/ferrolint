//! A port of Pylint's `undefined-variable` (E0602): a name read that no
//! scope and no builtin defines. A module with a star import the host
//! cannot tell the names of is left alone, since it may define anything.
//! A name bound later in the scope that reads it, or in the module, is left
//! to Pylint's `E0601` (or to nothing).

use std::collections::HashSet;

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

/// Whether the nearest `try` around `node` has a handler that names
/// `NameError`, as Pylint's `node_ignores_exception` reads it.
/// shortcut: a `contextlib.suppress(NameError)` is not read, add it when seen.
fn catches_name_error(node: &Node<'_>, cx: &Context<'_>) -> bool {
    for parent in cx.ancestors(node) {
        match parent.kind {
            NodeKind::ExceptHandler => return false,
            NodeKind::Try => {
                return cx.children(parent).any(|handler| {
                    // the handler's type is its first child when that is an expression
                    handler.kind == NodeKind::ExceptHandler
                        && cx.children(handler).next().is_some_and(|ty| {
                            !Context::is_statement(ty)
                                && cx.nodes()[ty.id as usize..=ty.last as usize]
                                    .iter()
                                    .any(|n| n.name == Some("NameError"))
                        })
                });
            }
            _ => {}
        }
    }
    false
}

#[derive(Default)]
struct UndefinedVariable {
    /// A star import whose names the host cannot tell.
    star_import: bool,
    /// The names the other star imports bind.
    starred: HashSet<String>,
    /// With the scope of the read, and whether it is a `del`.
    undefined: Vec<(String, Range, u32, bool)>,
    /// (scope, name) of every binding in the file, wherever it sits.
    bound: HashSet<(u32, String)>,
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
            NodeKind::Alias if node.name == Some("*") => match cx.star_names(node) {
                Some(names) => self.starred.extend(names),
                None => self.star_import = true,
            },
            NodeKind::FunctionDef | NodeKind::ClassDef | NodeKind::ExceptHandler => {
                if let Some(name) = node.name {
                    self.bound.insert((cx.scope(node).id, name.to_owned()));
                }
            }
            NodeKind::Alias => {
                if let Some(binding) = cx.resolve(node) {
                    self.bound.insert((cx.scope(node).id, binding.name));
                }
            }
            NodeKind::Name if node.context == ExprContext::Store => {
                if let Some(name) = node.name {
                    self.bound.insert((cx.scope(node).id, name.to_owned()));
                }
            }
            NodeKind::Name if matches!(node.context, ExprContext::Load | ExprContext::Del) => {
                let Some(name) = node.name else {
                    return;
                };
                if MODULE_GLOBALS.contains(&name)
                    || cx.resolve(node).is_some()
                    || catches_name_error(node, cx)
                {
                    return;
                }
                self.undefined.push((
                    name.to_owned(),
                    node.range,
                    cx.scope(node).id,
                    node.context == ExprContext::Del,
                ));
            }
            _ => {}
        }
    }

    fn end_file(&mut self, cx: &mut Context<'_>) {
        if self.star_import {
            return;
        }
        for (name, range, scope, deleted) in self.undefined.drain(..) {
            // Bound later in the scope, or in the module: Pylint says `E0601` or
            // nothing, except for a `del`, which no later binding excuses.
            if self.starred.contains(&name)
                || (!deleted
                    && [scope, 0]
                        .into_iter()
                        .any(|scope| self.bound.contains(&(scope, name.clone()))))
            {
                continue;
            }
            cx.report(Diagnostic::new(
                CODE,
                format!("Undefined variable '{name}'"),
                range,
            ));
        }
    }
}

export_checker!(UndefinedVariable);
