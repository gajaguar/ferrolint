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
//! | PRB006 | the value of the `probe` option, when the config sets it   |
//! | PRB007 | the class each name load is, or is an instance of          |
//! | PRB008 | the MRO of each class defined at the module level          |
//! | PRB009 | which class of that MRO binds a member called `perm`       |

// The struct is only reachable through the WASM export.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::fmt::Write;

use ferrolint_plugin_sdk::Node;
use ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::checker::{
    Ast, Diagnostic, File, Guest, PluginInfo,
};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::classes::{self, ClassRef};
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

/// What the `classes` queries answer for a name load (PRB007) and for a class
/// defined at the module level (PRB008, PRB009).
fn class_findings(node: &Node<'_>, module: &str) -> Vec<Diagnostic> {
    let name = node.name.unwrap_or_default();
    let info = |code, message| diagnostic(code, message, node.range, Severity::Info);
    match node.kind {
        NodeKind::Name if node.context == ExprContext::Load => {
            let class = match classes::class_of(node.id) {
                Some(ClassRef::Class(class)) => format!("class {class}"),
                Some(ClassRef::Instance(class)) => format!("instance of {class}"),
                None => "unknown".to_owned(),
            };
            vec![info("PRB007", format!("{name} is {class}"))]
        }
        NodeKind::ClassDef if node.parent == Some(0) => {
            let class = format!("{module}.{name}");
            let mro = classes::ancestors(&class)
                .map_or_else(|| "unknown".to_owned(), |mro| mro.join(" "));
            let owner = match classes::member_owner(&class, "perm") {
                None => "unknown".to_owned(),
                Some(None) => "nobody".to_owned(),
                Some(Some(owner)) => owner,
            };
            vec![
                info("PRB008", format!("mro of {name}: {mro}")),
                info("PRB009", format!("owner of perm in {name}: {owner}")),
            ]
        }
        _ => Vec::new(),
    }
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
            contributions: Vec::new(),
            suppresses: false,
            checks_project: false,
            rules: vec![
                rule("PRB001", "A visitor event.", Severity::Info),
                rule("PRB002", "What a name resolves to.", Severity::Info),
                rule("PRB003", "The bindings visible at a name.", Severity::Info),
                rule("PRB004", "A function definition.", Severity::Warning),
                rule("PRB005", "A class definition.", Severity::Error),
                rule("PRB006", "The `probe` option.", Severity::Info),
                rule("PRB007", "The class a name is.", Severity::Info),
                rule("PRB008", "The MRO of a class.", Severity::Info),
                rule("PRB009", "The owner of a member.", Severity::Info),
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
        if let Some((_, value)) = file.options.iter().find(|(key, _)| key == "probe") {
            diagnostics.push(diagnostic(
                "PRB006",
                format!("probe={value}"),
                empty,
                Severity::Info,
            ));
        }
        for node in &nodes {
            diagnostics.extend(class_findings(node, &file.module));
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
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::augment::Guest for Probe {
    fn suppress(
        _file: ferrolint_plugin_sdk::File,
        _ast: ferrolint_plugin_sdk::Ast,
        _findings: Vec<ferrolint_plugin_sdk::RawDiagnostic>,
    ) -> Vec<u32> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::project::Guest for Probe {
    fn check_project(
        _files: Vec<(ferrolint_plugin_sdk::File, ferrolint_plugin_sdk::Ast)>,
    ) -> Vec<(String, ferrolint_plugin_sdk::RawDiagnostic)> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
ferrolint_plugin_sdk::bindings::export!(Probe);
