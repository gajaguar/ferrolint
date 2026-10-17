//! A port of Pylint's `self-assigning-variable` (W0127): `x = x`, which
//! changes nothing.

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
        let children: Vec<&Node> = cx.children(node).collect();
        let [target, value] = children.as_slice() else {
            return;
        };
        let both_names = target.kind == NodeKind::Name && value.kind == NodeKind::Name;
        if let (true, Some(name)) = (both_names, target.name)
            && target.name == value.name
        {
            let message = format!("Assigning the same variable '{name}' to itself");
            cx.report(Diagnostic::new(CODE, message, node.range));
        }
    }
}

export_checker!(SelfAssigning);
