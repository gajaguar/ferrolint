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
        // Pylint looks at the left operand first
        let (constant, other) = if singleton(left) {
            (left, right)
        } else if singleton(right) {
            (right, left)
        } else {
            return;
        };
        let (left_text, right_text) = (printed(cx, left), printed(cx, right));
        let other_text = printed(cx, other);
        let constant_text = cx.text(constant);
        let comparison = format!("{left_text} {operator} {right_text}");
        let identity = if operator == "==" { "is" } else { "is not" };
        let example = format!("'{left_text} {identity} {right_text}'");
        let message = if constant.kind == NodeKind::NoneLiteral {
            format!("Comparison '{comparison}' should be {example}")
        } else {
            let truthy = (constant_text == "True") == (operator == "==");
            let test = match (truthy, is_test_condition(cx, node)) {
                (true, true) => other_text,
                (true, false) => format!("bool({other_text})"),
                (false, _) => format!("not {other_text}"),
            };
            format!(
                "Comparison '{comparison}' should be {example} if checking for the singleton value {constant_text}, or '{test}' if testing for {}",
                if truthy { "truthiness" } else { "falsiness" }
            )
        };
        let mut diagnostic = Diagnostic::new(CODE, message, node.range);
        if constant.kind == NodeKind::NoneLiteral && other.id < constant.id {
            let replacement = format!("{} {identity} None", cx.text(other));
            diagnostic = diagnostic.fix(Fix::replace(
                node.range,
                replacement,
                format!("Use '{identity}'"),
            ));
        }
        cx.report(diagnostic);
    }
}

/// The node's text with each simple double-quoted string in the single
/// quotes astroid prints.
/// shortcut: any other spelling (a prefix, an escape, a line break, odd
/// spacing) stays as written; upgrade with a host-side `as_string`.
fn printed(cx: &Context<'_>, node: &Node<'_>) -> String {
    let text = cx.text(node);
    let base = node.range.start.offset as usize;
    let (mut out, mut at, mut id) = (String::new(), 0, node.id);
    while id <= node.last {
        let inner = &cx.nodes()[id as usize];
        id += 1;
        match inner.kind {
            // leave the strings inside an f-string as written
            NodeKind::FString | NodeKind::TString => id = inner.last + 1,
            NodeKind::StringLiteral => {
                let literal = cx.text(inner);
                let quoted = (literal.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
                    .filter(|s| !s.contains(['\\', '\'', '"']));
                if let Some(content) = quoted {
                    let start = inner.range.start.offset as usize - base;
                    out.push_str(&text[at..start]);
                    out.push('\'');
                    out.push_str(content);
                    out.push('\'');
                    at = start + literal.len();
                }
            }
            _ => {}
        }
    }
    out + &text[at..]
}

/// Pylint's `is_test_condition`: the comparison is the direct test of an
/// `if`, `while`, `assert` or conditional expression, a comprehension's
/// filter, or an argument of `bool(...)`.
fn is_test_condition(cx: &Context<'_>, node: &Node<'_>) -> bool {
    use NodeKind as K;
    let Some(parent) = cx.parent(node) else {
        return false;
    };
    let index = cx.children(parent).position(|c| c.id == node.id);
    match parent.kind {
        K::If | K::While | K::Assert | K::ElifElseClause => index == Some(0),
        // the node list follows the source: body, test, orelse
        K::IfExp => index == Some(1),
        // the node list holds a comprehension's iterable and target first
        K::Comprehension => index >= Some(2),
        // a call's positional arguments sit under an `arguments` node
        K::Arguments => cx.parent(parent).is_some_and(|call| {
            let func = cx.children(call).next();
            call.kind == K::Call
                && func.is_some_and(|f| f.kind == K::Name && f.name == Some("bool"))
        }),
        _ => false,
    }
}

export_checker!(SingletonComparison);
