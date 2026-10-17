//! Test plugin, written against the raw contract bindings: it reports every
//! visitor event it receives and every semantic answer it gets, so the
//! acceptance tests can read them back from the report.
//!
//! | Code   | Reports                                                    |
//! | :----- | :--------------------------------------------------------- |
//! | PRB001 | each visitor event, numbered so the order survives sorting |
//! | PRB002 | what each name load resolves to                            |
//! | PRB003 | the bindings visible at each name load                     |
//! | PRB004 | each function definition, with a fix                       |
//! | PRB005 | each class definition, without a fix                       |

// The struct is only reachable through the WASM export.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::fmt::Write;

use ferrolint_plugin_sdk::Node;
use ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::checker::{
    Ast, Diagnostic, File, Guest, PluginInfo,
};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::semantic::{self, Binding};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::types::{
    ExprContext, Fix, NodeKind, Position, Range, Rule, Severity,
};

struct Probe;

fn rule(code: &str, summary: &str, severity: Severity) -> Rule {
    Rule {
        code: code.into(),
        name: code.to_lowercase(),
        summary: summary.into(),
        severity,
        pylint_id: None,
        pylint_symbol: None,
    }
}

fn diagnostic(code: &str, message: String, range: Range, severity: Severity) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        message,
        range,
        severity,
        fix: None,
    }
}

/// An enum case as the reports spell it: `FunctionDef` is `functiondef`.
fn tag(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    debug.rsplit("::").next().unwrap_or(&debug).to_lowercase()
}

fn kind(kind: NodeKind) -> String {
    tag(kind)
}

fn describe(binding: &Binding) -> String {
    let scope = semantic::scope_of(binding.node.unwrap_or(0));
    let mut out = tag(binding.kind);
    if binding.node.is_some() {
        let line = binding.range.map_or(0, |range| range.start.line);
        let _ = write!(
            out,
            " in {} {} line {line}",
            tag(scope.kind),
            scope.name.as_deref().unwrap_or("-")
        );
    }
    if let Some(origin) = &binding.origin {
        let _ = write!(out, " from {origin}");
    }
    out
}

struct Events {
    diagnostics: Vec<Diagnostic>,
    sequence: u32,
}

impl Events {
    fn push(&mut self, event: &str, range: Range) {
        let message = format!("{:04} {event}", self.sequence);
        self.sequence += 1;
        self.diagnostics
            .push(diagnostic("PRB001", message, range, Severity::Info));
    }
}

impl Guest for Probe {
    fn info() -> PluginInfo {
        PluginInfo {
            name: "probe".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            rules: vec![
                rule("PRB001", "A visitor event.", Severity::Info),
                rule("PRB002", "What a name resolves to.", Severity::Info),
                rule("PRB003", "The bindings visible at a name.", Severity::Info),
                rule("PRB004", "A function definition.", Severity::Warning),
                rule("PRB005", "A class definition.", Severity::Error),
            ],
        }
    }

    fn check(file: File, ast: Ast) -> Vec<Diagnostic> {
        let nodes = Node::decode(&ast);
        let origin = Position {
            offset: 0,
            line: 1,
            column: 1,
        };
        let empty = Range {
            start: origin,
            end: origin,
        };
        let mut events = Events {
            diagnostics: Vec::new(),
            sequence: 0,
        };
        events.push(&format!("start-file {}", file.module), empty);
        let mut open: Vec<&Node<'_>> = Vec::new();
        for node in &nodes {
            while open.last().is_some_and(|top| top.last < node.id) {
                if let Some(done) = open.pop() {
                    events.push(&format!("leave {}", kind(done.kind)), done.range);
                }
            }
            let name = node.name.unwrap_or("");
            events.push(
                format!("visit {} {name}", kind(node.kind)).trim_end(),
                node.range,
            );
            open.push(node);
        }
        while let Some(done) = open.pop() {
            events.push(&format!("leave {}", kind(done.kind)), done.range);
        }
        events.push("end-file", empty);

        let mut diagnostics = events.diagnostics;
        for node in &nodes {
            let name = node.name.unwrap_or_default().to_owned();
            match node.kind {
                NodeKind::Name if node.context == ExprContext::Load => {
                    let target = semantic::resolve(node.id)
                        .map_or_else(|| "unresolved".to_owned(), |binding| describe(&binding));
                    diagnostics.push(diagnostic(
                        "PRB002",
                        format!("{name} -> {target}"),
                        node.range,
                        Severity::Info,
                    ));
                    let visible: Vec<String> = semantic::visible_bindings(node.id)
                        .iter()
                        .map(|binding| {
                            let line = binding.range.map_or(0, |range| range.start.line);
                            format!("{}={}@{line}", binding.name, tag(binding.kind))
                        })
                        .collect();
                    diagnostics.push(diagnostic(
                        "PRB003",
                        format!("at {name}: {}", visible.join(" ")),
                        node.range,
                        Severity::Info,
                    ));
                }
                NodeKind::FunctionDef => diagnostics.push(Diagnostic {
                    fix: Some(Fix {
                        description: format!("Rename {name} to {}", name.to_uppercase()),
                        replacement: name.to_uppercase(),
                        range: node.range,
                    }),
                    ..diagnostic(
                        "PRB004",
                        format!("function {name}"),
                        node.range,
                        Severity::Warning,
                    )
                }),
                NodeKind::ClassDef => diagnostics.push(diagnostic(
                    "PRB005",
                    format!("class {name}"),
                    node.range,
                    Severity::Error,
                )),
                _ => {}
            }
        }
        diagnostics
    }
}

#[cfg(target_arch = "wasm32")]
ferrolint_plugin_sdk::bindings::export!(Probe);
