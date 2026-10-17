//! A port of Pylint's `pointless-statement` (W0104): an expression
//! statement with no effect, such as a bare name, attribute, literal,
//! comparison or arithmetic. Left alone: anything that calls, awaits or
//! yields (Pylint's W0106 covers some of those), strings (docstrings and
//! W0105), `...`, and the only statement of a `try` body, which is there
//! to raise.

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
        let effects = (node.id..=node.last)
            .filter_map(|id| cx.node(id))
            .any(|inner| {
                matches!(
                    inner.kind,
                    NodeKind::Call
                        | NodeKind::Await
                        | NodeKind::Yield
                        | NodeKind::YieldFrom
                        | NodeKind::Named
                )
            });
        if effects || sole_try_statement(node, cx) {
            return;
        }
        let pointless = matches!(
            value.kind,
            NodeKind::Name
                | NodeKind::Attribute
                | NodeKind::Subscript
                | NodeKind::NumberLiteral
                | NodeKind::BooleanLiteral
                | NodeKind::NoneLiteral
                | NodeKind::Compare
                | NodeKind::BinOp
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

/// Whether the statement is the whole body of a `try`: the statements
/// before the first `except` handler.
fn sole_try_statement(node: &Node<'_>, cx: &Context<'_>) -> bool {
    let Some(parent) = cx
        .parent(node)
        .filter(|parent| parent.kind == NodeKind::Try)
    else {
        return false;
    };
    let body: Vec<u32> = cx
        .children(parent)
        .take_while(|child| child.kind != NodeKind::ExceptHandler)
        .map(|child| child.id)
        .collect();
    body == [node.id]
}

export_checker!(PointlessStatement);
