//! A port of Pylint's `singleton-comparison` (C0121): `== None`,
//! `!= None`, `== True` and the like, where identity or truthiness is
//! meant. Comes with a fix for the `None` comparisons.

use ferrolint_plugin_sdk::prelude::*;

const CODE: &str = "CMP001";

#[derive(Default)]
struct SingletonComparison;

impl Checker for SingletonComparison {
    fn info() -> Info {
        Info::new("singleton-comparison", env!("CARGO_PKG_VERSION")).rule(
            Rule::new(
                CODE,
                "singleton-comparison",
                "A singleton is compared with == or !=.",
            )
            .severity(Severity::Info)
            .pylint("C0121", "singleton-comparison"),
        )
    }

    fn visit(&mut self, node: &Node<'_>, cx: &mut Context<'_>) {
        let (NodeKind::Compare, Some(operator @ ("==" | "!="))) = (node.kind, node.detail) else {
            return;
        };
        let children: Vec<&Node> = cx.children(node).collect();
        let [left, right] = children.as_slice() else {
            return;
        };
        let singleton =
            |node: &Node| matches!(node.kind, NodeKind::NoneLiteral | NodeKind::BooleanLiteral);
        let (subject, constant) = if singleton(right) {
            (left, right)
        } else if singleton(left) {
            (right, left)
        } else {
            return;
        };
        let subject_text = cx.text(subject);
        let constant_text = cx.text(constant);
        let comparison = format!("{subject_text} {operator} {constant_text}");
        let identity = if operator == "==" { "is" } else { "is not" };
        let message = if constant.kind == NodeKind::NoneLiteral {
            format!("Comparison '{comparison}' should be '{subject_text} {identity} None'")
        } else {
            let truthy = (constant_text == "True") == (operator == "==");
            let test = if truthy {
                subject_text.to_owned()
            } else {
                format!("not {subject_text}")
            };
            format!(
                "Comparison '{comparison}' should be '{subject_text} {identity} {constant_text}' if checking for the singleton value {constant_text}, or '{test}' if testing for {}",
                if truthy { "truthiness" } else { "falsiness" }
            )
        };
        let mut diagnostic = Diagnostic::new(CODE, message, node.range);
        if constant.kind == NodeKind::NoneLiteral && subject.id < constant.id {
            let replacement = format!("{subject_text} {identity} None");
            diagnostic = diagnostic.fix(Fix::replace(
                node.range,
                replacement,
                format!("Use '{identity}'"),
            ));
        }
        cx.report(diagnostic);
    }
}

export_checker!(SingletonComparison);
