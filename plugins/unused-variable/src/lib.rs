//! A port of Pylint's `unused-variable` (W0612): a function assigns a
//! local name it never reads: an assignment, a loop, `with`, `except` or
//! `match` target, a walrus, an undecorated nested `def` or `class`. Names
//! matching Pylint's default `dummy-variables-rgx`, names declared `global`
//! or `nonlocal`, and locals assigned above a `locals()` call (W0641's)
//! are left alone, as are a
//! parameter assigned again and a name that is a comprehension target
//! anywhere in the same function (Pylint's quirk).
//!
//! shortcut: the `dummy-variables-rgx` option is not read, and a name used
//! only in a string annotation or a type comment still counts as unused.
//! Upgrade by reading the option and collecting those names.

use std::collections::{HashMap, HashSet};

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "NAM003";

/// Pylint's default `dummy-variables-rgx`, `re.match`ed: `_+$`,
/// `_[a-zA-Z0-9_]*[a-zA-Z0-9]+?$`, `dummy`, `^ignored_` and `^unused_`.
fn is_dummy(name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    name.chars().all(|c| c == '_')
        || (name.starts_with('_')
            && name.chars().all(word)
            && name.ends_with(|c: char| c.is_ascii_alphanumeric()))
        || ["dummy", "ignored_", "unused_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// A local assignment.
struct Assigned {
    scope: u32,
    name: String,
    range: Range,
    /// Ids of the function's node and of its last descendant.
    span: (u32, u32),
    /// The `except` handler that binds the name, whose body alone reads it.
    handler: Option<u32>,
}

#[derive(Default)]
struct UnusedVariable {
    assigned: Vec<Assigned>,
    /// Ids of the `except` handlers that read their own name.
    handlers_read: HashSet<u32>,
    /// (scope, name) of each parameter.
    params: HashSet<(u32, String)>,
    /// (node id, name) of each comprehension target.
    comprehension_targets: Vec<(u32, String)>,
    /// (scope, name) pairs some read reached.
    read: HashSet<(u32, String)>,
    /// Function -> line of its last `locals()` call; the locals assigned
    /// above that line escape through it.
    escaping: HashMap<u32, u32>,
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
        let Some(name) = node.name else {
            return;
        };
        if node.kind == NodeKind::Parameter {
            if let Some(binding) = cx.resolve(node) {
                self.params.insert((binding.scope, name.to_owned()));
            }
            return;
        }
        // The name of an `except` handler, a `match` pattern or a nested
        // definition is a field of its node, not a `Name` child.
        let bound = matches!(
            node.kind,
            NodeKind::ExceptHandler
                | NodeKind::Pattern
                | NodeKind::FunctionDef
                | NodeKind::ClassDef
        );
        if node.kind != NodeKind::Name && !bound {
            return;
        }
        match if bound {
            ExprContext::Store
        } else {
            node.context
        } {
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
                    BindingKind::Assignment
                        | BindingKind::Annotation
                        | BindingKind::LoopVar
                        | BindingKind::WithItem
                        | BindingKind::ExceptHandler
                        | BindingKind::NamedExpr
                        | BindingKind::MatchCapture
                        | BindingKind::FunctionDef
                        | BindingKind::ClassDef
                ) && binding.node == Some(node.id)
                    && cx.scope(node).id == binding.scope
                    && cx.scope(node).kind == ScopeKind::Function;
                // A decorated definition is left alone, as Pylint does.
                let decorated = cx
                    .children(node)
                    .any(|child| child.kind == NodeKind::Decorator);
                if local && !decorated && !is_dummy(name) {
                    let function = cx.node(cx.scope(node).node);
                    let span = function.map_or((0, 0), |f| (f.id, f.last));
                    self.assigned.push(Assigned {
                        scope: binding.scope,
                        name: name.to_owned(),
                        range: node.range,
                        span,
                        handler: (binding.kind == BindingKind::ExceptHandler).then_some(node.id),
                    });
                } else if binding.kind == BindingKind::ComprehensionVar {
                    self.comprehension_targets.push((node.id, name.to_owned()));
                }
            }
            ExprContext::Load | ExprContext::Del => {
                if let Some(binding) = cx.resolve(node) {
                    if name == "locals" && binding.kind == BindingKind::Builtin {
                        let line = self.escaping.entry(cx.scope(node).id).or_default();
                        *line = (*line).max(node.range.start.line);
                    }
                    self.read.insert((binding.scope, name.to_owned()));
                }
                if let Some(handler) = cx
                    .ancestors(node)
                    .find(|a| a.kind == NodeKind::ExceptHandler && a.name == Some(name))
                {
                    self.handlers_read.insert(handler.id);
                }
            }
            ExprContext::None => {}
        }
    }

    fn end_file(&mut self, cx: &mut Context<'_>) {
        let mut reported = HashSet::new();
        for Assigned {
            scope,
            name,
            range,
            span: (first, last),
            handler,
        } in &self.assigned
        {
            let comprehension_target = self
                .comprehension_targets
                .iter()
                .any(|(id, target)| (first..=last).contains(&id) && target == name);
            // A handler's name is read in its body only, whatever else reads
            // the same name elsewhere in the function.
            let read = match handler {
                Some(id) => self.handlers_read.contains(id),
                None => self.read.contains(&(*scope, name.to_owned())),
            };
            if self
                .escaping
                .get(scope)
                .is_some_and(|line| range.start.line < *line)
                || self.params.contains(&(*scope, name.to_owned()))
                || comprehension_target
                || read
            {
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
