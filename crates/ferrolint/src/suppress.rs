//! Comment directives: the line-level suppressions a comment carries
//! (`# ferrolint: disable=`, `# pylint: disable=` and `# noqa:`).

use std::collections::HashSet;

use ferrolint_core::{Diagnostic, LineIndex, Registry};
use ruff_python_ast::token::{TokenKind, Tokens};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{ModModule, Stmt};
use ruff_python_parser::Parsed;
use ruff_text_size::{Ranged, TextRange, TextSize};
use serde::{Deserialize, Serialize};

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
        let payload = directive(segment, "ferrolint:").or_else(|| pylint_payload(segment));
        let list = payload
            .and_then(|payload| {
                directive(payload, "disable-msg").or_else(|| directive(payload, "disable"))
            })
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

/// A `disable=` list: codes separated by commas or whitespace, ending at the
/// first word that is not one (`disable=W0612 -- because ...`). Trailing
/// punctuation after a code (`.`, `;`, `)`) is dropped, as Pylint's tokenizer
/// skips it.
fn disable_codes(list: &str) -> Vec<&str> {
    list.split(|c: char| c == ',' || c.is_whitespace())
        .filter(|code| !code.is_empty())
        .map(|code| code.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
        .take_while(|code| {
            code.chars().all(is_code_char) && code.chars().any(|c| c.is_ascii_alphanumeric())
        })
        .collect()
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

/// The text after a comment segment's `pylint:` marker. Pylint's tokenizer
/// skips what it does not know, so `pylint:: disable=` works.
fn pylint_payload(segment: &str) -> Option<&str> {
    directive(segment, "pylint:")
        .map(|payload| payload.trim_start_matches(|c: char| !c.is_ascii_alphanumeric()))
}

/// The codes of a comment's `pylint: <keyword>=` lists. `disable-msg` and
/// `enable-msg` are the old spellings of `disable` and `enable`.
fn pylint_codes<'a>(comment: &'a str, keyword: &str) -> Vec<&'a str> {
    let old = format!("{keyword}-msg");
    comment
        .split('#')
        .skip(1)
        .filter_map(|segment| {
            let payload = pylint_payload(segment)?;
            directive(payload, &old)
                .or_else(|| directive(payload, keyword))
                .and_then(|rest| rest.trim_start().strip_prefix('='))
        })
        .flat_map(disable_codes)
        .collect()
}

/// The codes a suppression comment names as spelled, over the lines it covers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Span {
    pub first: u32,
    pub last: u32,
    pub codes: Vec<String>,
}

/// A compound statement, or the module, that a Pylint `disable=` can scope to.
struct Block {
    range: TextRange,
    /// The name of a def or class (the module: offset 0), whose line starts
    /// the block for Pylint.
    head: Option<TextSize>,
    /// The first statement of the body after any docstring.
    first: Option<TextSize>,
}

fn first_child(body: &[Stmt]) -> Option<TextSize> {
    let docstring = matches!(body.first(), Some(Stmt::Expr(e)) if e.value.is_string_literal_expr());
    body.get(usize::from(docstring)).map(Ranged::start)
}

struct Blocks(Vec<Block>);

impl<'a> Visitor<'a> for Blocks {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let (head, first) = match stmt {
            Stmt::FunctionDef(def) => (Some(def.name.start()), first_child(&def.body)),
            Stmt::ClassDef(class) => (Some(class.name.start()), first_child(&class.body)),
            Stmt::If(_)
            | Stmt::For(_)
            | Stmt::While(_)
            | Stmt::With(_)
            | Stmt::Try(_)
            | Stmt::Match(_) => (None, None),
            _ => return,
        };
        self.0.push(Block {
            range: stmt.range(),
            head,
            first,
        });
        visitor::walk_stmt(self, stmt);
    }
}

/// The module, then every compound statement in source order, so the last
/// block containing an offset is the innermost.
fn blocks(source: &str, parsed: &Parsed<ModModule>) -> Vec<Block> {
    let body = &parsed.syntax().body;
    let mut blocks = Blocks(vec![Block {
        range: TextRange::up_to(TextSize::of(source)),
        head: Some(TextSize::ZERO),
        first: first_child(body),
    }]);
    blocks.visit_body(body);
    blocks.0
}

/// The codes each suppression comment names as spelled, over the lines it
/// covers. `ferrolint:` and `noqa:` count only on a line shared with code.
/// `pylint: disable-next=` covers the next line. `pylint: disable=` covers
/// its line when trailing; trailing a def or class line, the whole
/// definition; on a line of its own, from there to the end of the innermost
/// compound statement (the module), from the def line (line 1) when it comes
/// before the body's first statement.
pub fn spelled(source: &str, parsed: &Parsed<ModModule>, index: &LineIndex) -> Vec<Span> {
    let comments = comments(source, parsed.tokens());
    let scoped = comments
        .iter()
        .any(|comment| !pylint_codes(comment.text, "disable").is_empty());
    let blocks = if scoped {
        blocks(source, parsed)
    } else {
        Vec::new()
    };
    let end = |block: &Block| index.line(block.range.end().to_u32());
    let mut spans = Vec::new();
    let mut push = |first: u32, last: u32, codes: Vec<&str>| {
        if !codes.is_empty() {
            spans.push(Span {
                first,
                last,
                codes: codes.into_iter().map(str::to_owned).collect(),
            });
        }
    };
    for comment in &comments {
        let line = index.line(comment.start);
        let line_start = source[..comment.start as usize]
            .rfind(['\n', '\r'])
            .map_or(0, |at| at + 1);
        let own_line = source[line_start..comment.start as usize].trim().is_empty();
        if !own_line {
            push(line, line, disabled_codes(comment.text));
        }
        push(
            line + 1,
            line + 1,
            pylint_codes(comment.text, "disable-next"),
        );
        let at = TextSize::new(comment.start);
        let scope = if own_line {
            // shortcut: a comment after a block's last statement belongs to the
            // enclosing block, as its range ends there; upgrade if Pylint's
            // indentation-based ending is needed.
            blocks.iter().rfind(|b| b.range.contains(at)).map(|block| {
                let before_first = block.first.is_none_or(|first| at < first);
                let start = block.head.filter(|_| before_first);
                (
                    start.map_or(line, |head| index.line(head.to_u32())),
                    end(block),
                )
            })
        } else {
            // blocks[0] is the module: a trailing comment on line 1 is not a def line.
            blocks
                .iter()
                .skip(1)
                .find(|b| b.head.is_some_and(|head| index.line(head.to_u32()) == line))
                .map(|block| (line, end(block)))
        };
        if let Some((first, last)) = scope {
            let codes = pylint_codes(comment.text, "disable");
            // Pylint reports too-many-lines on line 1, so a module-level
            // pragma reaches it wherever it stands.
            let module_level = own_line && !blocks.iter().skip(1).any(|b| b.range.contains(at));
            if module_level {
                let lines = codes
                    .iter()
                    .copied()
                    .filter(|code| matches!(*code, "too-many-lines" | "C0302"))
                    .collect();
                push(1, last, lines);
            }
            push(first, last, codes);
        }
    }
    spans
}

/// The suppressions of one file: the rule codes silenced over line ranges.
#[derive(Default)]
pub struct Suppressions {
    ranges: Vec<(u32, u32, HashSet<String>)>,
}

/// Pylint's renamed messages: the old symbol or id, and the current id.
const OLD_NAMES: &[(&str, &str)] = &[
    ("broad-except", "W0718"),
    ("W0703", "W0718"),
    ("blacklisted-name", "C0104"),
    ("C0102", "C0104"),
];

impl Suppressions {
    /// Resolve each spelled code to its rule code; an unknown one is inert.
    pub fn resolve(spelled: &[Span], registry: &Registry) -> Self {
        let ranges = spelled
            .iter()
            .map(|span| {
                let codes = span
                    .codes
                    .iter()
                    .filter_map(|spelling| {
                        let current = OLD_NAMES
                            .iter()
                            .find(|(old, _)| old == spelling)
                            .map_or(spelling.as_str(), |(_, id)| id);
                        registry.resolve(current)
                    })
                    .map(str::to_owned)
                    .collect();
                (span.first, span.last, codes)
            })
            .collect();
        Self { ranges }
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
        self.ranges.iter().any(|(first, end, codes)| {
            codes.contains(&diagnostic.code)
                && [range.start.line, last]
                    .iter()
                    .any(|line| (*first..=*end).contains(line))
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
        assert_eq!(disabled_codes("# pylint:: disable=W0611"), ["W0611"]);
        assert_eq!(disabled_codes("# pylint: disable-msg=W0611"), ["W0611"]);
        assert_eq!(
            disabled_codes("# pylint: disable=W0611 W0612;"),
            ["W0611", "W0612"]
        );
        assert_eq!(disabled_codes("# pylint: disable=W0611)"), ["W0611"]);
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

    /// The `(first, last)` lines of every span `source` spells.
    fn lines(source: &str) -> Vec<(u32, u32)> {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        let spans = spelled(source, &parsed, &LineIndex::new(source));
        spans.iter().map(|span| (span.first, span.last)).collect()
    }

    #[test]
    fn own_line_pragma_reaches_the_end_of_the_module() {
        assert_eq!(
            lines("import os\n# pylint: disable=W0611\nimport sys\nx = 1\n"),
            [(2, 5)]
        );
    }

    #[test]
    fn own_line_pragma_ends_with_its_block() {
        let source = "def f():\n    a = 1\n    # pylint: disable=W0612\n    b = 2\nc = 3\n";
        assert_eq!(lines(source), [(3, 4)]);
    }

    #[test]
    fn pragma_before_the_first_statement_covers_the_def() {
        let source = "import os\n\ndef f():\n    \"\"\"Doc.\"\"\"\n    # pylint: disable=W0612\n    a = 1\nb = 2\n";
        assert_eq!(lines(source), [(3, 6)]);
        assert_eq!(lines("# pylint: disable=C0114\nx = 1\n"), [(1, 3)]);
    }

    #[test]
    fn pragma_after_the_last_statement_of_an_inner_block_ends_with_the_outer_one() {
        let source =
            "def f():\n    if x:\n        a = 1\n    # pylint: disable=W0612\n    b = 2\nc = 3\n";
        assert_eq!(lines(source), [(4, 5)]);
    }

    #[test]
    fn trailing_pragma_on_a_def_covers_the_definition() {
        let source = "@d\ndef f(a):  # pylint: disable=W0613\n    return 1\nx = 2\n";
        assert_eq!(lines(source), [(2, 2), (2, 3)]);
        assert_eq!(
            lines("@d  # pylint: disable=W0613\ndef f():\n    pass\n"),
            [(1, 1)]
        );
    }

    #[test]
    fn trailing_pragma_on_an_if_covers_its_line() {
        let source = "if x:  # pylint: disable=W0612\n    a = 1\n";
        assert_eq!(lines(source), [(1, 1)]);
    }

    #[test]
    fn disable_next_covers_the_next_line() {
        assert_eq!(
            lines("# pylint: disable-next=W0611\nimport os\nimport sys\n"),
            [(2, 2)]
        );
        assert_eq!(
            lines("x = 1  # pylint: disable-next=W0611\nimport os\n"),
            [(2, 2)]
        );
    }

    #[test]
    fn module_pragma_for_too_many_lines_reaches_line_one() {
        assert_eq!(
            lines("import os\n# pylint: disable=too-many-lines\nx = 1\n"),
            [(1, 4), (2, 4)]
        );
    }

    #[test]
    fn inert_pragmas() {
        assert_eq!(lines("# pylint: enable=W0611\nx = 1\n"), []);
        assert_eq!(lines("# pylint: skip-file\nx = 1\n"), []);
        assert_eq!(lines("# ferrolint: disable=IMP001\nx = 1\n"), []);
        assert_eq!(lines("# noqa: F401\nx = 1\n"), []);
    }
}
