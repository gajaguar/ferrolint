//! The SARIF 2.1.0 report: one run, the rules of the registry, one result
//! per finding.

use std::fmt::Write;

use ferrolint_core::{Finding, Range, RuleInfo, Severity};
use serde_json::{Value, json};

use crate::engine::Report;

const SCHEMA: &str =
    "https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json";

pub fn render(report: &Report) -> String {
    let rules: Vec<&RuleInfo> = report.registry.rules().collect();
    let results: Vec<Value> = report
        .findings
        .iter()
        .map(|finding| result(finding, &rules))
        .collect();
    let document = json!({
        "$schema": SCHEMA,
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "ferrolint",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/gajaguar/ferrolint",
                    "rules": rules.iter().map(|rule| descriptor(rule)).collect::<Vec<_>>(),
                }
            },
            "columnKind": "unicodeCodePoints",
            "results": results,
        }],
    });
    let mut out = document.to_string();
    out.push('\n');
    out
}

fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
    }
}

fn descriptor(rule: &RuleInfo) -> Value {
    let mut properties = json!({ "aliases": rule.aliases() });
    if let (Some(plugin), Some(map)) = (&rule.plugin, properties.as_object_mut()) {
        map.insert("plugin".into(), json!(plugin));
    }
    json!({
        "id": rule.code,
        "name": rule.name,
        "shortDescription": { "text": rule.summary },
        "defaultConfiguration": { "level": level(rule.severity) },
        "properties": properties,
    })
}

fn result(finding: &Finding, rules: &[&RuleInfo]) -> Value {
    let d = &finding.diagnostic;
    let uri = uri(&finding.path);
    let mut result = json!({
        "ruleId": d.code,
        "level": level(d.severity),
        "message": { "text": d.message },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": { "uri": uri },
                "region": region(&d.range),
            }
        }],
        "properties": { "aliases": finding.aliases },
    });
    let Some(map) = result.as_object_mut() else {
        return result;
    };
    if let Some(index) = rules.iter().position(|rule| rule.code == d.code) {
        map.insert("ruleIndex".into(), json!(index));
    }
    if let Some(fix) = &d.fix {
        map.insert(
            "fixes".into(),
            json!([{
                "description": { "text": fix.description },
                "artifactChanges": [{
                    "artifactLocation": { "uri": uri },
                    "replacements": [{
                        "deletedRegion": region(&fix.range),
                        "insertedContent": { "text": fix.replacement },
                    }],
                }],
            }]),
        );
    }
    result
}

fn region(range: &Range) -> Value {
    json!({
        "startLine": range.start.line,
        "startColumn": range.start.column,
        "endLine": range.end.line,
        "endColumn": range.end.column,
    })
}

/// A relative path as a URI reference: `/`-separated, with every byte
/// outside the unreserved set percent-encoded.
fn uri(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::uri;

    #[test]
    fn paths_become_uri_references() {
        assert_eq!(uri("pkg/mod.py"), "pkg/mod.py");
        assert_eq!(uri("my dir/é.py"), "my%20dir/%C3%A9.py");
    }
}
