//! Report formats.

use std::fmt::Write;

use crate::engine::Report;

/// The default format: one line per diagnostic, then a summary line.
pub fn text(report: &Report) -> String {
    let mut out = String::new();
    for finding in &report.findings {
        let d = &finding.diagnostic;
        let aliases = if finding.aliases.is_empty() {
            String::new()
        } else {
            format!(" ({})", finding.aliases.join(", "))
        };
        let _ = writeln!(
            out,
            "{}:{}:{}: {} {}{} {}",
            finding.path,
            d.range.start.line,
            d.range.start.column,
            d.severity.as_str(),
            d.code,
            aliases,
            d.message
        );
    }
    let _ = writeln!(out, "{}", summary(report));
    out
}

fn summary(report: &Report) -> String {
    format!(
        "{} linted, {}.",
        plural(report.files, "file"),
        plural(report.findings.len(), "finding")
    )
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}
