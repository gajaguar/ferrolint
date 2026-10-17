//! Comment directives: the line-level suppressions a comment carries
//! (`# ferrolint: disable=`, `# pylint: disable=` and `# noqa:`).

use std::collections::{BTreeMap, HashMap, HashSet};

use ferrolint_core::{Diagnostic, LineIndex, Registry};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_text_size::Ranged;

use crate::checks;

/// A comment in a source file.
pub struct Comment<'a> {
    pub start: u32,
    pub end: u32,
    pub text: &'a str,
}

/// Every comment token of a parsed file, in source order.
pub fn comments<'a>(source: &'a str, tokens: &Tokens) -> Vec<Comment<'a>> {
    tokens
        .iter()
        .filter(|token| token.kind() == TokenKind::Comment)
        .map(|token| {
            let range = token.range();
            Comment {
                start: range.start().to_u32(),
                end: range.end().to_u32(),
                text: &source[range.start().to_usize()..range.end().to_usize()],
            }
        })
        .collect()
}

/// What a `noqa` directive in a comment says.
#[derive(Debug, Eq, PartialEq)]
pub enum Noqa<'a> {
    /// `# noqa` with no codes.
    Bare,
    /// `# noqa: A, B`.
    Codes(Vec<&'a str>),
}

/// The text after `keyword` when a comment segment (the text after one of
/// its `#`) starts with it, keyword case aside, followed by something that
/// cannot continue a word.
fn directive<'a>(segment: &'a str, keyword: &str) -> Option<&'a str> {
    let segment = segment.trim_start();
    let head = segment.get(..keyword.len())?;
    let rest = &segment[keyword.len()..];
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let continues = keyword.ends_with(word) && rest.starts_with(word);
    (head.eq_ignore_ascii_case(keyword) && !continues).then_some(rest)
}

/// Every `noqa` directive of a comment, in order. A directive may follow
/// other text after its own `#`, as in `# type: ignore  # noqa`.
fn noqa_directives(comment: &str) -> impl Iterator<Item = Noqa<'_>> {
    comment.split('#').skip(1).filter_map(|segment| {
        let rest = directive(segment, "noqa")?;
        let codes = rest.trim_start().strip_prefix(':').map(noqa_codes);
        Some(match codes {
            Some(codes) if !codes.is_empty() => Noqa::Codes(codes),
            _ => Noqa::Bare,
        })
    })
}

/// The first `noqa` directive of a comment, if it has one.
pub fn noqa(comment: &str) -> Option<Noqa<'_>> {
    noqa_directives(comment).next()
}

/// The codes a comment's directives name, as spelled: every
/// `ferrolint: disable=` and `pylint: disable=` list and every `noqa:`
/// list, keywords case aside. Other Pylint directives, such as
/// `disable-next=` or `enable=`, name nothing.
pub fn disabled_codes(comment: &str) -> Vec<&str> {
    let mut codes = Vec::new();
    for segment in comment.split('#').skip(1) {
        let payload = directive(segment, "ferrolint:").or_else(|| directive(segment, "pylint:"));
        let list = payload
            .and_then(|payload| directive(payload, "disable"))
            .and_then(|rest| rest.trim_start().strip_prefix('='));
        if let Some(list) = list {
            codes.extend(disable_codes(list));
        }
    }
    for noqa in noqa_directives(comment) {
        if let Noqa::Codes(listed) = noqa {
            codes.extend(listed);
        }
    }
    codes
}

/// A `disable=` list: comma-separated codes, ending at the first entry
/// followed by prose (`disable=W0612 -- because ...`).
fn disable_codes(list: &str) -> Vec<&str> {
    let mut codes = Vec::new();
    for entry in list.split(',') {
        let mut words = entry.split_whitespace();
        let Some(code) = words.next() else {
            continue;
        };
        if !code.chars().all(is_code_char) {
            break;
        }
        codes.push(code);
        if words.next().is_some() {
            break;
        }
    }
    codes
}

/// A `noqa:` list: codes separated by commas or spaces, ending at the first
/// word that does not look like a code (it has no digit, `-` or `_`).
fn noqa_codes(payload: &str) -> Vec<&str> {
    payload
        .split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .take_while(|code| {
            code.chars().all(is_code_char)
                && code
                    .chars()
                    .any(|c| c.is_ascii_digit() || c == '-' || c == '_')
        })
        .collect()
}

fn is_code_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// The codes each suppression comment names as spelled, by line. Only a
/// comment that shares its line with code counts; one on a line of its own
/// suppresses nothing.
pub fn spelled(source: &str, tokens: &Tokens, index: &LineIndex) -> BTreeMap<u32, Vec<String>> {
    let mut lines: BTreeMap<u32, Vec<String>> = BTreeMap::new();
    for comment in comments(source, tokens) {
        let line_start = source[..comment.start as usize]
            .rfind(['\n', '\r'])
            .map_or(0, |at| at + 1);
        if source[line_start..comment.start as usize].trim().is_empty() {
            continue;
        }
        let codes = disabled_codes(comment.text);
        if !codes.is_empty() {
            lines
                .entry(index.line(comment.start))
                .or_default()
                .extend(codes.into_iter().map(str::to_owned));
        }
    }
    lines
}

/// The suppressions of one file: the rule codes silenced on each line.
#[derive(Default)]
pub struct Suppressions {
    lines: HashMap<u32, HashSet<String>>,
}

impl Suppressions {
    /// Resolve each spelled code to its rule code; an unknown one is inert.
    pub fn resolve(spelled: &BTreeMap<u32, Vec<String>>, registry: &Registry) -> Self {
        let lines = spelled
            .iter()
            .map(|(line, codes)| {
                let codes = codes
                    .iter()
                    .filter_map(|spelling| registry.resolve(spelling))
                    .map(str::to_owned)
                    .collect();
                (*line, codes)
            })
            .collect();
        Self { lines }
    }

    /// Whether a comment on the first or last line of the diagnostic names
    /// its code. A plugin abort is never suppressed.
    pub fn suppresses(&self, diagnostic: &Diagnostic) -> bool {
        if diagnostic.code == checks::PLUGIN_ABORTED {
            return false;
        }
        let range = diagnostic.range;
        // The end is exclusive: a range ending at a line's first column
        // stops on the line before.
        let last = if range.end.column == 1 && range.end.line > range.start.line {
            range.end.line - 1
        } else {
            range.end.line
        };
        [range.start.line, last].iter().any(|line| {
            self.lines
                .get(line)
                .is_some_and(|codes| codes.contains(&diagnostic.code))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noqa_forms() {
        assert_eq!(noqa("# noqa"), Some(Noqa::Bare));
        assert_eq!(noqa("#NOQA"), Some(Noqa::Bare));
        assert_eq!(noqa("# noqa:"), Some(Noqa::Bare));
        assert_eq!(noqa("# noqa: W0611"), Some(Noqa::Codes(vec!["W0611"])));
        assert_eq!(
            noqa("# noqa: W0611,unused-import  (why?)"),
            Some(Noqa::Codes(vec!["W0611", "unused-import"]))
        );
        assert_eq!(noqa("# type: ignore  # noqa"), Some(Noqa::Bare));
        assert_eq!(noqa("# noqanot"), None);
        assert_eq!(noqa("# nothing here"), None);
    }

    #[test]
    fn disable_directives() {
        assert_eq!(disabled_codes("# ferrolint: disable=IMP001"), ["IMP001"]);
        assert_eq!(
            disabled_codes("#Pylint:Disable = W0611, unused-import"),
            ["W0611", "unused-import"]
        );
        assert_eq!(
            disabled_codes("# pylint: disable-next=W0611"),
            Vec::<&str>::new()
        );
        assert_eq!(disabled_codes("# pylint: enable=W0611"), Vec::<&str>::new());
        assert_eq!(
            disabled_codes("# type: ignore  # pylint: disable=W0611  # noqa: F401"),
            ["W0611", "F401"]
        );
        assert_eq!(disabled_codes("# noqa"), Vec::<&str>::new());
        assert_eq!(
            disabled_codes("# noqa: E501  # noqa: IMP001"),
            ["E501", "IMP001"]
        );
    }

    #[test]
    fn prose_after_a_list_is_not_codes() {
        assert_eq!(
            disabled_codes("# pylint: disable=W0612 -- not an unused-import"),
            ["W0612"]
        );
        assert_eq!(
            disabled_codes("# noqa: E501 because the line is long"),
            ["E501"]
        );
    }
}
