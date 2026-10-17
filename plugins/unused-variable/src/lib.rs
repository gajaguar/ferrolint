//! A port of Pylint's `unused-variable` (W0612): a function assigns a
//! local name it never reads. Names starting with `_`, names declared
//! `global` or `nonlocal`, loop targets, and every local of a function
//! that calls `locals()` are left alone.

use std::collections::HashSet;

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "NAM003";

#[derive(Default)]
struct UnusedVariable {
    /// (scope, name, range) of each local assignment.
    assigned: Vec<(u32, String, Range)>,
    /// (scope, name) pairs some read reached.
    read: HashSet<(u32, String)>,
    /// Functions whose locals escape through `locals()`.
    escaping: HashSet<u32>,
}

impl Checker for UnusedVariable {
    fn info() -> Info {
        Info::new("unused-variable", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "unused-variable",
                "A local variable is assigned but never used.",
            )
            .pylint("W0612", "unused-variable"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::Name {
            return;
        }
        let Some(name) = node.name else {
            return;
        };
        match node.context {
            ExprContext::Store => {
                let Some(binding) = cx.resolve(node) else {
                    return;
                };
                // A store that reaches another scope through `nonlocal` or
                // `global`, and an augmented assignment, both read the name.
                let augmented = cx
                    .parent(node)
                    .is_some_and(|parent| parent.kind == NodeKind::AugAssign);
                if augmented || cx.scope(node).id != binding.scope {
                    self.read.insert((binding.scope, name.to_owned()));
                }
                let local = matches!(
                    binding.kind,
                    BindingKind::Assignment | BindingKind::Annotation
                ) && binding.node == Some(node.id)
                    && cx.scope(node).id == binding.scope
                    && cx.scope(node).kind == ScopeKind::Function;
                if local && !name.starts_with('_') {
                    self.assigned
                        .push((binding.scope, name.to_owned(), node.range));
                }
            }
            ExprContext::Load | ExprContext::Del => {
                if let Some(binding) = cx.resolve(node) {
                    if name == "locals" && binding.kind == BindingKind::Builtin {
                        self.escaping.insert(cx.scope(node).id);
                    }
                    self.read.insert((binding.scope, name.to_owned()));
                }
            }
            ExprContext::None => {}
        }
    }

    fn end_file(&mut self, cx: &mut Context<'_>) {
        let mut reported = HashSet::new();
        for (scope, name, range) in &self.assigned {
            if self.escaping.contains(scope) || self.read.contains(&(*scope, name.to_owned())) {
                continue;
            }
            if reported.insert((*scope, name.to_owned())) {
                cx.report(Diagnostic::new(
                    CODE,
                    format!("Unused variable '{name}'"),
                    *range,
                ));
            }
        }
    }
}

export_checker!(UnusedVariable);
