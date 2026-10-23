//! A port of Pylint's `global-statement` (W0603): a `global` statement
//! inside a function or class. One at module level is Pylint's W0604,
//! which this plugin leaves alone. Pylint gives such a statement W0601 or
//! W0602 instead when one of its names is never defined at module level or
//! never rebound after the statement, so this plugin stays silent for it.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "STM001";

/// Whether `node` binds its name as Pylint's `AssignName`, `ClassDef` and
/// `FunctionDef` do. An `except ... as name` clause holds its name on the handler.
/// shortcut: a `case` capture pattern is not a rebinding, so a function that
/// only captures a global name gets W0602 where Pylint says W0603; upgrade when
/// the pattern names are visible here.
fn rebinds(node: &Node<'_>) -> bool {
    match node.kind {
        NodeKind::FunctionDef | NodeKind::ClassDef | NodeKind::ExceptHandler => true,
        NodeKind::Name => node.context == ExprContext::Store,
        _ => false,
    }
}

/// Whether `node` binds its name at module level, where Pylint's `visit_global`
/// stops looking.
fn defines(node: &Node<'_>, cx: &Context<'_>) -> bool {
    match node.kind {
        NodeKind::FunctionDef | NodeKind::ClassDef => cx
            .parent(node)
            .is_some_and(|parent| parent.kind == NodeKind::Module),
        _ => rebinds(node) && cx.scope(node).kind == ScopeKind::Module,
    }
}

/// The module attributes that astroid's `ModuleModel` defines without a binding.
const MODULE_ATTRIBUTES: &[&str] = &[
    "__dict__",
    "__doc__",
    "__file__",
    "__name__",
    "__package__",
    "__path__",
    "__spec__",
];

/// Whether an import binds `name` where Pylint's `module_locals` keeps it: at
/// module level, or in a function that declares `global name`.
fn imports_module_name(name: &str, cx: &Context<'_>) -> bool {
    let nodes = cx.nodes();
    nodes.iter().any(|node| {
        node.kind == NodeKind::Alias
            && node
                .detail
                .or(node.name.and_then(|dotted| dotted.split('.').next()))
                == Some(name)
            && {
                let scope = cx.scope(node);
                scope.kind == ScopeKind::Module
                    || nodes.iter().any(|global| {
                        global.kind == NodeKind::Global
                            && cx.scope(global).id == scope.id
                            && cx.children(global).any(|child| child.name == Some(name))
                    })
            }
    })
}

/// Whether Pylint's `visit_global` reports one of the statement's names as
/// W0601 or W0602, which replaces W0603.
fn takes_other_message(global: &Node<'_>, cx: &Context<'_>) -> bool {
    let nodes = cx.nodes();
    let line = global.range.start.line;
    let scope = cx.scope(global).id;
    cx.children(global)
        .filter(|child| child.kind == NodeKind::Identifier)
        .filter_map(|child| child.name)
        .any(|name| {
            // an import binds the name locally, which Pylint reads as no message
            if imports_module_name(name, cx) {
                return false;
            }
            let later = |node: &&Node<'_>| {
                node.name == Some(name)
                    && node.range.start.line > line
                    && cx.scope(node).id == scope
            };
            let rebound = nodes.iter().filter(later).any(|node| rebinds(node));
            let deleted = nodes
                .iter()
                .filter(later)
                .any(|node| node.kind == NodeKind::Name && node.context == ExprContext::Del);
            (!rebound && !deleted)
                || (!MODULE_ATTRIBUTES.contains(&name)
                    && !nodes
                        .iter()
                        .any(|node| node.name == Some(name) && defines(node, cx)))
        })
}

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
        if node.kind == NodeKind::Global
            && cx.scope(node).kind != ScopeKind::Module
            && !takes_other_message(node, cx)
        {
            cx.report(Diagnostic::new(
                CODE,
                "Using the global statement",
                node.range,
            ));
        }
    }
}

export_checker!(GlobalStatement);
