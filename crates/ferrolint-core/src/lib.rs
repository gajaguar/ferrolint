//! Core types for ferrolint.

use std::path::PathBuf;

/// Severity of a diagnostic.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A source range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// An optional fix for a diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fix {
    pub description: String,
    pub replacement: String,
    pub range: Range,
}

/// A diagnostic emitted by a plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub range: Range,
    pub severity: Severity,
    pub fix: Option<Fix>,
}

/// A loaded plugin.
#[derive(Clone, Debug)]
pub struct Plugin {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
}

/// A rule provided by a plugin.
#[derive(Clone, Debug)]
pub struct Rule {
    pub code: String,
    pub name: String,
}
