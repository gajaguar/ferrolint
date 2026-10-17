//! Report formats. Every format carries the same findings in the same
//! order; only the packaging differs.

mod junit;
mod sarif;

use std::fmt::Write;

use ferrolint_core::{Finding, Fix, Position, Range};
use serde_json::{Value, json};

use crate::engine::Report;

/// The report formats `--format` selects.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum)]
pub enum Format {
    #[default]
    Text,
    Json,
    Sarif,
    Junit,
}

pub fn render(report: &Report, format: Format) -> String {
    match format {
        Format::Text => text(report),
        Format::Json => json(report),
        Format::Sarif => sarif::render(report),
        Format::Junit => junit::render(report),
    }
}

/// A finding as one line of text: path, line, column, severity, code with
/// its aliases, and message.
fn line(finding: &Finding) -> String {
    let d = &finding.diagnostic;
    let aliases = if finding.aliases.is_empty() {
        String::new()
    } else {
        format!(" ({})", finding.aliases.join(", "))
    };
    format!(
        "{}:{}:{}: {} {}{} {}",
        finding.path,
        d.range.start.line,
        d.range.start.column,
        d.severity.as_str(),
        d.code,
        aliases,
        d.message
    )
}

/// The default format: one line per diagnostic, then a summary line.
pub fn text(report: &Report) -> String {
    let mut out = String::new();
    for finding in &report.findings {
        let _ = writeln!(out, "{}", line(finding));
    }
    let _ = writeln!(out, "{}", summary(report));
    out
}

fn summary(report: &Report) -> String {
    let mode = report
        .diff_base
        .as_ref()
        .map(|reference| format!(" (diff mode against {reference})"))
        .unwrap_or_default();
    format!(
        "{} linted{mode}, {}.",
        plural(report.files.len(), "file"),
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

/// One JSON document: the diagnostics in report order and a summary. Keys
/// are sorted and the output is compact, so it is canonical JSON.
pub fn json(report: &Report) -> String {
    let diagnostics: Vec<Value> = report.findings.iter().map(json_finding).collect();
    let mut document = json!({
        "diagnostics": diagnostics,
        "summary": {
            "files": report.files.len(),
            "findings": report.findings.len(),
            "mode": if report.diff_base.is_some() { "diff" } else { "full" },
        },
    });
    if let (Some(reference), Some(summary)) =
        (&report.diff_base, document["summary"].as_object_mut())
    {
        summary.insert("diff-base".into(), json!(reference));
    }
    let mut out = document.to_string();
    out.push('\n');
    out
}

fn json_finding(finding: &Finding) -> Value {
    let d = &finding.diagnostic;
    let mut object = json!({
        "aliases": finding.aliases,
        "code": d.code,
        "message": d.message,
        "path": finding.path,
        "range": json_range(&d.range),
        "severity": d.severity.as_str(),
    });
    if let (Some(fix), Some(map)) = (&d.fix, object.as_object_mut()) {
        map.insert("fix".into(), json_fix(fix));
    }
    object
}

fn json_fix(fix: &Fix) -> Value {
    json!({
        "description": fix.description,
        "range": json_range(&fix.range),
        "replacement": fix.replacement,
    })
}

fn json_range(range: &Range) -> Value {
    json!({ "end": json_position(range.end), "start": json_position(range.start) })
}

fn json_position(position: Position) -> Value {
    json!({ "column": position.column, "line": position.line, "offset": position.offset })
}
