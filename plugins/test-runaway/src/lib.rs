//! Test plugin that loops forever on a file holding `RUNAWAY` and reports
//! one RUN001 on every other file.

// The struct is only reachable through the WASM export.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::checker::{
    Ast, Diagnostic, File, Guest, PluginInfo,
};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::types::{Position, Range, Rule, Severity};

struct Runaway;

impl Guest for Runaway {
    fn info() -> PluginInfo {
        PluginInfo {
            name: "runaway".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            contributions: Vec::new(),
            suppresses: false,
            checks_project: false,
            rules: vec![Rule {
                code: "RUN001".into(),
                name: "checked".into(),
                summary: "The plugin finished the file.".into(),
                severity: Severity::Info,
                pylint_id: None,
                pylint_symbol: None,
            }],
        }
    }

    fn check(file: File, ast: Ast) -> Vec<Diagnostic> {
        if file.source.contains("RUNAWAY") {
            let mut spins: u64 = 0;
            loop {
                spins = std::hint::black_box(spins.wrapping_add(ast.nodes.len() as u64));
            }
        }
        let origin = Position {
            offset: 0,
            line: 1,
            column: 1,
        };
        vec![Diagnostic {
            code: "RUN001".into(),
            message: "checked".into(),
            range: Range {
                start: origin,
                end: origin,
            },
            severity: Severity::Info,
            fix: None,
        }]
    }
}

#[cfg(target_arch = "wasm32")]
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::augment::Guest for Runaway {
    fn suppress(
        _file: ferrolint_plugin_sdk::File,
        _findings: Vec<ferrolint_plugin_sdk::RawDiagnostic>,
    ) -> Vec<u32> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::project::Guest for Runaway {
    fn check_project(
        _files: Vec<ferrolint_plugin_sdk::File>,
    ) -> Vec<(String, ferrolint_plugin_sdk::RawDiagnostic)> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
ferrolint_plugin_sdk::bindings::export!(Runaway);
