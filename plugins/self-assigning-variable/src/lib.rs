//! A port of Pylint's `self-assigning-variable` (W0127): `x = x`, which
//! changes nothing. A class body is skipped, as Pylint does: there it exposes an
//! outer name as a class attribute.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "ASG001";

#[derive(Default)]
struct SelfAssigning;

impl Checker for SelfAssigning {
    fn info() -> Info {
        Info::new("self-assigning-variable", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "self-assigning-variable",
                "A name is assigned to itself.",
            )
            .pylint("W0127", "self-assigning-variable"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::Assign {
            return;
        }
        // A class body `X = X` exposes an outer name as a class attribute; Pylint skips it.
        if cx.scope(node).kind == ScopeKind::Class {
            return;
        }
        let children: Vec<&Node> = cx.children(node).collect();
        let [target, value] = children.as_slice() else {
            return;
        };
        let pairs: Vec<(&Node, &Node)> =
            if target.kind == NodeKind::Tuple && value.kind == NodeKind::Tuple {
                let (names, values): (Vec<&Node>, Vec<&Node>) =
                    (cx.children(target).collect(), cx.children(value).collect());
                if names.len() != values.len() {
                    return;
                }
                names.into_iter().zip(values).collect()
            } else {
                vec![(target, value)]
            };
        for (target, value) in pairs {
            let both_names = target.kind == NodeKind::Name && value.kind == NodeKind::Name;
            if let (true, Some(name)) = (both_names, target.name)
                && target.name == value.name
            {
                let message = format!("Assigning the same variable '{name}' to itself");
                cx.report(Diagnostic::new(CODE, message, node.range));
            }
        }
    }
}

export_checker!(SelfAssigning);
