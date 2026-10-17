//! The `JUnit` XML report: one testcase per linted file, one failure per
//! finding of that file.

use std::fmt::Write;

use ferrolint_core::Finding;

use crate::engine::Report;

pub fn render(report: &Report) -> String {
    let tests = report.files.len();
    let failures = report.findings.len();
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
    let mut findings = report.findings.iter().peekable();
    for file in &report.files {
        let mut own: Vec<&Finding> = Vec::new();
        while let Some(finding) = findings.next_if(|finding| finding.path == *file) {
            own.push(finding);
        }
        let name = escape(file);
        if own.is_empty() {
            let _ = writeln!(
                out,
                "    <testcase classname=\"ferrolint\" name=\"{name}\" file=\"{name}\"/>"
            );
            continue;
        }
        let _ = writeln!(
            out,
            "    <testcase classname=\"ferrolint\" name=\"{name}\" file=\"{name}\">"
        );
        for finding in own {
            let d = &finding.diagnostic;
            let _ = writeln!(
                out,
                "      <failure type=\"{}\" message=\"{}\">{}</failure>",
                escape(&d.code),
                escape(&format!("{} {}", d.code, d.message)),
                escape(&super::line(finding))
            );
        }
        out.push_str("    </testcase>\n");
    }
    out.push_str("  </testsuite>\n</testsuites>\n");
    out
}

/// Escape text for an XML attribute or element; characters XML 1.0 cannot
/// carry become U+FFFD.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if u32::from(c) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape;

    #[test]
    fn escapes_markup_and_control_characters() {
        assert_eq!(
            escape("a<b & \"c\"\u{1}"),
            "a&lt;b &amp; &quot;c&quot;\u{fffd}"
        );
    }
}
