//! Test plugin that loops forever on a file holding `RUNAWAY` and reports
//! one RUN001 on every other file.

// The struct is only reachable through the WASM export.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::checker::{
    Diagnostic, File, Guest, Node, PluginInfo,
};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::types::{Position, Range, Rule, Severity};

struct Runaway;

impl Guest for Runaway {
    fn info() -> PluginInfo {
        PluginInfo {
            name: "runaway".into(),
            version: env!("CARGO_PKG_VERSION").into(),
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

    fn check(file: File, nodes: Vec<Node>) -> Vec<Diagnostic> {
        if file.source.contains("RUNAWAY") {
            let mut spins: u64 = 0;
            loop {
                spins = std::hint::black_box(spins.wrapping_add(nodes.len() as u64));
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
ferrolint_plugin_sdk::bindings::export!(Runaway);
