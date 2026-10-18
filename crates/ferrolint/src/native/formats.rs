//! Python's two format-string grammars (`%` and `str.format`), parsed as
//! Pylint's `checkers/utils.py` does, plus the `repr` the messages quote.

use std::fmt::Write;

/// A format string that ends in the middle of a field or specifier.
#[derive(Debug, PartialEq)]
pub struct Incomplete;

#[derive(Debug, PartialEq)]
pub enum PercentError {
    Incomplete,
    /// The index (in characters) of the unsupported conversion character.
    Unsupported(usize),
}

/// The first part of a `str.format` field name: `0` or `name`.
#[derive(Debug, Eq, PartialEq)]
pub enum Key {
    Index(usize),
    Name(String),
}

/// An attribute (`true`) or item (`false`) accessor and its text.
pub type Accessor = (bool, String);
pub type Field = (Key, Vec<Accessor>);

fn next(chars: &[char], at: usize) -> Result<usize, PercentError> {
    if at + 1 == chars.len() {
        Err(PercentError::Incomplete)
    } else {
        Ok(at + 1)
    }
}

/// Whether a `%` format string names its arguments, and how many positional
/// arguments it takes (`parse_format_string`).
pub fn parse_percent(text: &str) -> Result<(bool, usize), PercentError> {
    let chars: Vec<char> = text.chars().collect();
    let (mut named, mut args, mut at) = (false, 0, 0);
    while at < chars.len() {
        if chars[at] == '%' {
            at = next(&chars, at)?;
            let mut key = false;
            if chars[at] == '(' {
                let mut depth = 1;
                at = next(&chars, at)?;
                let start = at;
                while depth != 0 {
                    match chars[at] {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    at = next(&chars, at)?;
                }
                key = at - 1 > start;
            }
            while "#0- +".contains(chars[at]) {
                at = next(&chars, at)?;
            }
            if chars[at] == '*' {
                args += 1;
                at = next(&chars, at)?;
            } else {
                while chars[at].is_ascii_digit() {
                    at = next(&chars, at)?;
                }
            }
            if chars[at] == '.' {
                at = next(&chars, at)?;
                if chars[at] == '*' {
                    args += 1;
                    at = next(&chars, at)?;
                } else {
                    while chars[at].is_ascii_digit() {
                        at = next(&chars, at)?;
                    }
                }
            }
            if "hlL".contains(chars[at]) {
                at = next(&chars, at)?;
            }
            // cspell:disable-next-line
            if !"diouxXeEfFgGcrs%a".contains(chars[at]) {
                return Err(PercentError::Unsupported(at));
            }
            if key {
                named = true;
            } else if chars[at] != '%' {
                args += 1;
            }
        }
        at += 1;
    }
    Ok((named, args))
}

/// The `(name, spec)` of each replacement field, split as
/// `Formatter.parse` does.
fn replacement_fields(chars: &[char]) -> Result<Vec<(String, String)>, Incomplete> {
    let (mut at, mut fields) = (0, Vec::new());
    while at < chars.len() {
        let (mut brace, mut last) = (false, '\0');
        while at < chars.len() {
            last = chars[at];
            at += 1;
            if last == '{' || last == '}' {
                brace = true;
                break;
            }
        }
        let at_end = at >= chars.len();
        if brace && (last == '}' && (at_end || chars[at] != '}') || last == '{' && at_end) {
            return Err(Incomplete);
        }
        if brace && chars[at] == last {
            at += 1; // an escaped brace is literal text
            continue;
        }
        if !brace {
            continue;
        }
        let start = at;
        let mut stop = None;
        while at < chars.len() {
            let c = chars[at];
            at += 1;
            match c {
                '{' => return Err(Incomplete),
                '[' => {
                    while at < chars.len() && chars[at] != ']' {
                        at += 1;
                    }
                }
                '}' | ':' | '!' => {
                    stop = Some(c);
                    break;
                }
                _ => {}
            }
        }
        let name: String = chars[start..at.saturating_sub(1).max(start)]
            .iter()
            .collect();
        let mut spec_start = at;
        match stop {
            None => return Err(Incomplete),
            Some('}') => {
                fields.push((name, String::new()));
                continue;
            }
            Some(stop) => {
                if stop == '!' {
                    if at >= chars.len() {
                        return Err(Incomplete);
                    }
                    at += 1; // the conversion character
                    if at < chars.len() {
                        let c = chars[at];
                        at += 1;
                        if c == '}' {
                            fields.push((name, String::new()));
                            continue;
                        }
                        if c != ':' {
                            return Err(Incomplete);
                        }
                    }
                    spec_start = at;
                }
            }
        }
        let mut depth = 1;
        let mut closed = false;
        while at < chars.len() {
            let c = chars[at];
            at += 1;
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        if !closed {
            return Err(Incomplete);
        }
        fields.push((name, chars[spec_start..at - 1].iter().collect()));
    }
    Ok(fields)
}

/// Every field name of a format string, nested specs included.
fn field_names(text: &str, out: &mut Vec<String>) -> Result<(), Incomplete> {
    let chars: Vec<char> = text.chars().collect();
    for (name, spec) in replacement_fields(&chars)? {
        out.push(name);
        if !spec.is_empty() {
            field_names(&spec, out)?;
        }
    }
    Ok(())
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// `_string.formatter_field_name_split`: the first part and the accessors.
fn split_name(name: &str) -> Result<Field, Incomplete> {
    let chars: Vec<char> = name.chars().collect();
    let mut at = chars
        .iter()
        .position(|&c| c == '.' || c == '[')
        .unwrap_or(chars.len());
    let first: String = chars[..at].iter().collect();
    let key = match first.parse::<usize>() {
        Ok(index) if digits(&first) => Key::Index(index),
        _ => Key::Name(first),
    };
    let mut accessors = Vec::new();
    while at < chars.len() {
        let attribute = match chars[at] {
            '.' => true,
            '[' => false,
            _ => return Err(Incomplete),
        };
        at += 1;
        let start = at;
        while at < chars.len()
            && (if attribute {
                chars[at] != '.' && chars[at] != '['
            } else {
                chars[at] != ']'
            })
        {
            at += 1;
        }
        if !attribute {
            if at >= chars.len() {
                return Err(Incomplete);
            }
            at += 1; // the closing bracket
        }
        let text: String = chars[start..if attribute { at } else { at - 1 }]
            .iter()
            .collect();
        if text.is_empty() {
            return Err(Incomplete);
        }
        accessors.push((attribute, text));
    }
    Ok((key, accessors))
}

/// The fields with a name that is not a plain number, the count of automatic
/// fields and the count of distinct explicit numbers
/// (`parse_format_method_string`).
pub fn parse_method(text: &str) -> Result<(Vec<Field>, usize, usize), Incomplete> {
    let mut names = Vec::new();
    field_names(text, &mut names)?;
    let (mut fields, mut implicit, mut explicit) =
        (Vec::new(), 0, std::collections::BTreeSet::new());
    for name in names {
        if digits(&name) {
            explicit.insert(name);
        } else if name.is_empty() {
            implicit += 1;
        } else {
            let (key, accessors) = split_name(&name)?;
            if let Key::Index(index) = key {
                explicit.insert(index.to_string());
            }
            fields.push((key, accessors));
        }
    }
    Ok((fields, implicit, explicit.len()))
}

/// Python's `repr` of a string.
pub fn repr(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::from(quote);
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => {
                let _ = write!(out, "\\x{:02x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent() {
        assert_eq!(parse_percent("%s %(a)d %%"), Ok((true, 1)));
        assert_eq!(parse_percent("%*.*f"), Ok((false, 3)));
        assert_eq!(parse_percent("%y"), Err(PercentError::Unsupported(1)));
        assert_eq!(parse_percent("100%"), Err(PercentError::Incomplete));
        assert_eq!(parse_percent("%(a"), Err(PercentError::Incomplete));
    }

    #[test]
    fn method() {
        let (fields, implicit, explicit) = parse_method("{} {0.a} {b[1]} {:{}}").unwrap();
        assert_eq!((implicit, explicit), (3, 1));
        assert_eq!(fields[0], (Key::Index(0), vec![(true, "a".into())]));
        assert_eq!(
            fields[1],
            (Key::Name("b".into()), vec![(false, "1".into())])
        );
        for bad in [
            "{", "}", "{a", "{a.}", "{a[0}", "{!", "{:{", "{a[0]b}", "{a{}",
        ] {
            assert_eq!(parse_method(bad), Err(Incomplete), "{bad}");
        }
        assert!(parse_method("{{}} {!r} {:>{}}").is_ok());
    }
}
