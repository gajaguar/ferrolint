//! Test plugin that tries, on every file, to read the filesystem, open a
//! socket and read the environment, and reports what happened (SBX001).
//! On a file holding `SANDBOX_EXIT` it calls `exit`, a builtin the host
//! disables.

// The struct is only reachable through the WASM export.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::checker::{
    Ast, Diagnostic, File, Guest, PluginInfo,
};
use ferrolint_plugin_sdk::bindings::ferrolint::plugin::types::{Position, Range, Rule, Severity};

struct Sandbox;

fn outcome<T, E: std::fmt::Display>(attempts: &[Result<T, E>]) -> String {
    let allowed = attempts.iter().filter(|attempt| attempt.is_ok()).count();
    if allowed == 0 {
        "denied".to_owned()
    } else {
        format!("ALLOWED {allowed} of {}", attempts.len())
    }
}

impl Guest for Sandbox {
    fn info() -> PluginInfo {
        PluginInfo {
            name: "sandbox".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            contributions: Vec::new(),
            suppresses: false,
            checks_project: false,
            rules: vec![Rule {
                code: "SBX001".into(),
                name: "sandbox-attempt".into(),
                summary: "What an attempt to leave the sandbox did.".into(),
                severity: Severity::Info,
                pylint_id: None,
                pylint_symbol: None,
            }],
        }
    }

    fn check(file: File, _ast: Ast) -> Vec<Diagnostic> {
        if file.source.contains("SANDBOX_EXIT") {
            std::process::exit(3);
        }
        let filesystem = outcome(&[
            std::fs::read_to_string("/etc/hostname").map(drop),
            std::fs::read_dir("/").map(drop),
            std::fs::read_dir(".").map(drop),
            std::fs::write("escape.txt", "x"),
        ]);
        let network = outcome(&[
            std::net::TcpStream::connect("127.0.0.1:9").map(drop),
            std::net::UdpSocket::bind("0.0.0.0:0").map(drop),
        ]);
        let environment = outcome(&[
            std::env::var("HOME")
                .map(drop)
                .map_err(|err| err.to_string()),
            std::env::var("PATH")
                .map(drop)
                .map_err(|err| err.to_string()),
            if std::env::vars().count() == 0 {
                Err("no variables".to_owned())
            } else {
                Ok(())
            },
        ]);
        let origin = Position {
            offset: 0,
            line: 1,
            column: 1,
        };
        [
            ("filesystem", filesystem),
            ("network", network),
            ("environment", environment),
        ]
        .into_iter()
        .map(|(what, result)| Diagnostic {
            code: "SBX001".into(),
            message: format!("{what}: {result}"),
            range: Range {
                start: origin,
                end: origin,
            },
            severity: Severity::Info,
            fix: None,
        })
        .collect()
    }
}

#[cfg(target_arch = "wasm32")]
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::augment::Guest for Sandbox {
    fn suppress(
        _file: ferrolint_plugin_sdk::File,
        _findings: Vec<ferrolint_plugin_sdk::RawDiagnostic>,
    ) -> Vec<u32> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
impl ferrolint_plugin_sdk::bindings::exports::ferrolint::plugin::project::Guest for Sandbox {
    fn check_project(
        _files: Vec<ferrolint_plugin_sdk::File>,
    ) -> Vec<(String, ferrolint_plugin_sdk::RawDiagnostic)> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
ferrolint_plugin_sdk::bindings::export!(Sandbox);
