//! The informational messages Pylint raises about its own `# pylint:`
//! pragmas: I0010, I0011, I0013, I0022 and I0023. The pragma is read the way
//! Pylint reads it, so a comment holds at most one, the last.

use std::sync::OnceLock;

use ferrolint_core::{Diagnostic, LineIndex};
use regex::Regex;

use crate::pylint_names::CORE_MESSAGES;
use crate::suppress::Comment;

/// The pragma text of a comment: what follows the `pylint:` of the last `#`
/// that has one, up to the next `;` or `#`.
fn pragma_text(comment: &str) -> Option<&str> {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    for (hash, _) in comment.match_indices('#').rev() {
        let mut from = hash + 1;
        while let Some(found) = comment[from..].find("pylint:") {
            let at = from + found;
            from = at + 1;
            if comment[..at].chars().next_back().is_some_and(word) {
                continue;
            }
            let rest = &comment[at + "pylint:".len()..];
            let span = &rest[..rest.find([';', '#']).unwrap_or(rest.len())];
            // Whitespace alone still counts as text, with no word in it.
            if !span.is_empty() {
                return Some(span.trim_start());
            }
        }
    }
    None
}

/// One `keyword=name, name` option of a pragma.
struct Pragma<'a> {
    action: &'a str,
    names: Vec<&'a str>,
}

const MESSAGE_KEYWORDS: [&str; 5] = [
    "disable-next",
    "disable-msg",
    "enable-msg",
    "disable",
    "enable",
];

/// The options of a pragma before its first fault, and the token of that
/// fault when it makes the pragma badly formed (I0010). An option Pylint does
/// not recognise (E0011) ends the pragma without one.
fn parse(text: &str) -> (Vec<Pragma<'_>>, Option<&str>) {
    static TOKENS: OnceLock<Regex> = OnceLock::new();
    let tokens = TOKENS.get_or_init(|| {
        let pattern = r"(?P<keyword>\b(?:disable-next|disable-all|disable-msg|enable-msg|skip-file|disable|enable)\b)|(?P<name>[0-9A-Za-z\-_]{2,})|(?P<assign>=)|(?P<name_number>[CREIWF]\d*)";
        Regex::new(pattern).unwrap_or_else(|_| unreachable!("a static regex"))
    });
    let mut done = Vec::new();
    let mut action: Option<&str> = None;
    let mut names = Vec::new();
    let mut assignment_required = false;
    let mut previous = "";
    for token in tokens.captures_iter(text) {
        let value = token.get(0).map_or("", |token| token.as_str());
        if token.name("assign").is_some() {
            if !assignment_required {
                let missing = action.is_none() && previous.is_empty();
                return (done, missing.then_some(""));
            }
            assignment_required = false;
        } else if assignment_required {
            return (done, Some(action.unwrap_or("")));
        } else if token.name("keyword").is_some() {
            if let Some(action) = action {
                if names.is_empty() && MESSAGE_KEYWORDS.contains(&action) {
                    return (done, Some(action));
                }
                done.push(Pragma {
                    action,
                    names: std::mem::take(&mut names),
                });
            }
            action = Some(value);
            assignment_required = MESSAGE_KEYWORDS.contains(&value);
        } else {
            names.push(value);
        }
        previous = value;
    }
    let Some(action) = action else {
        return (done, None);
    };
    if names.is_empty() && MESSAGE_KEYWORDS.contains(&action) {
        return (done, Some(action));
    }
    done.push(Pragma { action, names });
    (done, None)
}

/// The messages a pragma name stands for: a letter is a category, a checker
/// name is all its messages, anything else an id or a symbol, current or
/// legacy. A name outside Pylint's own messages stands for none.
fn definitions(name: &str) -> Vec<(&'static str, &'static str)> {
    let current = |row: &&(&str, &str, &str, &str, &str)| row.0 == row.2;
    let rows = CORE_MESSAGES.iter();
    let lower = name.to_ascii_lowercase();
    let mut found: Vec<_> = if name.len() == 1 {
        let category = name.to_ascii_uppercase();
        rows.filter(current)
            .filter(|row| row.0.starts_with(&category))
            .map(|row| (row.2, row.3))
            .collect()
    } else if let Some(checker) = rows.clone().find(|row| row.4 == lower).map(|row| row.4) {
        rows.filter(current)
            .filter(|row| row.4 == checker)
            .map(|row| (row.2, row.3))
            .collect()
    } else {
        let id = is_id(name).then(|| name.to_ascii_uppercase());
        rows.filter(|row| id.as_deref().map_or(row.1 == name, |id| row.0 == id))
            .map(|row| (row.2, row.3))
            .collect()
    };
    found.dedup();
    found
}

/// Whether a pragma name is a message id: a letter, then digits.
fn is_id(name: &str) -> bool {
    name.len() > 1 && name[1..].bytes().all(|b| b.is_ascii_digit())
}

/// The symbol Pylint suggests for the message id `name`, spelled in any case.
fn symbol(name: &str) -> Option<&'static str> {
    let id = name.to_ascii_uppercase();
    CORE_MESSAGES
        .iter()
        .find(|row| row.0 == id)
        .map(|row| row.1)
}

/// The informational findings of the `# pylint:` comments of a file. The
/// first `skip-file`, `disable-all` or `disable=all` ends the file for
/// Pylint: its later pragmas are not read and the by-id findings are lost.
pub fn informational(source: &str, comments: &[Comment], index: &LineIndex) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut by_id = Vec::new();
    let mut ignored = false;
    'file: for comment in comments {
        let Some(text) = pragma_text(comment.text) else {
            continue;
        };
        let (pragmas, invalid) = parse(text);
        // A finding on the next line (`disable-next`) sits where it starts.
        let next = source[comment.end as usize..]
            .find('\n')
            .and_then(|at| u32::try_from(at).ok())
            .map(|at| comment.end + at + 1);
        let mut push = |code: &str, message: String, line_after: bool| {
            let start = next.filter(|_| line_after).unwrap_or(comment.start);
            let end = if line_after { start } else { comment.end };
            let range = index.range(source, start, end);
            let diagnostic = Diagnostic {
                code: code.into(),
                message,
                range,
                severity: crate::native::rule(code, "", "").severity,
                fix: None,
            };
            if code == "I0023" {
                by_id.push(diagnostic);
            } else {
                out.push(diagnostic);
            }
        };
        for pragma in &pragmas {
            let action = pragma.action;
            if matches!(action, "disable-all" | "skip-file") {
                if action == "disable-all" {
                    push("I0022", deprecated("disable-all", "skip-file"), false);
                }
                push("I0013", "Ignoring entire file".into(), false);
                ignored = true;
                break 'file;
            }
            if let Some(current) = action.strip_suffix("-msg") {
                push("I0022", deprecated(action, current), false);
            }
            let disables = action.starts_with("disable");
            for &name in &pragma.names {
                if action == "disable" && name == "all" {
                    push("I0022", deprecated("disable=all", "skip-file"), false);
                    push("I0013", "Ignoring entire file".into(), false);
                    ignored = true;
                    break 'file;
                }
                let after = action == "disable-next";
                for (id, symbol) in definitions(name) {
                    if disables && symbol != "locally-disabled" {
                        push("I0011", format!("Locally disabling {symbol} ({id})"), after);
                    }
                }
                if let (true, Some(symbol)) = (is_id(name), symbol(name)) {
                    let verb = if disables { "disable" } else { "enable" };
                    let message =
                        format!("'{name}' is cryptic: use '# pylint: {verb}={symbol}' instead");
                    push("I0023", message, after);
                }
            }
        }
        if let Some(token) = invalid {
            push(
                "I0010",
                format!("Unable to consider inline option '{token}'"),
                false,
            );
        }
    }
    if !ignored {
        out.extend(by_id);
    }
    out
}

fn deprecated(old: &str, new: &str) -> String {
    format!("Pragma \"{old}\" is deprecated, use \"{new}\" instead")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suppress::Suppressions;
    use ferrolint_core::Registry;

    fn pragma(text: &str) -> Vec<(&str, Vec<&str>)> {
        parse(text)
            .0
            .into_iter()
            .map(|p| (p.action, p.names))
            .collect()
    }

    #[test]
    fn the_last_pragma_of_a_comment_wins() {
        assert_eq!(pragma_text("# pylint: disable=a"), Some("disable=a"));
        assert_eq!(
            pragma_text("# pylint: disable=a # pylint: enable=b"),
            Some("enable=b")
        );
        assert_eq!(
            pragma_text("# pylint: disable=a; enable=b"),
            Some("disable=a")
        );
        assert_eq!(
            pragma_text("# note # pylint: disable=a # more"),
            Some("disable=a ")
        );
        assert_eq!(pragma_text("# xpylint: disable=a"), None);
        assert_eq!(pragma_text("# pylint:;"), None);
        assert_eq!(pragma_text("# pylint:  "), Some(""));
    }

    #[test]
    fn options_and_their_faults() {
        assert_eq!(
            pragma("disable = W0611, unused-import enable=C"),
            [
                ("disable", vec!["W0611", "unused-import"]),
                ("enable", vec!["C"])
            ]
        );
        assert_eq!(pragma("skip-file"), [("skip-file", vec![])]);
        assert_eq!(parse("disable").1, Some("disable"));
        assert_eq!(parse("disable=").1, Some("disable"));
        assert_eq!(parse("disable W0611").1, Some("disable"));
        assert_eq!(parse("=x").1, Some(""));
        assert_eq!(parse("bogus=1").1, None);
        assert_eq!(parse("skip-file=1").1, None);
        assert_eq!(
            pragma("disable=W0611 skip-file=1"),
            [("disable", vec!["W0611"])]
        );
        // An option that fails is lost with the names it was still reading.
        assert_eq!(pragma("disable=W0611 bogus=1"), []);
    }

    #[test]
    fn names_stand_for_messages() {
        assert_eq!(definitions("W0611"), [("W0611", "unused-import")]);
        assert_eq!(definitions("w0611"), [("W0611", "unused-import")]);
        assert_eq!(definitions("W0703"), [("W0718", "broad-exception-caught")]);
        assert_eq!(definitions("missing-docstring").len(), 3);
        assert!(definitions("W").len() > 50);
        assert!(definitions("imports").iter().any(|m| m.0 == "W0406"));
        assert_eq!(definitions("nonexistent"), []);
        assert_eq!(symbol("w0611"), Some("unused-import"));
    }

    /// `line:ID:message`, in line order, as the recorded Pylint output, once
    /// the pragmas of the file have hidden what they hide.
    fn render(source: &str) -> String {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let comments = crate::suppress::comments(source, parsed.tokens());
        let index = LineIndex::new(source);
        let registry = Registry::new(crate::checks::host_rules()).unwrap();
        let spans = crate::suppress::spelled(source, &parsed, &index);
        let hidden = Suppressions::resolve(&spans, &registry);
        let mut found: Vec<_> = informational(source, &comments, &index)
            .into_iter()
            .filter(|d| !hidden.suppresses(d))
            .map(|d| (d.range.start.line, d.code, d.message))
            .collect();
        found.sort();
        found.dedup();
        found
            .iter()
            .map(|(line, code, message)| format!("{line}:{code}:{message}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn pragmas_fixture() {
        let source = include_str!("native/fixtures/pragmas.py");
        let expected = include_str!("native/fixtures/pragmas.expected").trim_end();
        assert_eq!(render(source), expected);
    }

    #[test]
    fn a_pragma_hides_the_messages_below_it() {
        let source = include_str!("native/fixtures/pragmas_disabled.py");
        let expected = include_str!("native/fixtures/pragmas_disabled.expected").trim_end();
        assert_eq!(render(source), expected);
    }

    #[test]
    fn a_skipped_file_loses_the_by_id_findings() {
        let source = include_str!("native/fixtures/pragmas_skip.py");
        let expected = include_str!("native/fixtures/pragmas_skip.expected").trim_end();
        assert_eq!(render(source), expected);
        let source = "# pylint: disable=W0611\n# pylint: skip-file\n# pylint: disable=W0612\n";
        assert_eq!(
            render(source),
            "1:I0011:Locally disabling unused-import (W0611)\n2:I0013:Ignoring entire file"
        );
    }

    #[test]
    fn disable_next_reports_on_the_next_line() {
        assert_eq!(
            render("# pylint: disable-next=W0611\nimport os\n"),
            "2:I0011:Locally disabling unused-import (W0611)\n\
             2:I0023:'W0611' is cryptic: use '# pylint: disable=unused-import' instead"
        );
    }
}
