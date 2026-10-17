//! Comment directives: the line-level suppressions a comment carries
//! (`# ferrolint: disable=`, `# pylint: disable=` and `# noqa:`).

use std::collections::HashMap;

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

/// The `noqa` directive of a comment, if it has one. The directive may
/// follow other text after its own `#`, as in `# type: ignore  # noqa`.
pub fn noqa(comment: &str) -> Option<Noqa<'_>> {
    comment.split('#').skip(1).find_map(|segment| {
        let segment = segment.trim_start();
        let keyword = segment.get(..4)?;
        if !keyword.eq_ignore_ascii_case("noqa") {
            return None;
        }
        let rest = &segment[4..];
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        let codes = rest.trim_start().strip_prefix(':').map(split_codes);
        Some(match codes {
            Some(codes) if !codes.is_empty() => Noqa::Codes(codes),
            _ => Noqa::Bare,
        })
    })
}

/// The codes a comment's directives name, as spelled: every
/// `ferrolint: disable=` and `pylint: disable=` list (keyword case aside)
/// and every `noqa:` list. Other Pylint directives, such as
/// `disable-next=` or `enable=`, suppress nothing.
pub fn disabled_codes(comment: &str) -> Vec<&str> {
    let mut codes = Vec::new();
    for segment in comment.split('#').skip(1) {
        let segment = segment.trim_start();
        let payload = ["ferrolint:", "pylint:"].iter().find_map(|keyword| {
            let head = segment.get(..keyword.len())?;
            head.eq_ignore_ascii_case(keyword)
                .then(|| segment[keyword.len()..].trim_start())
        });
        if let Some(payload) = payload {
            let Some(rest) = payload.strip_prefix("disable") else {
                continue;
            };
            if let Some(list) = rest.trim_start().strip_prefix('=') {
                codes.extend(split_codes(list));
            }
        }
    }
    if let Some(Noqa::Codes(listed)) = noqa(comment) {
        codes.extend(listed);
    }
    codes
}

/// The suppressions of one file, by line.
#[derive(Default)]
pub struct Suppressions {
    lines: HashMap<u32, Vec<String>>,
}

impl Suppressions {
    pub fn new(source: &str, tokens: &Tokens, index: &LineIndex) -> Self {
        let mut lines: HashMap<u32, Vec<String>> = HashMap::new();
        for comment in comments(source, tokens) {
            let codes = disabled_codes(comment.text);
            if !codes.is_empty() {
                lines
                    .entry(index.line(comment.start))
                    .or_default()
                    .extend(codes.into_iter().map(str::to_owned));
            }
        }
        Self { lines }
    }

    /// Whether a comment on the line the diagnostic starts or ends on
    /// names its code, in any spelling the registry resolves. A plugin
    /// abort is never suppressed.
    pub fn suppresses(&self, registry: &Registry, diagnostic: &Diagnostic) -> bool {
        if diagnostic.code == checks::PLUGIN_ABORTED {
            return false;
        }
        let range = diagnostic.range;
        [range.start.line, range.end.line].iter().any(|line| {
            self.lines.get(line).is_some_and(|codes| {
                codes
                    .iter()
                    .any(|code| registry.resolve(code) == Some(diagnostic.code.as_str()))
            })
        })
    }
}

/// Split a comma- or space-separated code list, stopping at the first
/// token that is not a code.
fn split_codes(payload: &str) -> Vec<&str> {
    payload
        .split([',', ' ', '\t'])
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .take_while(|code| {
            code.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .collect()
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
            disabled_codes("#Pylint:disable = W0611, unused-import"),
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
    }
}
