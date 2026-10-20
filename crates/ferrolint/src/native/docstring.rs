//! The docstring parser of `pylint.extensions.docparams`: Pylint's
//! `_check_docs_utils.py` (Sphinx, Epytext, Google and `NumPy` styles), with
//! its regular expressions copied pattern by pattern, so a docstring is
//! read as Pylint reads it.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;

/// Compile a pattern of this file; they are constants, so a failure is a bug.
#[allow(clippy::expect_used)]
fn re(pattern: &str) -> Regex {
    regex::RegexBuilder::new(pattern)
        .size_limit(64 << 20)
        .build()
        .expect("docstring pattern")
}

/// `re_type` of `SphinxDocstring`: a valid Python name with an optional link prefix.
const TYPE: &str = r"[~!.]?\w(?:\w|\.[^.])*";
const SIMPLE_CONTAINER: &str = r"[~!.]?\w(?:\w|\.[^.])*[(\[][^\n\s]+[)\]]";
const XREF: &str = r"(?::\w+:)?`[~!.]?\w(?:\w|\.[^.])*`";
const SEP: &str = r"(?:\s+(?:of|or)\s+|\s*,\s*|\s+\|\s+)";

fn multiple_simple_type() -> String {
    format!("(?:{SIMPLE_CONTAINER}|{TYPE})(?:{SEP}(?:{SIMPLE_CONTAINER}|{TYPE}))*")
}

fn container_type() -> String {
    format!(r"(?:{TYPE}|{XREF})[(\[][^\n]+[)\]]")
}

fn multiple_type() -> String {
    let c = container_type();
    format!("(?:{c}|{TYPE}|{XREF})(?:{SEP}(?:{c}|{TYPE}|{XREF}))*")
}

/// The patterns of the Sphinx style; Epytext is the same with `@` for the
/// first `:` of each.
struct Sphinx {
    param: Regex,
    ty: Regex,
    property_type: Regex,
    raise: Regex,
    rtype: Regex,
    returns: Regex,
}

impl Sphinx {
    fn new(sigil: char) -> Self {
        let multi = multiple_simple_type();
        let epytext = sigil == '@';
        Self {
            param: re(&format!(
                r"(?s){sigil}(?:param|parameter|arg|argument|key|keyword)\s+(?:({TYPE}|{SIMPLE_CONTAINER})\s+)?((\\\*{{0,2}}\w+)|(\w+))\s*:"
            )),
            ty: re(&format!(r"(?s){sigil}type\s+({multi})\s*:")),
            property_type: re(&format!(r"(?s){sigil}type:\s+{multi}")),
            raise: re(&format!(
                r"(?s){sigil}(?:raises?|except|exception)\s+({multi})\s*:"
            )),
            rtype: re(if epytext {
                "(?s)@(?:rtype|returntype):"
            } else {
                ":rtype:"
            }),
            returns: re(if epytext { "@returns?:" } else { ":returns?:" }),
        }
    }
}

static SPHINX: LazyLock<Sphinx> = LazyLock::new(|| Sphinx::new(':'));
static EPYTEXT: LazyLock<Sphinx> = LazyLock::new(|| Sphinx::new('@'));

/// The patterns of the Google style and, with other sections, of `NumPy`.
struct Sections {
    param_section: Regex,
    raise_section: Regex,
    raise_line: Regex,
    returns_section: Regex,
    returns_line: Regex,
    yields_section: Regex,
    param_line: Regex,
}

fn google_section(header: &str) -> Regex {
    re(&format!(r"(?sm)^([ ]*){header}\s*:\s*$(.*)"))
}

fn numpy_section(header: &str) -> Regex {
    re(&format!(r"(?sm)^([ ]*){header}\s*?$\s*[-=]+\s*?$(.*)"))
}

static GOOGLE: LazyLock<Sections> = LazyLock::new(|| {
    let multi = multiple_type();
    let returns_line = re(&format!(r"(?sm)\A\s*({multi}:)?\s*(.*)"));
    Sections {
        param_section: google_section("(?:Args|Arguments|Parameters)"),
        raise_section: google_section("Raises"),
        raise_line: re(&format!(r"(?sm)\A\s*({multi})\s*:\s*(.*)")),
        returns_section: google_section("Returns?"),
        returns_line,
        yields_section: google_section("Yields?"),
        param_line: re(&format!(
            r"(?sm)\A\s*((?:\\?\*{{0,2}})?[\w\\]+)\s*([(]{multi}(?:,\s+optional)?[)])?\s*:\s*(.*)"
        )),
    }
});

static NUMPY: LazyLock<Sections> = LazyLock::new(|| {
    let multi = multiple_type();
    Sections {
        param_section: numpy_section("(?:Args|Arguments|Parameters)"),
        raise_section: numpy_section("Raises"),
        raise_line: re(&format!(r"(?sm)\A\s*({TYPE})$\s*(.*)")),
        returns_section: numpy_section("Returns?"),
        returns_line: re(&format!(r"(?sm)\A\s*(?:\w+\s+:\s+)?({multi})$\s*(.*)")),
        yields_section: numpy_section("Yields?"),
        param_line: re(
            r"(?s)\A\s*(?P<param_name>\*{0,2}\w+(?:\s*,\s*\*{0,2}\w+)*)(?:[ \t]*:[ \t]*(?P<param_type>[^\n]*?)[ \t]*)?(?:\n|$)(?:\s*(?P<param_desc>.*))?",
        ),
    }
});

/// `re_keyword_param_section`, which `NumPy` inherits from Google unchanged.
static KEYWORD_PARAM_SECTION: LazyLock<Regex> =
    LazyLock::new(|| google_section(r"Keyword\s(?:Args|Arguments|Parameters)"));

static PROPERTY_RETURNS_LINE: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"(?sm)^{}:\s*(.*)", multiple_type())));

static PARAMETERS_SEE: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?s)For\s+the\s+(other)?\s*parameters\s*,\s+see"));

static HEADER_UNDERLINE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*-+$"));

static EXC_SPLIT: LazyLock<Regex> = LazyLock::new(|| re(r"(\s*,(?:\s*or\s)?\s*|\s+or\s+)"));

/// Python's `str.isspace` for one character.
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

fn lstrip(text: &str) -> &str {
    text.trim_start_matches(is_space)
}

/// Python's `str.expandtabs()`.
fn expand_tabs(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut column = 0;
    for c in text.chars() {
        match c {
            '\t' => {
                let n = 8 - column % 8;
                out.extend(std::iter::repeat_n(' ', n));
                column += n;
            }
            '\n' | '\r' => {
                out.push(c);
                column = 0;
            }
            _ => {
                out.push(c);
                column += 1;
            }
        }
    }
    out
}

/// Python's `str.splitlines()`.
fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if !matches!(
            c,
            '\n' | '\r' | '\x0b' | '\x0c' | '\x1c'..='\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}'
        ) {
            continue;
        }
        lines.push(&text[start..i]);
        start = i + c.len_utf8();
        if c == '\r'
            && let Some(&(j, '\n')) = chars.peek()
        {
            chars.next();
            start = j + 1;
        }
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// `_split_multiple_exc_types`: `re.split` with a captured delimiter keeps
/// the delimiters in the result, as Pylint's code does.
fn split_exceptions(target: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut last = 0;
    for found in EXC_SPLIT.find_iter(target) {
        parts.push(&target[last..found.start()]);
        parts.push(found.as_str());
        last = found.end();
    }
    parts.push(&target[last..]);
    parts
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Default,
    Sphinx,
    Epytext,
    Google,
    Numpy,
}

/// A parsed docstring: Pylint's `Docstring` and its subclasses.
pub(super) struct Docstring {
    kind: Kind,
    doc: String,
}

impl Docstring {
    fn new(kind: Kind, doc: &str) -> Self {
        Self {
            kind,
            doc: expand_tabs(doc),
        }
    }

    /// `docstringify`: the style whose sections match the most, ties going to
    /// the first of Sphinx, Epytext, Google and `NumPy`, and none to
    /// `default_type` (`default-docstring-type`).
    pub(super) fn parse(doc: Option<&str>, default_type: &str) -> Self {
        let doc = doc.unwrap_or("");
        let default = match default_type {
            "sphinx" => Kind::Sphinx,
            "epytext" => Kind::Epytext,
            "google" => Kind::Google,
            "numpy" => Kind::Numpy,
            _ => Kind::Default,
        };
        let mut best = (0, Self::new(default, doc));
        for kind in [Kind::Sphinx, Kind::Epytext, Kind::Google, Kind::Numpy] {
            let candidate = Self::new(kind, doc);
            let sections = candidate.matching_sections();
            if sections > best.0 {
                best = (sections, candidate);
            }
        }
        best.1
    }

    /// The text of the docstring, tabs expanded; empty when there is none.
    pub(super) fn text(&self) -> &str {
        &self.doc
    }

    fn sphinx(&self) -> &'static Sphinx {
        if self.kind == Kind::Epytext {
            &EPYTEXT
        } else {
            &SPHINX
        }
    }

    fn sections(&self) -> &'static Sections {
        if self.kind == Kind::Numpy {
            &NUMPY
        } else {
            &GOOGLE
        }
    }

    fn is_sphinx(&self) -> bool {
        matches!(self.kind, Kind::Sphinx | Kind::Epytext)
    }

    fn is_google(&self) -> bool {
        matches!(self.kind, Kind::Google | Kind::Numpy)
    }

    /// Whether the style has a `Yields` section, so a generator documents
    /// its output there and not under `Returns`.
    pub(super) fn supports_yields(&self) -> bool {
        self.is_google()
    }

    pub(super) fn matching_sections(&self) -> usize {
        let doc = &self.doc;
        let hits = if self.is_sphinx() {
            let s = self.sphinx();
            [
                s.param.is_match(doc),
                s.raise.is_match(doc),
                s.rtype.is_match(doc),
                s.returns.is_match(doc),
                s.property_type.is_match(doc),
            ]
            .to_vec()
        } else if self.is_google() {
            let s = self.sections();
            [
                s.param_section.is_match(doc),
                s.raise_section.is_match(doc),
                s.returns_section.is_match(doc),
                s.yields_section.is_match(doc),
                PROPERTY_RETURNS_LINE.is_match(self.first_line()),
            ]
            .to_vec()
        } else {
            Vec::new()
        };
        hits.into_iter().filter(|hit| *hit).count()
    }

    pub(super) fn exceptions(&self) -> BTreeSet<String> {
        let mut types = BTreeSet::new();
        if self.is_sphinx() {
            for found in self.sphinx().raise.captures_iter(&self.doc) {
                types.extend(split_exceptions(&found[1]).into_iter().map(str::to_owned));
            }
        } else if self.is_google() {
            let s = self.sections();
            for entry in self.parse_section(&s.raise_section) {
                let Some(found) = s.raise_line.captures(&entry) else {
                    continue;
                };
                if !found[2].is_empty() {
                    types.extend(split_exceptions(&found[1]).into_iter().map(str::to_owned));
                }
            }
        }
        types
    }

    pub(super) fn has_params(&self) -> bool {
        if self.doc.is_empty() {
            return false;
        }
        if self.is_sphinx() {
            self.sphinx().param.is_match(&self.doc)
        } else if self.is_google() {
            self.sections().param_section.is_match(&self.doc)
        } else {
            false
        }
    }

    pub(super) fn has_returns(&self) -> bool {
        if self.doc.is_empty() {
            return false;
        }
        if self.is_sphinx() {
            self.sphinx().returns.is_match(&self.doc)
        } else if self.is_google() {
            self.section_group(true, 2)
        } else {
            false
        }
    }

    pub(super) fn has_rtype(&self) -> bool {
        if self.doc.is_empty() {
            return false;
        }
        if self.is_sphinx() {
            self.sphinx().rtype.is_match(&self.doc)
        } else if self.is_google() {
            self.section_group(true, 1)
        } else {
            false
        }
    }

    pub(super) fn has_property_returns(&self) -> bool {
        match self.kind {
            Kind::Sphinx => !self.doc.is_empty() && !lstrip(&self.doc).starts_with(':'),
            Kind::Epytext => self.has_property_type() && !lstrip(&self.doc).starts_with('@'),
            Kind::Google | Kind::Numpy => {
                let s = self.sections();
                let first = self.first_line();
                !(s.param_section.is_match(first)
                    || s.raise_section.is_match(first)
                    || s.returns_section.is_match(first)
                    || s.yields_section.is_match(first))
            }
            Kind::Default => false,
        }
    }

    pub(super) fn has_property_type(&self) -> bool {
        if self.doc.is_empty() {
            return false;
        }
        if self.is_sphinx() {
            self.sphinx().property_type.is_match(&self.doc)
        } else if self.is_google() {
            // `re.match` on the first line: the pattern is anchored by `^`.
            PROPERTY_RETURNS_LINE.is_match(self.first_line())
        } else {
            false
        }
    }

    pub(super) fn has_yields(&self) -> bool {
        !self.doc.is_empty() && self.is_google() && self.section_group(false, 2)
    }

    pub(super) fn has_yields_type(&self) -> bool {
        !self.doc.is_empty() && self.is_google() && self.section_group(false, 1)
    }

    /// Whether an entry of the `Returns` (or `Yields`) section has a
    /// non-empty `group` of the entry's line pattern.
    fn section_group(&self, returns: bool, group: usize) -> bool {
        let s = self.sections();
        let section = if returns {
            &s.returns_section
        } else {
            &s.yields_section
        };
        self.parse_section(section).iter().any(|entry| {
            s.returns_line
                .captures(entry)
                .and_then(|found| found.get(group))
                .is_some_and(|m| !m.as_str().is_empty())
        })
    }

    /// The names with a description and the names with a type.
    pub(super) fn match_param_docs(&self) -> (BTreeSet<String>, BTreeSet<String>) {
        let mut with_doc = BTreeSet::new();
        let mut with_type = BTreeSet::new();
        if self.is_sphinx() {
            let s = self.sphinx();
            for found in s.param.captures_iter(&self.doc) {
                // Remove the escape characters necessary for asterisks.
                let name = found[2].replace('\\', "");
                if found.get(1).is_some() {
                    with_type.insert(name.clone());
                }
                with_doc.insert(name);
            }
            for found in s.ty.captures_iter(&self.doc) {
                with_type.insert(found[1].to_owned());
            }
        } else if self.is_google() {
            let s = self.sections();
            let mut entries = self.parse_section(&s.param_section);
            entries.extend(self.parse_section(&KEYWORD_PARAM_SECTION));
            for entry in entries {
                if self.kind == Kind::Numpy {
                    numpy_param(&s.param_line, &entry, &mut with_doc, &mut with_type);
                    continue;
                }
                let Some(found) = s.param_line.captures(&entry) else {
                    continue;
                };
                let name = found[1].replace('\\', "");
                if found.get(2).is_some_and(|m| !m.as_str().is_empty()) {
                    with_type.insert(name.clone());
                }
                if found.get(3).is_some_and(|m| !m.as_str().is_empty()) {
                    with_doc.insert(name);
                }
            }
        }
        (with_doc, with_type)
    }

    pub(super) fn params_documented_elsewhere(&self) -> bool {
        PARAMETERS_SEE.is_match(&self.doc)
    }

    fn first_line(&self) -> &str {
        lstrip(&self.doc).split('\n').next().unwrap_or("")
    }

    /// `_parse_section`: the entries of the section `section_re` finds, by
    /// indentation (and, for `NumPy`, up to a `----` line).
    fn parse_section(&self, section_re: &Regex) -> Vec<String> {
        let Some(found) = section_re.captures(&self.doc) else {
            return Vec::new();
        };
        let numpy = self.kind == Kind::Numpy;
        let indent = found.get(1).map_or(0, |m| m.as_str().chars().count());
        let mut min_indentation = if numpy { indent } else { indent + 1 };
        let mut entries = Vec::new();
        let mut entry: Vec<&str> = Vec::new();
        let mut first = true;
        for line in splitlines(found.get(2).map_or("", |m| m.as_str())) {
            if line.chars().all(is_space) {
                continue;
            }
            let indentation = line.chars().count() - line.trim_start_matches(' ').chars().count();
            if indentation < min_indentation {
                break;
            }
            // The first line after the header defines the minimum indentation.
            if first {
                min_indentation = indentation;
                first = false;
            }
            if indentation == min_indentation {
                if numpy && HEADER_UNDERLINE.is_match(line) {
                    break;
                }
                // A line of the minimum indentation begins a new entry.
                if !entry.is_empty() {
                    entries.push(entry.join("\n"));
                    entry.clear();
                }
            }
            entry.push(line);
        }
        if !entry.is_empty() {
            entries.push(entry.join("\n"));
        }
        entries
    }
}

/// `NumpyDocstring.match_param_docs` for one entry.
fn numpy_param(
    line: &Regex,
    entry: &str,
    with_doc: &mut BTreeSet<String>,
    with_type: &mut BTreeSet<String>,
) {
    let Some(found) = line.captures(entry) else {
        return;
    };
    let group = |name: &str| found.name(name).map_or("", |m| m.as_str());
    let ty = group("param_type");
    // A lone type on the name line stands for the description too.
    let desc = if group("param_desc").is_empty() {
        ty
    } else {
        group("param_desc")
    };
    for name in group("param_name").split(',').map(str::trim) {
        if !ty.is_empty() {
            with_type.insert(name.to_owned());
        }
        if !desc.is_empty() {
            with_doc.insert(name.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Docstring;

    fn names(set: &std::collections::BTreeSet<String>) -> Vec<&str> {
        set.iter().map(String::as_str).collect()
    }

    #[test]
    fn sphinx() {
        let doc = Docstring::parse(
            Some(
                "Sum.\n\n:param int a: first\n:param b: second\n:type b: str\n:param \\*args: rest\n:returns: total\n:rtype: int\n:raises ValueError, KeyError: bad\n",
            ),
            "default",
        );
        let (with_doc, with_type) = doc.match_param_docs();
        assert_eq!(names(&with_doc), ["*args", "a", "b"]);
        assert_eq!(names(&with_type), ["a", "b"]);
        assert!(doc.has_returns() && doc.has_rtype() && doc.has_params());
        assert!(!doc.supports_yields());
        assert!(doc.exceptions().contains("KeyError"));
    }

    #[test]
    fn google() {
        let doc = Docstring::parse(
            Some(
                "Sum.\n\nArgs:\n    a (int): first\n    b: second\n        more\n    *rest: other\n\nReturns:\n    int: total\n\nRaises:\n    ValueError: bad\n",
            ),
            "default",
        );
        let (with_doc, with_type) = doc.match_param_docs();
        assert_eq!(names(&with_doc), ["*rest", "a", "b"]);
        assert_eq!(names(&with_type), ["a"]);
        assert!(doc.has_returns() && doc.has_rtype() && doc.supports_yields());
        assert_eq!(names(&doc.exceptions()), ["ValueError"]);
        assert!(!doc.has_yields());
    }

    #[test]
    fn numpy() {
        let doc = Docstring::parse(
            Some(
                "Sum.\n\nParameters\n----------\na, b : int\n    the operands\nc\n    third\n\nReturns\n-------\nint\n    total\n\nYields\n------\nint\n    each\n",
            ),
            "default",
        );
        let (with_doc, with_type) = doc.match_param_docs();
        assert_eq!(names(&with_doc), ["a", "b", "c"]);
        assert_eq!(names(&with_type), ["a", "b"]);
        assert!(doc.has_returns() && doc.has_rtype() && doc.has_yields() && doc.has_yields_type());
    }

    #[test]
    fn default_type_and_elsewhere() {
        let doc = Docstring::parse(
            Some("Plain text.\nFor the parameters, see other."),
            "google",
        );
        assert!(doc.params_documented_elsewhere() && !doc.has_params());
        assert_eq!(Docstring::parse(None, "default").text(), "");
    }
}
