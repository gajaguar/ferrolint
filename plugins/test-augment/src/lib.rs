//! Test plugin for the 1.1.0 exports: it drops the host findings that
//! mention `Foo`, adds `pk` to `good-names`, and reports once per run how many files
//! the project phase gave it.
//!
//! | Code   | Reports                                      |
//! | :----- | :------------------------------------------- |
//! | AUG001 | the number of files, on the first file       |

use ferrolint_plugin_sdk::prelude::*;
use ferrolint_plugin_sdk::{File, Position, Range, RawDiagnostic};

#[derive(Default)]
struct Augment;

impl Checker for Augment {
    fn info() -> Info {
        Info::new("augment", env!("CARGO_PKG_VERSION"))
            .rule(Rule::new(
                "AUG001",
                "project-size",
                "How many files the project phase saw.",
            ))
            .contribute("good-names", &["pk"])
            .suppressing()
            .project_wide()
    }

    fn suppress(_file: &File, findings: &[RawDiagnostic]) -> Vec<u32> {
        findings
            .iter()
            .enumerate()
            .filter(|(_, finding)| finding.message.contains("\"Foo\""))
            .filter_map(|(at, _)| u32::try_from(at).ok())
            .collect()
    }

    fn check_project(files: &[File]) -> Vec<(String, Diagnostic)> {
        let start = Position {
            offset: 0,
            line: 1,
            column: 1,
        };
        files
            .first()
            .map(|file| {
                let message = format!("{} files", files.len());
                let range = Range { start, end: start };
                (file.path.clone(), Diagnostic::new("AUG001", message, range))
            })
            .into_iter()
            .collect()
    }
}

ferrolint_plugin_sdk::export_checker!(Augment);
