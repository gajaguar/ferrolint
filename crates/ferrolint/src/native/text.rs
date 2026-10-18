//! The file's own text: `C2503` bad-file-encoding, `E2511`
//! invalid-character-carriage-return, `C0327` mixed-line-endings and
//! `C0328` unexpected-line-ending-format. `E2501` invalid-unicode-codec is
//! not ported: Pylint cannot reach it, as Python rejects a UTF-16 or UTF-32
//! file (`F0010`) before the check runs.

use std::collections::BTreeSet;

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::token::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use super::{Checker, Ctx, rule};

const ENCODING: &str = "C2503";
const CARRIAGE: &str = "E2511";
const MIXED: &str = "C0327";
const EXPECTED: &str = "C0328";

pub struct Text;

impl Checker for Text {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                ENCODING,
                "bad-file-encoding",
                "A coding comment that names an encoding other than UTF-8.",
            ),
            rule(
                CARRIAGE,
                "invalid-character-carriage-return",
                "A carriage return that does not end a CRLF line.",
            ),
            rule(
                MIXED,
                "mixed-line-endings",
                "A logical line whose ending differs from the previous one.",
            ),
            rule(
                EXPECTED,
                "unexpected-line-ending-format",
                "A logical line whose ending is not the `expected-line-ending-format`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if enabled.contains(ENCODING) {
            encoding(ctx, out);
        }
        if enabled.contains(CARRIAGE) {
            carriage_returns(ctx, out);
        }
        line_endings(
            ctx,
            enabled.contains(MIXED),
            enabled.contains(EXPECTED),
            out,
        );
    }
}

/// The encoding a PEP 263 comment names (`# coding: latin-1`).
fn cookie(line: &str) -> Option<&str> {
    let comment = line
        .trim_start_matches([' ', '\t', '\x0c'])
        .strip_prefix('#')?;
    let mut from = 0;
    while let Some(found) = comment[from..].find("coding") {
        let after = &comment[from + found + "coding".len()..];
        if let Some(rest) = after.strip_prefix([':', '=']) {
            let rest = rest.trim_start_matches([' ', '\t']);
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
                .unwrap_or(rest.len());
            if end > 0 {
                return Some(&rest[..end]);
            }
        }
        from += found + 1;
    }
    None
}

/// Whether Pylint's normalized codec name is `utf-8`.
fn is_utf8(name: &str) -> bool {
    let name = name.to_ascii_lowercase().replace('_', "-");
    let rest = name.strip_prefix("utf").unwrap_or("?");
    let rest = rest.strip_prefix(['-', ' ']).unwrap_or(rest);
    let Some(rest) = rest.strip_prefix('8') else {
        return false;
    };
    let rest = rest.strip_prefix(['-', ' ']).unwrap_or(rest);
    rest.is_empty() || rest == "sig"
}

/// A byte offset; a source over 4 GiB is not supported.
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn line_range(ctx: &Ctx<'_>, line: u32) -> TextRange {
    let start = ctx.index.line_start(line).unwrap_or(0);
    let end = ctx
        .index
        .line_start(line + 1)
        .unwrap_or_else(|| to_u32(ctx.source.len()));
    TextRange::new(TextSize::from(start), TextSize::from(end))
}

fn encoding(ctx: &Ctx<'_>, out: &mut Vec<Diagnostic>) {
    let source = ctx.source.strip_prefix('\u{feff}').unwrap_or(ctx.source);
    let mut lines = source.lines();
    let first = lines.next().unwrap_or("");
    // Python reads a second line only when the first is blank or a comment.
    let blank = first.trim_start_matches([' ', '\t', '\x0c']);
    let second = (blank.is_empty() || blank.starts_with('#'))
        .then(|| lines.next())
        .flatten();
    for (line, text) in [(1, Some(first)), (2, second)] {
        if let Some(name) = text.and_then(cookie) {
            if !is_utf8(name) {
                out.push(ctx.report_at(
                    ENCODING,
                    line_range(ctx, line),
                    "PEP8 recommends UTF-8 as encoding for Python files",
                ));
            }
            return;
        }
    }
}

/// Pylint splits the file at `\n` only, so a lone `\r` stays inside its line;
/// the diagnostic keeps that line number.
fn carriage_returns(ctx: &Ctx<'_>, out: &mut Vec<Diagnostic>) {
    let mut offset = 0;
    for (number, line) in (1..).zip(ctx.source.split_inclusive('\n')) {
        let text = if number == 1 {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        let skipped = line.len() - text.len();
        // The end of a CRLF line is fine, and a `\r` first on its line stops the search.
        let crlf = if text.ends_with("\r\n") {
            text.len() - 2
        } else {
            usize::MAX
        };
        if !text.starts_with('\r') {
            for (at, _) in text.match_indices('\r').filter(|&(at, _)| at != crlf) {
                let start = offset + skipped + at;
                let range = TextRange::at(TextSize::from(to_u32(start)), TextSize::from(1));
                let mut found = ctx.report_at(
                    CARRIAGE,
                    range,
                    "Invalid unescaped character carriage-return, use \"\\r\" instead.",
                );
                let column = to_u32(text[..at].chars().count()) + 1;
                found.range.start.line = number;
                found.range.end.line = number;
                found.range.start.column = column;
                found.range.end.column = column + 1;
                out.push(found);
            }
        }
        offset += line.len();
    }
}

fn line_endings(ctx: &Ctx<'_>, mixed: bool, unexpected: bool, out: &mut Vec<Diagnostic>) {
    let expected = ctx
        .option("expected-line-ending-format")
        .filter(|_| unexpected);
    if !mixed && expected.is_none_or(str::is_empty) {
        return;
    }
    let mut last: Option<&str> = None;
    for token in ctx.parsed.tokens() {
        if token.kind() != TokenKind::Newline {
            continue;
        }
        let range = token.range();
        let ending = &ctx.source[range];
        // "" is the newline Python adds to a file that does not end in one
        if mixed && last.is_some_and(|before| !ending.is_empty() && before != ending) {
            out.push(ctx.report_at(MIXED, range, "Mixed line endings LF and CRLF"));
        }
        last = Some(ending);
        if let Some(expected) = expected.filter(|expected| !expected.is_empty()) {
            let found = if ending == "\n" { "LF" } else { "CRLF" };
            if found != expected {
                out.push(ctx.report_at(
                    EXPECTED,
                    range,
                    format!("Unexpected line ending format. There is '{found}' while it should be '{expected}'."),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::native::{lint, lint_with};

    fn rows(found: Vec<(u32, String)>, ids: &[&str]) -> Vec<(u32, String)> {
        found
            .into_iter()
            .filter(|(_, code)| ids.contains(&code.as_str()))
            .collect()
    }

    fn expect(list: &[(u32, &str)]) -> Vec<(u32, String)> {
        list.iter()
            .map(|(line, code)| (*line, (*code).to_owned()))
            .collect()
    }

    // Each expected row below is Pylint's own output on the same bytes.
    #[test]
    fn coding_comments() {
        let cases = [
            ("# -*- coding: latin-1 -*-\nx = 1\n", vec![(1, "C2503")]),
            (
                "#!/usr/bin/python\n# coding=ascii\nx = 1\n",
                vec![(2, "C2503")],
            ),
            ("x = 1\n# coding: latin-1\n", vec![]),
            ("# vim: set fileencoding=utf8 :\nx = 1\n", vec![]),
            ("# coding: UTF_8\nx = 1\n", vec![]),
            ("# coding: iso-8859-15\nx = 1\n", vec![(1, "C2503")]),
            ("#coding:cp1252\nx = 1\n", vec![(1, "C2503")]),
        ];
        for (source, want) in cases {
            assert_eq!(rows(lint(source), &["C2503"]), expect(&want), "{source:?}");
        }
    }

    #[test]
    fn carriage_return_inside_a_line() {
        let cases = [
            ("x = 1  # a\rb\n", vec![(1, "E2511")]),
            ("x = \"\"\"a\rb\"\"\"\n", vec![(1, "E2511")]),
            ("# \rab\r\n", vec![(1, "E2511")]),
            (
                "x = \"\"\"a\r\rb\"\"\"\r\n",
                vec![(1, "E2511"), (1, "E2511")],
            ),
            ("x = 1\r\ny = 2\r\n", vec![]),
        ];
        for (source, want) in cases {
            assert_eq!(rows(lint(source), &["E2511"]), expect(&want), "{source:?}");
        }
    }

    #[test]
    fn line_endings() {
        let ids = ["C0327", "C0328"];
        let le1 = "x = 1\ny = 2\r\nz = 3\n";
        assert_eq!(
            rows(
                lint_with(le1, &[("expected-line-ending-format", "LF")]),
                &ids
            ),
            expect(&[(2, "C0327"), (2, "C0328"), (3, "C0327")])
        );
        let crlf = [("expected-line-ending-format", "CRLF")];
        assert_eq!(
            rows(lint_with(le1, &crlf), &ids),
            expect(&[(1, "C0328"), (2, "C0327"), (3, "C0327"), (3, "C0328")])
        );
        // a last line without its newline counts as CRLF, and never as mixed
        let le6 = "x = 1\ny = 2";
        assert_eq!(
            rows(
                lint_with(le6, &[("expected-line-ending-format", "LF")]),
                &ids
            ),
            expect(&[(2, "C0328")])
        );
        assert_eq!(rows(lint_with(le6, &crlf), &ids), expect(&[(1, "C0328")]));
        // a trailing comment and the inside of brackets are not logical lines
        let le3 = "x = 1\ny = (1,\r\n  2)\n# c\r\nz = 3\n\n";
        assert_eq!(
            rows(
                lint_with(le3, &[("expected-line-ending-format", "LF")]),
                &ids
            ),
            expect(&[])
        );
        let le4 = "x = 1\ny = 2\n# comment";
        assert_eq!(
            rows(lint_with(le4, &crlf), &ids),
            expect(&[(1, "C0328"), (2, "C0328")])
        );
        assert_eq!(rows(lint(le1), &ids), expect(&[(2, "C0327"), (3, "C0327")]));
    }
}
