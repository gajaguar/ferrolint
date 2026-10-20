//! R0801 duplicate-code: Pylint's `symilar`. Every linted file is reduced to
//! its stripped lines (no comments, docstrings, imports or signatures, as the
//! options say), a window of `min-similarity-lines` of them is hashed, and the
//! windows two files share are merged into runs.
//!
//! The windows hash as Pylint 4.1.3 does, by the sum of their lines' hashes,
//! so two windows of the same lines in another order also match; the pairs,
//! the runs and the message follow `symilar` step by step.
//!
//! shortcut: Pylint shows the lines of one of the files at random (a set's
//! iteration order); this shows the first file's. The files are taken in name
//! order, where Pylint takes them in the order of its file walk.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use ferrolint_core::LineIndex;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, Stmt};
use ruff_python_parser::Parsed;
use ruff_text_size::Ranged;

/// What of the options this rule reads.
pub struct Settings {
    pub min: usize,
    comments: bool,
    docstrings: bool,
    imports: bool,
    signatures: bool,
}

impl Settings {
    pub fn new(options: &[(String, String)]) -> Self {
        let get = |key: &str| {
            options
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.trim().to_ascii_lowercase())
        };
        let yn = |key: &str| !matches!(get(key).as_deref(), Some("n" | "no" | "false" | "0"));
        Self {
            min: get("min-similarity-lines")
                .and_then(|value| value.parse().ok())
                .unwrap_or(4),
            comments: yn("ignore-comments"),
            docstrings: yn("ignore-docstrings"),
            imports: yn("ignore-imports"),
            signatures: yn("ignore-signatures"),
        }
    }
}

/// One file: its lines as read, and the ones that count, with their
/// 0-based numbers.
pub struct LineSet {
    name: String,
    real: Vec<String>,
    stripped: Vec<(String, u32)>,
}

impl LineSet {
    /// `disabled` tells whether a 1-based line has `duplicate-code` disabled.
    pub fn new(
        name: &str,
        source: &str,
        parsed: &Parsed<ruff_python_ast::ModModule>,
        index: &LineIndex,
        settings: &Settings,
        disabled: impl Fn(u32) -> bool,
    ) -> Self {
        let real = split_lines(source);
        let mut ignored = HashSet::new();
        if settings.imports {
            let mut imports = Imports(index, &mut ignored);
            imports.visit_body(&parsed.syntax().body);
        }
        if settings.signatures {
            let mut functions = Vec::new();
            collect(&parsed.syntax().body, &mut functions);
            for function in functions {
                let first = function
                    .decorator_list
                    .first()
                    .map_or(function.name.start(), |d| d.expression.start());
                let first = index.line(first.to_u32());
                let body = skip_docstring(&function.body);
                let last = body.first().map_or_else(
                    || index.line(function.end().to_u32()) + 1,
                    |stmt| lineno(stmt, index),
                );
                ignored.extend(first..last);
            }
        }
        let stripped = strip(&real, &ignored, settings, disabled);
        Self {
            name: name.to_owned(),
            real,
            stripped,
        }
    }
}

/// Python's `str.splitlines(keepends=True)`.
fn split_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        let end = match c {
            '\r' if matches!(chars.peek(), Some((_, '\n'))) => {
                chars.next();
                at + 2
            }
            '\r' | '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => at + c.len_utf8(),
            _ => continue,
        };
        lines.push(text[start..end].to_owned());
        start = end;
    }
    if start < text.len() {
        lines.push(text[start..].to_owned());
    }
    lines
}

/// Every line an `import` covers, at any depth.
struct Imports<'a>(&'a LineIndex, &'a mut HashSet<u32>);

impl<'a> Visitor<'a> for Imports<'_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)) {
            let first = self.0.line(stmt.start().to_u32());
            let last = self.0.line(stmt.end().to_u32());
            self.1.extend(first..=last);
        }
        visitor::walk_stmt(self, stmt);
    }
}

/// The functions Pylint's `_get_functions` finds: those of the module and,
/// recursively, of the bodies of its functions and classes.
fn collect<'a>(body: &'a [Stmt], out: &mut Vec<&'a ruff_python_ast::StmtFunctionDef>) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(function) => {
                out.push(function);
                collect(&function.body, out);
            }
            Stmt::ClassDef(class) => collect(&class.body, out),
            _ => {}
        }
    }
}

/// astroid keeps the docstring out of a body.
fn skip_docstring(body: &[Stmt]) -> &[Stmt] {
    match body.first() {
        Some(Stmt::Expr(expr)) if matches!(&*expr.value, Expr::StringLiteral(_)) => &body[1..],
        _ => body,
    }
}

/// astroid's `lineno`: a decorated function starts at its first decorator.
fn lineno(stmt: &Stmt, index: &LineIndex) -> u32 {
    let at = match stmt {
        Stmt::FunctionDef(f) => f
            .decorator_list
            .first()
            .map_or(f.name.start(), |d| d.expression.start()),
        Stmt::ClassDef(c) => c.name.start(),
        other => other.start(),
    };
    index.line(at.to_u32())
}

/// Pylint's `stripped_lines`.
fn strip(
    lines: &[String],
    ignored: &HashSet<u32>,
    settings: &Settings,
    disabled: impl Fn(u32) -> bool,
) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let mut docstring: Option<&str> = None;
    for (at, raw) in lines.iter().enumerate() {
        let number = u32::try_from(at + 1).unwrap_or(u32::MAX);
        if disabled(number) {
            continue;
        }
        let mut line = raw.trim();
        if settings.docstrings {
            if docstring.is_none() {
                for (quote, skip) in [("\"\"\"", 0), ("'''", 0), ("r\"\"\"", 1), ("r'''", 1)] {
                    if line.starts_with(quote) {
                        docstring = Some(&quote[skip..skip + 3]);
                        line = &line[quote.len()..];
                        break;
                    }
                }
            }
            if let Some(quote) = docstring {
                if line.ends_with(quote) {
                    docstring = None;
                }
                line = "";
            }
        }
        if settings.comments {
            line = line.split('#').next().unwrap_or_default().trim();
        }
        if ignored.contains(&number) {
            line = "";
        }
        if !line.is_empty() {
            out.push((line.to_owned(), number - 1));
        }
    }
    out
}

/// The hashes of a file's windows of `min` lines, and each window's span.
struct Windows {
    by_hash: HashMap<u64, Vec<usize>>,
    spans: Vec<(u32, u32)>,
}

fn windows(set: &LineSet, min: usize) -> Windows {
    let mut by_hash: HashMap<u64, Vec<usize>> = HashMap::new();
    let mut spans = Vec::new();
    let lines = &set.stripped;
    if lines.len() < min {
        return Windows { by_hash, spans };
    }
    let hashes: Vec<u64> = lines
        .iter()
        .map(|(text, _)| {
            let mut hasher = DefaultHasher::new();
            text.hash(&mut hasher);
            hasher.finish()
        })
        .collect();
    let mut window = hashes[..min]
        .iter()
        .fold(0u64, |sum, hash| sum.wrapping_add(*hash));
    for first in 0..=lines.len() - min {
        let after = first + min;
        let end = lines
            .get(after)
            .map_or(lines[lines.len() - 1].1 + 1, |line| line.1);
        spans.push((lines[first].1, end));
        by_hash.entry(window).or_default().push(first);
        if after < lines.len() {
            window = window
                .wrapping_sub(hashes[first])
                .wrapping_add(hashes[after]);
        }
    }
    Windows { by_hash, spans }
}

/// A run of common windows, from a pair of window indices.
struct Run {
    first: (u32, u32),
    second: (u32, u32),
    lines: usize,
}

/// Whether the text has a word character, as Pylint's `.*\w+` match.
fn has_content(text: &str) -> bool {
    text.chars().any(|c| c.is_alphanumeric() || c == '_')
}

/// The lines with content among `count` stripped ones from `from`.
fn texts(set: &LineSet, from: usize, count: usize) -> Vec<&str> {
    set.stripped
        .iter()
        .skip(from)
        .take(count)
        .map(|(text, _)| text.as_str())
        .filter(|text| has_content(text))
        .collect()
}

/// What two files have in common (`_find_common`): the count of lines and
/// the two spans of each run that is long enough.
fn common(
    one: &LineSet,
    two: &LineSet,
    a: &Windows,
    b: &Windows,
    min: usize,
) -> Vec<(usize, (u32, u32), (u32, u32))> {
    let mut hashes: Vec<u64> = a
        .by_hash
        .keys()
        .filter(|hash| b.by_hash.contains_key(hash))
        .copied()
        .collect();
    hashes.sort_by_key(|hash| a.by_hash[hash][0]);
    // The runs in the order they were found, and where each one is.
    let mut runs: Vec<Option<Run>> = Vec::new();
    let mut slot: HashMap<(usize, usize), usize> = HashMap::new();
    let mut keys: Vec<(usize, usize)> = Vec::new();
    for hash in hashes {
        let (first, second) = (&a.by_hash[&hash], &b.by_hash[&hash]);
        let pairs: Vec<(usize, usize)> = if first.len() * second.len() > 500 {
            first.iter().copied().zip(second.iter().copied()).collect()
        } else {
            first
                .iter()
                .flat_map(|&i| second.iter().map(move |&j| (i, j)))
                .collect()
        };
        for (i, j) in pairs {
            slot.insert((i, j), runs.len());
            keys.push((i, j));
            runs.push(Some(Run {
                first: a.spans[i],
                second: b.spans[j],
                lines: min,
            }));
        }
    }
    // `remove_successive`: a run that starts one window later extends the run
    // before it.
    for key in &keys {
        let mut next = (key.0 + 1, key.1 + 1);
        let Some(&head) = slot.get(key) else { continue };
        while let Some(&at) = slot.get(&next) {
            let Some(tail) = runs[at].take() else { break };
            slot.remove(&next);
            if let Some(run) = runs[head].as_mut() {
                run.first.1 = tail.first.1;
                run.second.1 = tail.second.1;
                run.lines += 1;
            }
            next = (next.0 + 1, next.1 + 1);
        }
    }
    let mut found = Vec::new();
    for (key, run) in keys.iter().zip(&runs) {
        let Some(run) = run else { continue };
        if !slot.contains_key(key) {
            continue;
        }
        let same = texts(one, key.0, run.lines)
            .into_iter()
            .zip(texts(two, key.1, run.lines))
            .filter(|(x, y)| x == y)
            .count();
        if same > min {
            found.push((run.lines, run.first, run.second));
        }
    }
    found
}

/// The messages of the files' duplicates, longest first: the text of each,
/// without the position.
pub fn report(sets: &[LineSet], min: usize) -> Vec<String> {
    if min == 0 {
        return Vec::new();
    }
    let windows: Vec<Windows> = sets.iter().map(|set| windows(set, min)).collect();
    // Only the pairs of files that share a window are compared, in Pylint's order.
    let mut owners: HashMap<u64, Vec<usize>> = HashMap::new();
    for (at, w) in windows.iter().enumerate() {
        for hash in w.by_hash.keys() {
            owners.entry(*hash).or_default().push(at);
        }
    }
    let mut pairs: std::collections::BTreeMap<(usize, usize), ()> = Default::default();
    for files in owners.values().filter(|files| files.len() > 1) {
        for (n, &i) in files.iter().enumerate() {
            for &j in &files[n + 1..] {
                pairs.insert((i.min(j), i.max(j)), ());
            }
        }
    }
    // Per count of lines, the groups in the order found; a pair that shares
    // a chunk with a group before it is dropped, as Pylint does.
    type Chunk = (usize, u32, u32);
    let mut by_count: Vec<(usize, Vec<[Chunk; 2]>)> = Vec::new();
    for &(i, j) in pairs.keys() {
        for (count, first, second) in common(&sets[i], &sets[j], &windows[i], &windows[j], min) {
            let chunks = [(i, first.0, first.1), (j, second.0, second.1)];
            let at = by_count
                .iter()
                .position(|(n, _)| *n == count)
                .unwrap_or_else(|| {
                    by_count.push((count, Vec::new()));
                    by_count.len() - 1
                });
            let groups = &mut by_count[at].1;
            if !groups.iter().any(|g| g.iter().any(|c| chunks.contains(c))) {
                groups.push(chunks);
            }
        }
    }
    // `sorted(reverse=True)` keeps the order of equal counts.
    let mut sims: Vec<(usize, [Chunk; 2])> = by_count
        .into_iter()
        .flat_map(|(count, groups)| groups.into_iter().map(move |g| (count, g)))
        .collect();
    sims.sort_by(|a, b| b.0.cmp(&a.0));
    sims.into_iter()
        .map(|(_, chunks)| {
            let mut head: Vec<String> = chunks
                .iter()
                .map(|(set, start, end)| format!("=={}:[{start}:{end}]", sets[*set].name))
                .collect();
            head.sort();
            let (set, start, end) = chunks[0];
            let body = sets[set]
                .real
                .iter()
                .skip(start as usize)
                .take((end as usize).saturating_sub(start as usize))
                .map(|line| line.trim_end().to_owned());
            let text: Vec<String> = head.into_iter().chain(body).collect();
            format!(
                "Similar lines in {} files\n{}",
                chunks.len(),
                text.join("\n")
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(name: &str, source: &str, settings: &Settings) -> LineSet {
        let parsed = ruff_python_parser::parse_module(source).unwrap();
        LineSet::new(
            name,
            source,
            &parsed,
            &LineIndex::new(source),
            settings,
            |_| false,
        )
    }

    #[test]
    fn a_run_of_common_lines_is_reported_once_with_the_lines() {
        let settings = Settings::new(&[]);
        let a = "import os\n\n\ndef first(values):\n    \"\"\"Sum.\"\"\"\n    total = 0\n    for value in values:\n        total += value\n        total -= 1\n    return total\n";
        let b = "import sys\n\n\ndef second(values):\n    total = 0\n    for value in values:  # loop\n        total += value\n        total -= 1\n    return total\n\nPAD = 1\n";
        let found = report(&[set("a", a, &settings), set("b", b, &settings)], 4);
        assert_eq!(
            found,
            [
                "Similar lines in 2 files\n==a:[5:10]\n==b:[4:10]\n    total = 0\n    for value in values:\n        total += value\n        total -= 1\n    return total"
            ]
        );
    }

    #[test]
    fn exactly_the_minimum_is_not_enough() {
        let settings = Settings::new(&[]);
        let a = "x = 1\ny = 2\nz = 3\nw = 4\nA = 1\n";
        let b = "x = 1\ny = 2\nz = 3\nw = 4\nB = 1\n";
        assert!(report(&[set("a", a, &settings), set("b", b, &settings)], 4).is_empty());
        let a = format!("{a}v = 5\n");
        let b = format!("{b}v = 5\n");
        assert_eq!(
            report(&[set("a", &a, &settings), set("b", &b, &settings)], 4).len(),
            0
        );
    }

    #[test]
    fn the_ignore_options_decide_what_counts() {
        let on = Settings::new(&[]);
        let off = Settings::new(&[("ignore-imports".into(), "n".into())]);
        let a = "import a1\nimport a2\nimport a3\nimport a4\nimport a5\nimport a6\n";
        let b = "import a1\nimport a2\nimport a3\nimport a4\nimport a5\nimport a6\n";
        assert!(report(&[set("a", a, &on), set("b", b, &on)], 4).is_empty());
        assert_eq!(report(&[set("a", a, &off), set("b", b, &off)], 4).len(), 1);
    }
}
