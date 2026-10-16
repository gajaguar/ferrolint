//! Core types for ferrolint: diagnostics, source positions and the rule
//! registry that resolves rule codes and their Pylint aliases.

mod lines;
mod rules;

pub use lines::LineIndex;
pub use rules::{Registry, RegistryError, RuleInfo};

use serde::{Deserialize, Serialize};

/// Severity of a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// A point in a source file: a byte offset plus its 1-based line and
/// 1-based column, the column counted in characters.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Position {
    pub offset: u32,
    pub line: u32,
    pub column: u32,
}

/// A source range, start inclusive and end exclusive.
#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// A suggested edit: replace `range` with `replacement`. The host reports
/// it and never applies it on the plugin's behalf.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Fix {
    pub description: String,
    pub replacement: String,
    pub range: Range,
}

/// A diagnostic as a check produced it.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub range: Range,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

/// A diagnostic placed in a file, ready to report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    /// The path as reported: relative to the working directory, `/`-separated.
    pub path: String,
    pub diagnostic: Diagnostic,
    /// The rule's Pylint msg-id and symbol, when it has them.
    pub aliases: Vec<String>,
}

impl Finding {
    /// The total order every report uses: path, start, end, code, message.
    pub fn sort_key(&self) -> (&str, u32, u32, &str, &str) {
        let d = &self.diagnostic;
        (
            &self.path,
            d.range.start.offset,
            d.range.end.offset,
            &d.code,
            &d.message,
        )
    }
}

/// Sort findings into the one order every report uses.
pub fn sort_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
}
