//! String formatting: `C0209` consider-using-f-string, `W1300`
//! bad-format-string-key, `W1307` invalid-format-index, `W1308`
//! duplicate-string-formatting-argument and `E0119` misplaced-format-function.
//! `W1402` anomalous-unicode-escape-in-string reads the string tokens.
//! `W1306` missing-format-attribute needs the attributes of an inferred
//! type, which the host does not have, so it is not ported.

use std::collections::{BTreeSet, HashMap};

use ferrolint_core::{Diagnostic, RuleInfo};
use ruff_python_ast::token::TokenKind;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprBinOp, ExprCall, HasNodeIndex, Number, Operator, Stmt};
use ruff_text_size::{Ranged, TextRange, TextSize};

use super::formats::{Key, parse_method, parse_percent, repr};
use super::{Checker, Ctx, rule};
use crate::wit::BindingKind;

const F_STRING: &str = "C0209";
const KEY: &str = "W1300";
const INDEX: &str = "W1307";
const DUPLICATE: &str = "W1308";
const MISPLACED: &str = "E0119";
const UNICODE_ESCAPE: &str = "W1402";

pub struct Strings;

impl Checker for Strings {
    fn rules(&self) -> Vec<RuleInfo> {
        vec![
            rule(
                F_STRING,
                "consider-using-f-string",
                "A regular string formatted with `format()` or `%` that could be an f-string.",
            ),
            rule(
                KEY,
                "bad-format-string-key",
                "A `%` format with named fields given a dict literal with a key that is not a string.",
            ),
            rule(
                INDEX,
                "invalid-format-index",
                "A `str.format` field indexes an argument that is a number, boolean or `None` literal.",
            ),
            rule(
                DUPLICATE,
                "duplicate-string-formatting-argument",
                "A `str.format` call that passes the same name more than once.",
            ),
            rule(
                UNICODE_ESCAPE,
                "anomalous-unicode-escape-in-string",
                "A `\\u`, `\\U` or `\\N` escape in a byte string, where it has no effect.",
            ),
            rule(
                MISPLACED,
                "misplaced-format-function",
                "`.format` called on the result of `print()`.",
            ),
        ]
    }

    fn check(&self, ctx: &Ctx<'_>, enabled: &BTreeSet<String>, out: &mut Vec<Diagnostic>) {
        if enabled.contains(UNICODE_ESCAPE) {
            unicode_escapes(ctx, out);
        }
        let mut sizes = Sizes(HashMap::new());
        sizes.visit_body(&ctx.parsed.syntax().body);
        let mut walk = Walk {
            ctx,
            enabled,
            out,
            sizes: sizes.0,
        };
        walk.visit_body(&ctx.parsed.syntax().body);
    }
}

/// The `\u`, `\U` and `\N` escapes of the non-raw byte strings.
fn unicode_escapes(ctx: &Ctx<'_>, out: &mut Vec<Diagnostic>) {
    for token in ctx
        .parsed
        .tokens()
        .iter()
        .filter(|token| token.kind() == TokenKind::String)
    {
        let text = &ctx.source[token.range()];
        let Some(quote) = text.find(['\'', '"']) else {
            continue;
        };
        let prefix = text[..quote].to_ascii_lowercase();
        if !prefix.contains('b') || prefix.contains('r') {
            continue;
        }
        let rest = &text[quote..];
        let quotes = if rest.len() >= 6
            && rest[..3] == rest[rest.len() - 3..]
            && rest[..1].repeat(3) == rest[..3]
        {
            3
        } else {
            1
        };
        let body = &rest[quotes..rest.len() - quotes];
        let base = token.start().to_usize() + quote + quotes;
        let mut at = 0;
        while let Some(found) = body[at..].find('\\') {
            let slash = at + found;
            let escape = body[slash..].chars().take(2).collect::<String>();
            if matches!(escape.as_bytes().get(1), Some(b'u' | b'U' | b'N')) {
                let start = TextSize::try_from(base + slash).unwrap_or_default();
                out.push(ctx.report_at(
                    UNICODE_ESCAPE,
                    TextRange::at(start, TextSize::from(2)),
                    format!(
                        "Anomalous Unicode escape in byte string: '{escape}'. String constant might be missing an r or u prefix."
                    ),
                ));
            }
            // a backslash consumes the character after it
            at = slash + 2;
        }
    }
}

/// The kind and length of each `name = {...}` or `name = [...]`, by the
/// node id of the name.
struct Sizes(HashMap<u32, (bool, usize)>);

impl<'a> Visitor<'a> for Sizes {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if let Stmt::Assign(assign) = stmt
            && let [Expr::Name(target)] = assign.targets.as_slice()
            && let Some(id) = target.node_index().load().as_u32()
        {
            match &*assign.value {
                Expr::Dict(dict) => self.0.insert(id, (true, dict.items.len())),
                Expr::List(list) => self.0.insert(id, (false, list.elts.len())),
                _ => None,
            };
        }
        visitor::walk_stmt(self, stmt);
    }
}

struct Walk<'a, 'c> {
    ctx: &'a Ctx<'c>,
    enabled: &'a BTreeSet<String>,
    out: &'a mut Vec<Diagnostic>,
    sizes: HashMap<u32, (bool, usize)>,
}

fn string_of(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::StringLiteral(string) => Some(string.value.to_str()),
        _ => None,
    }
}

/// The text of a `Const` key as Python prints it, for a key that is not a
/// string.
fn constant(expr: &Expr) -> Option<String> {
    Some(match expr {
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(int) => int.to_string(),
            Number::Float(float) => format!("{float:?}"),
            // Python prints `1j`, where Rust's debug form is `1.0j`
            Number::Complex { imag, .. } => format!("{imag}j"),
        },
        Expr::BooleanLiteral(boolean) => if boolean.value { "True" } else { "False" }.into(),
        Expr::NoneLiteral(_) => "None".into(),
        Expr::EllipsisLiteral(_) => "Ellipsis".into(),
        Expr::BytesLiteral(bytes) => {
            let body: String = bytes
                .value
                .iter()
                .flat_map(|part| part.value.iter().copied())
                .map(|b| match b {
                    b'\\' => "\\\\".into(),
                    b' '..=b'~' if b != b'\'' => char::from(b).to_string(),
                    _ => format!("\\x{b:02x}"),
                })
                .collect();
            format!("b'{body}'")
        }
        _ => return None,
    })
}

/// Whether `Expr.as_string()` would hold a backslash: a string or bytes
/// literal that `repr` escapes, or an f-string with a backslash in its source.
struct Escapes<'s>(&'s str, bool);

impl Visitor<'_> for Escapes<'_> {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::StringLiteral(string) => {
                let text = string.value.to_str();
                self.1 |= text.contains('\\')
                    || text.chars().any(char::is_control)
                    || (text.contains('\'') && text.contains('"'));
            }
            Expr::BytesLiteral(bytes) => {
                self.1 |= bytes.value.iter().any(|part| {
                    part.value
                        .iter()
                        .any(|b| !(b' '..=b'~').contains(b) || *b == b'\\')
                });
            }
            Expr::FString(_) => self.1 |= self.0[expr.range()].contains('\\'),
            _ => visitor::walk_expr(self, expr),
        }
    }
}

impl Walk<'_, '_> {
    /// Whether `expr` is a dict (`true`) or a list of known length: one written
    /// in place, or a name assigned only once to one. shortcut: no other
    /// value is followed, where Pylint infers any.
    fn container(&self, expr: &Expr) -> Option<(bool, usize)> {
        match expr {
            Expr::Dict(dict) => Some((true, dict.items.len())),
            Expr::List(list) => Some((false, list.elts.len())),
            Expr::Name(name) => {
                let binding = self
                    .ctx
                    .model
                    .resolve(self.ctx.flat, name.node_index().load().as_u32()?)?;
                (binding.kind == BindingKind::Assignment
                    && self.ctx.model.is_sole_binding(binding.id))
                .then_some(())?;
                self.sizes.get(&binding.node?).copied()
            }
            _ => None,
        }
    }

    fn on(&self, id: &str) -> bool {
        self.enabled.contains(id)
    }

    fn needs_escape(&self, expr: &Expr) -> bool {
        let mut escapes = Escapes(self.ctx.source, false);
        escapes.visit_expr(expr);
        escapes.1
    }

    /// `"...".format(...)` that an f-string would replace.
    fn format_to_f_string(&mut self, call: &ExprCall, text: &str, string: &Expr) {
        let Ok((fields, ..)) = parse_method(text) else {
            return;
        };
        let arguments = &call.arguments;
        // with positional arguments the keywords are not looked at
        if arguments.args.is_empty() {
            for keyword in &arguments.keywords {
                let repeated = keyword.arg.as_ref().is_some_and(|name| {
                    fields
                        .iter()
                        .filter(|(key, _)| *key == Key::Name(name.to_string()))
                        .count()
                        > 1
                });
                let big_dict = self
                    .container(&keyword.value)
                    .is_some_and(|(dict, len)| dict && len > 1)
                    && fields.len() > 1;
                if repeated || big_dict {
                    return;
                }
            }
        }
        for arg in &arguments.args {
            if let Expr::Starred(starred) = arg
                && self
                    .container(&starred.value)
                    .is_some_and(|(dict, len)| !dict && len > 1)
            {
                return;
            }
            if self.needs_escape(arg) {
                return;
            }
        }
        self.report(
            F_STRING,
            string,
            "Formatting a regular string which could be an f-string",
        );
    }

    fn report(&mut self, id: &str, node: &impl Ranged, message: &str) {
        if self.on(id) {
            self.out.push(self.ctx.report(id, node, message));
        }
    }

    /// `"..." % right`.
    fn percent(&mut self, modulo: &ExprBinOp, text: &str) {
        let (left, right) = (&*modulo.left, &*modulo.right);
        if self.ctx.py_version.is_none_or(|version| version >= (3, 6))
            && !self.needs_escape(right)
            && !text.contains(['{', '}'])
            && self.container(right).is_none_or(|(_, len)| len <= 1)
        {
            let message = "Formatting a regular string which could be an f-string";
            self.report(F_STRING, left, message);
            // a string on the right is the other `Const` child of the `%`
            if string_of(right).is_some() {
                self.report(F_STRING, right, message);
            }
        }
        if let (Ok((true, 0)), Expr::Dict(dict)) = (parse_percent(text), right) {
            for key in dict.items.iter().filter_map(|item| item.key.as_ref()) {
                if let Some(shown) = constant(key) {
                    // Pylint passes `None` as "no arguments", so `%s` stays as written
                    let shown = if matches!(key, Expr::NoneLiteral(_)) {
                        "%s".into()
                    } else {
                        shown
                    };
                    self.report(
                        KEY,
                        modulo,
                        &format!("Format string dictionary key should be a string, not {shown}"),
                    );
                }
            }
        }
    }

    fn str_format(&mut self, call: &ExprCall, text: &str) {
        let arguments = &call.arguments;
        let starred = arguments.args.iter().any(Expr::is_starred_expr)
            || arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none());
        let Ok((fields, implicit, explicit)) = parse_method(text) else {
            return;
        };
        if starred || implicit > 0 && explicit > 0 {
            return;
        }
        let names: BTreeSet<&str> = fields
            .iter()
            .filter_map(|(key, _)| match key {
                Key::Name(name) => Some(name.as_str()),
                Key::Index(_) => None,
            })
            .collect();
        let mut required = implicit + usize::from(names.contains(""));
        let check = if names.is_empty() {
            true
        } else {
            if required == 0 {
                required = explicit;
            }
            (!arguments.args.is_empty() || required > 0)
                && (!arguments.keywords.is_empty() || names.contains(""))
        };
        if check {
            if required == 0 {
                required = explicit;
            }
            if required == 0 {
                return;
            }
        }
        let mut seen: Vec<(&str, usize)> = Vec::new();
        for arg in &arguments.args {
            if let Expr::Name(name) = arg {
                match seen
                    .iter_mut()
                    .find(|(known, _)| *known == name.id.as_str())
                {
                    Some((_, count)) => *count += 1,
                    None => seen.push((name.id.as_str(), 1)),
                }
            }
        }
        for (name, _) in seen.iter().filter(|(_, count)| *count > 1) {
            self.report(
                DUPLICATE,
                call,
                &format!(
                    "Duplicate string formatting argument {}, consider passing as named argument",
                    repr(name)
                ),
            );
        }
        self.invalid_index(call, &fields);
    }

    /// shortcut: only an argument written in place as a number, boolean or
    /// `None` is seen, since no other type is known not to be subscriptable
    fn invalid_index(&mut self, call: &ExprCall, fields: &[super::formats::Field]) {
        let arguments = &call.arguments;
        for (key, accessors) in fields {
            let argument = match key {
                Key::Index(index) => arguments.args.get(*index),
                Key::Name(name) if name.is_empty() => arguments.args.first(),
                Key::Name(name) => arguments
                    .keywords
                    .iter()
                    .find(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
                    .map(|keyword| &keyword.value),
            };
            let (Some(argument), Some((false, item))) = (argument, accessors.first()) else {
                continue;
            };
            let scalar = matches!(
                argument,
                Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::EllipsisLiteral(_)
            );
            if scalar {
                // `str.format` reads a run of digits as an integer index
                let (shown, path) = match item.parse::<u64>() {
                    Ok(number) if item.chars().all(|c| c.is_ascii_digit()) => {
                        (number.to_string(), format!("[{number}]"))
                    }
                    _ => (repr(item), format!("[{}]", repr(item))),
                };
                let head = match key {
                    Key::Index(index) => index.to_string(),
                    Key::Name(name) if name.is_empty() => "0".into(),
                    Key::Name(name) => name.clone(),
                };
                self.report(
                    INDEX,
                    call,
                    &format!(
                        "Using invalid lookup key {shown} in format specifier {}",
                        repr(&format!("{head}{path}"))
                    ),
                );
            }
        }
    }

    fn is_builtin_print(&self, expr: &Expr) -> bool {
        let Expr::Call(call) = expr else {
            return false;
        };
        let Expr::Name(name) = &*call.func else {
            return false;
        };
        name.id.as_str() == "print"
            && name
                .node_index()
                .load()
                .as_u32()
                .and_then(|id| self.ctx.model.resolve(self.ctx.flat, id))
                .is_some_and(|binding| binding.kind == BindingKind::Builtin)
    }
}

impl<'a> Visitor<'a> for Walk<'_, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(call) => {
                if let Expr::Attribute(attribute) = &*call.func
                    && attribute.attr.as_str() == "format"
                {
                    if self.is_builtin_print(&attribute.value) {
                        self.report(MISPLACED, call, "format function is not called on str");
                    }
                    if let Some(text) = string_of(&attribute.value) {
                        self.str_format(call, text);
                    }
                }
                // Pylint takes the call around the attribute, so `f("{}".format)` counts
                let candidates = std::iter::once(&*call.func).chain(&call.arguments.args);
                for candidate in candidates {
                    if let Expr::Attribute(attribute) = candidate
                        && attribute.attr.as_str() == "format"
                        && let Some(text) = string_of(&attribute.value)
                        && self.ctx.py_version.is_none_or(|version| version >= (3, 6))
                    {
                        self.format_to_f_string(call, text, &attribute.value);
                    }
                }
            }
            Expr::BinOp(modulo) if modulo.op == Operator::Mod => {
                if let Some(text) = string_of(&modulo.left) {
                    self.percent(modulo, text);
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use crate::native::lint;

    /// What Pylint reports on `source`, for the rules of this module.
    fn found(source: &str, id: &str) -> Vec<u32> {
        lint(source)
            .into_iter()
            .filter(|(_, code)| code == id)
            .map(|(line, _)| line)
            .collect()
    }

    // The expected lines of every test below are Pylint's own output on the
    // same text.
    #[test]
    fn f_string_candidates() {
        let source = r#"a = 1
b = 2
"{} {}".format(a, b)
"%s %s" % (a, b)
"{x}".format(x=a)
"{x} {x}".format(x=a)
"{}".format("\n".join([]))
"%s" % {"a": 1, "b": 2}
"{{}} {}".format(a)
"{} %s".format
"x" % "\\"
"{a}{b}".format(**{"a": 1, "b": 2})
"%s {}" % a
"{".format(a)
"{}".format(*[a, b])
"{}".format()
"{a}".format(**{"a": 1})
"abc %s" % [a, b]
"{}".format(*a)
"{} {y}".format(a, y=b)
"{}".format('it\'s "x"')
("a"
 "{}").format(a)
"%s" % ("a\
b")
x = "%d" % 1 if a else "%s" % b
f("client_{}".format)
g(1, "{}".format, 2)
g(x="{}".format)
"#;
        let expected = [3, 4, 5, 9, 16, 17, 19, 20, 22, 24, 24, 26, 26, 27, 28];
        assert_eq!(found(source, "C0209"), expected);
    }

    #[test]
    fn f_string_candidates_held_by_names() {
        let source = "\
params = {'a': 1, 'b': 2}
one = {'a': 1}
many = [1, 2]
twice = {'a': 1, 'b': 2}
twice = {'a': 1}
'DROP %(a)s' % params
'DROP %(a)s' % one
'DROP %s' % many
'DROP %s' % twice
'{a}{b}'.format(**params)
'{a}'.format(**one)
'{} {}'.format(*many)


def f():
    return 'x %(a)s' % params
";
        assert_eq!(found(source, "C0209"), [7, 9, 11]);
    }

    #[test]
    fn non_string_format_key() {
        let source = "\
a = '%(x)s' % {1: 2}
b = '%(x)s' % {'x': 1, None: 2, 3.5: 4}
c = '%(x)s %s' % {1: 2}
d = '%s' % {1: 2}
e = '%(x)s' % {True: 1, b'k': 2, 0x10: 3, 1j: 4, ...: 5}
f = '%(x)s' % {1: 2, **a}
g = '%(x)s' % {-1: 2}
h = '%(x)y' % {1: 2}
i = '%(x)s' % dict
";
        assert_eq!(found(source, "W1300"), [1, 2, 2, 5, 5, 5, 5, 5, 6]);
    }

    #[test]
    fn duplicate_argument_and_scalar_index() {
        let source = "\
a = b = 1
'{} {}'.format(a, a)
'{0} {1} {0}'.format(a, b)
'{x} {y}'.format(a, a, x=b)
'abc'.format(a, a)
'{0[1]}'.format(5)
'{[x]}'.format(None)
'{n[0]}'.format(n=2.5)
'{0[1]}'.format('ab')
'{0[1]}{1[2]}'.format(True, ...)
'{0} {0}'.format(a, a)
'{} {x}'.format(a, a, x=b)
'{:{}}'.format(a, a)
'{}{}'.format(a, a, b)
'{0} {}'.format(a, a)
'{'.format(a, a)
'{0[-1]}'.format(5)
";
        assert_eq!(found(source, "W1308"), [2, 11, 12, 13, 14]);
        assert_eq!(found(source, "W1307"), [6, 7, 8, 10, 10, 17]);
    }

    #[test]
    fn unicode_escape_in_bytes() {
        let source = r#"a = b'\u1234'
b = b"\N{DASH}\U0001F600"
c = rb'\u1234'
d = b'\\u1234'
e = '\u1234'
f = B"""x
\u00e9 \\ \u"""
g = (b'ok'
     b'\Uabc')
h = Rb'\u'
i = b'\x41\n\u'
"#;
        assert_eq!(found(source, "W1402"), [1, 2, 2, 7, 7, 9, 11]);
    }

    #[test]
    fn format_on_print() {
        let source = "\
print('{}').format(1)
def f(print=print):
    print('{}').format(1)
x = g().format(1)
";
        assert_eq!(found(source, "E0119"), [1]);
    }
}
