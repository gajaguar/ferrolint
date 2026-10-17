//! The `JUnit` XML report: one testcase per linted file, one failure per
//! finding of that file.

use std::collections::BTreeMap;
use std::fmt::Write;

use ferrolint_core::Finding;

use crate::engine::Report;

pub fn render(report: &Report) -> String {
    let mut by_file: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for finding in &report.findings {
        by_file.entry(&finding.path).or_default().push(finding);
    }
    let tests = report.files.len();
    // A testcase fails once however many findings its file has.
    let failures = report
        .files
        .iter()
        .filter(|file| by_file.contains_key(file.as_str()))
        .count();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"ferrolint\" tests=\"{tests}\" failures=\"{failures}\" errors=\"0\">"
    );
    let _ = writeln!(
        out,
        "  <testsuite name=\"ferrolint\" tests=\"{tests}\" failures=\"{failures}\" errors=\"0\" skipped=\"0\">"
    );
    for file in &report.files {
        let name = attribute(file);
        let Some(own) = by_file.get(file.as_str()) else {
            let _ = writeln!(
                out,
                "    <testcase classname=\"ferrolint\" name=\"{name}\" file=\"{name}\"/>"
            );
            continue;
        };
        let _ = writeln!(
            out,
            "    <testcase classname=\"ferrolint\" name=\"{name}\" file=\"{name}\">"
        );
        for finding in own {
            let d = &finding.diagnostic;
            let _ = writeln!(
                out,
                "      <failure type=\"{}\" message=\"{}\">{}</failure>",
                attribute(&d.code),
                attribute(&format!("{} {}", d.code, d.message)),
                text(&super::line(finding))
            );
        }
        out.push_str("    </testcase>\n");
    }
    out.push_str("  </testsuite>\n</testsuites>\n");
    out
}

/// Escape element text; characters XML 1.0 cannot carry become U+FFFD.
fn text(value: &str) -> String {
    escape(value, false)
}

/// Escape an attribute value, keeping tabs and line breaks as character
/// references, which attribute normalization would turn into spaces.
fn attribute(value: &str) -> String {
    escape(value, true)
}

fn escape(value: &str, in_attribute: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\r' => out.push_str("&#13;"),
            '\t' | '\n' if in_attribute => {
                let _ = write!(out, "&#{};", u32::from(c));
            }
            '\t' | '\n' => out.push(c),
            c if u32::from(c) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{attribute, text};

    #[test]
    fn escapes_markup_and_control_characters() {
        assert_eq!(
            text("a<b & \"c\"\u{1}\n"),
            "a&lt;b &amp; &quot;c&quot;\u{fffd}\n"
        );
        assert_eq!(attribute("one\ntwo\tthree\r"), "one&#10;two&#9;three&#13;");
    }
}
