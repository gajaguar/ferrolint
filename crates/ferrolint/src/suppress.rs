//! Comment directives: the `noqa` payload a comment carries.

use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_text_size::Ranged;

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
}
