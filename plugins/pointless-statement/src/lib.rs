//! A port of Pylint's `pointless-statement` (W0104): an expression
//! statement with no effect, such as a bare name, attribute, literal or
//! comparison. Calls, awaits, yields, strings (docstrings and Pylint's
//! W0105) and `...` are left alone.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "STM002";

#[derive(Default)]
struct PointlessStatement;

impl Checker for PointlessStatement {
    fn info() -> Info {
        Info::new("pointless-statement", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(CODE, "pointless-statement", "A statement has no effect.")
                .pylint("W0104", "pointless-statement"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        if node.kind != NodeKind::Expr {
            return;
        }
        let Some(value) = cx.children(node).next() else {
            return;
        };
        let pointless = matches!(
            value.kind,
            NodeKind::Name
                | NodeKind::Attribute
                | NodeKind::Subscript
                | NodeKind::NumberLiteral
                | NodeKind::BooleanLiteral
                | NodeKind::NoneLiteral
                | NodeKind::Compare
                | NodeKind::UnaryOp
                | NodeKind::List
                | NodeKind::Tuple
                | NodeKind::Dict
                | NodeKind::Set
                | NodeKind::Lambda
        );
        if pointless {
            cx.report(Diagnostic::new(
                CODE,
                "Statement seems to have no effect",
                node.range,
            ));
        }
    }
}

export_checker!(PointlessStatement);
