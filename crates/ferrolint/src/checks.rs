//! The host's own rules: what the linter reports without any plugin.

use ferrolint_core::{Diagnostic, LineIndex, RuleInfo, Severity};
use ruff_python_ast::ModModule;
use ruff_python_parser::{ParseErrorType, ParseOptions, Parsed};

use crate::native;
use crate::suppress::{self, Noqa};

/// A plugin aborted on a file: it trapped, ran out of fuel or time.
pub const PLUGIN_ABORTED: &str = "FL001";
/// A `# noqa` comment without codes.
pub const BARE_NOQA: &str = "FL002";
/// The file does not parse.
pub const SYNTAX_ERROR: &str = "FL003";

/// Pylint's informational pragma messages, as msg-id and symbol. Pylint
/// leaves them off by default: they report only when `select` or a Pylint
/// `enable` names them.
pub const PRAGMA_MESSAGES: &[(&str, &str)] = &[
    ("I0010", "bad-inline-option"),
    ("I0011", "locally-disabled"),
    ("I0013", "file-ignored"),
    ("I0022", "deprecated-pragma"),
    ("I0023", "use-symbolic-message-instead"),
];

pub fn host_rules() -> Vec<RuleInfo> {
    let rule =
        |code: &str, name: &str, summary: &str, severity, pylint: Option<(&str, &str)>| RuleInfo {
            code: code.into(),
            name: name.into(),
            summary: summary.into(),
            severity,
            pylint_id: pylint.map(|(id, _)| id.into()),
            pylint_symbol: pylint.map(|(_, symbol)| symbol.into()),
            plugin: None,
        };
    let mut rules = vec![
        rule(
            PLUGIN_ABORTED,
            "plugin-aborted",
            "A plugin stopped before finishing a file: it trapped or hit its fuel or time cap.",
            Severity::Error,
            None,
        ),
        rule(
            BARE_NOQA,
            "bare-noqa",
            "A `# noqa` comment names no codes, so it suppresses nothing.",
            Severity::Warning,
            None,
        ),
        rule(
            SYNTAX_ERROR,
            "syntax-error",
            "The file does not parse as Python.",
            Severity::Error,
            Some(("E0001", "syntax-error")),
        ),
    ];
    rules.extend([
        native::rule(
            "I0010",
            "bad-inline-option",
            "A `# pylint:` option is badly formatted: a keyword with no message names or no `=`.",
        ),
        native::rule(
            "I0011",
            "locally-disabled",
            "A `# pylint:` comment disables a message, a category or a checker.",
        ),
        native::rule(
            "I0013",
            "file-ignored",
            "A `# pylint: skip-file`, `disable-all` or `disable=all` comment ends the file for Pylint.",
        ),
        native::rule(
            "I0022",
            "deprecated-pragma",
            "A `# pylint:` comment uses `disable-msg`, `enable-msg`, `disable-all` or `disable=all`.",
        ),
        native::rule(
            "I0023",
            "use-symbolic-message-instead",
            "A `# pylint:` comment enables or disables a message by its id, not its symbol.",
        ),
        native::rule(
            "E0011",
            "unrecognized-inline-option",
            "A `# pylint:` comment sets an option Pylint does not know.",
        ),
        native::rule(
            "W0012",
            "unknown-option-value",
            "A `disable`/`enable` option or `# pylint:` pragma names a message Pylint does not know.",
        ),
        native::rule(
            "E0013",
            "bad-plugin-value",
            "A module in `load-plugins` cannot be found.",
        ),
        native::rule(
            "E0014",
            "bad-configuration-section",
            "Defined by Pylint 4.1.3, which never emits it; ferrolint does not either.",
        ),
        native::rule(
            "E0015",
            "unrecognized-option",
            "A Pylint configuration sets an option Pylint does not know.",
        ),
        native::rule(
            "R0022",
            "useless-option-value",
            "A `disable`/`enable` option or `# pylint:` pragma names a message Pylint removed.",
        ),
    ]);
    rules
}

/// A finding of the Pylint configuration file, at its first line as Pylint
/// reports it.
pub fn config_diagnostic(code: &str, message: &str) -> Diagnostic {
    let start = ferrolint_core::Position {
        offset: 0,
        line: 1,
        column: 1,
    };
    Diagnostic {
        code: code.into(),
        message: message.into(),
        range: ferrolint_core::Range { start, end: start },
        severity: native::rule(code, "", "").severity,
        fix: None,
    }
}

/// A diagnostic for a file that is not UTF-8 and cannot be parsed.
pub fn not_utf8() -> Diagnostic {
    Diagnostic {
        code: SYNTAX_ERROR.into(),
        message: "the file is not valid UTF-8".into(),
        range: ferrolint_core::Range::default(),
        severity: Severity::Error,
        fix: None,
    }
}

/// Parse `source` as a module, tolerating `raise;stmt`, which CPython accepts
/// and Ruff's parser rejects at the `;` (it ends a bare `raise` on a newline
/// only).
///
/// The only errors being that one, the file is parsed again with each such
/// `raise` spelled `pass `: same length, so every offset, line and column is
/// unchanged. Any other error leaves the first parse, and so E0001, as it was.
///
/// shortcut: the tree holds a `Pass` where the source has the bare `raise`, so
/// a rule that reads that statement (W0101 after it) does not see it; upgrade
/// when Ruff's parser accepts the `;`.
pub fn parse(source: &str, options: ParseOptions) -> Parsed<ModModule> {
    let parsed = ruff_python_parser::parse_unchecked(source, options.clone())
        .try_into_module()
        .unwrap_or_else(|| unreachable!("Python source parses to a module"));
    let mut fixed = source.to_owned();
    for error in parsed.errors() {
        let semi = error.location.start().to_usize();
        let raise = semi.wrapping_sub(5);
        let bare = matches!(error.error, ParseErrorType::ExpectedExpression)
            && source.get(semi..=semi) == Some(";")
            && source.get(raise..semi) == Some("raise")
            && !source[..raise].ends_with(|c: char| c.is_alphanumeric() || c == '_');
        if !bare {
            return parsed;
        }
        fixed.replace_range(raise..semi, "pass ");
    }
    if parsed.errors().is_empty() {
        return parsed;
    }
    let again = ruff_python_parser::parse_unchecked(&fixed, options)
        .try_into_module()
        .unwrap_or_else(|| unreachable!("Python source parses to a module"));
    if again.errors().is_empty() {
        again
    } else {
        parsed
    }
}

/// The host rules' diagnostics for one parsed file.
///
/// `known` tells whether the rule registry resolves a message name, so a
/// plugin's rule named in a pragma is not reported as unknown. `plugin_loaded`
/// tells whether `load-plugins` names a Pylint plugin module, whose messages
/// and checkers count as known only while it is loaded.
pub fn check(
    source: &str,
    parsed: &Parsed<ModModule>,
    index: &LineIndex,
    module: &str,
    known: &dyn Fn(&str) -> bool,
    plugin_loaded: &dyn Fn(&str) -> bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(error) = parsed.errors().first() {
        let range = index.range(
            source,
            error.location.start().to_u32(),
            error.location.end().to_u32(),
        );
        // The shape of Pylint's E0001; the parser's text stands in for CPython's.
        let message = format!(
            "Parsing failed: '{} ({module}, line {})'",
            error.error, range.start.line
        );
        diagnostics.push(Diagnostic {
            code: SYNTAX_ERROR.into(),
            message,
            range,
            severity: Severity::Error,
            fix: None,
        });
    }
    let comments = suppress::comments(source, parsed.tokens());
    diagnostics.extend(crate::pragma::informational(source, &comments, index));
    for comment in comments {
        for (key, names) in suppress::pylint_options(comment.text) {
            let issues = if matches!(
                key,
                "disable" | "enable" | "disable-next" | "disable-msg" | "enable-msg"
            ) {
                names
                    .into_iter()
                    // A native port of an extension rule resolves too, yet Pylint
                    // knows its name only while the extension is loaded.
                    .filter(|name| {
                        !known(name)
                            || crate::pylint_names::owner(name).is_some_and(|m| !plugin_loaded(m))
                    })
                    .filter_map(|name| crate::pylint::message_issue(key, name, plugin_loaded))
                    .map(|issue| (issue.code, issue.message))
                    .collect()
            } else {
                vec![("E0011", format!("Unrecognized file option '{key}'"))]
            };
            for (code, message) in issues {
                diagnostics.push(Diagnostic {
                    code: code.into(),
                    message,
                    range: index.range(source, comment.start, comment.end),
                    severity: native::rule(code, "", "").severity,
                    fix: None,
                });
            }
        }
        if suppress::noqa(comment.text) == Some(Noqa::Bare) {
            diagnostics.push(Diagnostic {
                code: BARE_NOQA.into(),
                message: "`# noqa` without codes suppresses nothing; name the codes".into(),
                range: index.range(source, comment.start, comment.end),
                severity: Severity::Warning,
                fix: None,
            });
        }
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use ferrolint_core::LineIndex;
    use ruff_python_ast::PySourceType;
    use ruff_python_parser::{ParseOptions, parse_unchecked};

    use super::{check, parse};

    fn errors(source: &str) -> usize {
        parse(source, ParseOptions::from(PySourceType::Python))
            .errors()
            .len()
    }

    #[test]
    fn a_bare_raise_before_a_semicolon_parses() {
        assert_eq!(errors("try:\n\tpass\nexcept X:raise;x = 1\ny = 2\n"), 0);
        assert_eq!(errors("raise;x = 1\n"), 0);
    }

    #[test]
    fn other_semicolon_errors_stay() {
        // an error elsewhere, a `;` after other tokens, a name that ends in `raise`
        assert!(errors("def f(:\n    raise;x = 1\n") > 0);
        assert!(errors("x = (1;\n") > 0);
        assert!(errors("reraise;\nx = (\n") > 0);
        assert!(errors("raise;;x\n") > 0);
    }

    #[test]
    fn syntax_error_names_the_module_as_pylint_does() {
        let source = "x = 1\ndef f(:\n    pass\n";
        let parsed = parse_unchecked(source, ParseOptions::from(PySourceType::Python))
            .try_into_module()
            .unwrap();
        let index = LineIndex::new(source);
        let diagnostics = check(source, &parsed, &index, "pkg.sub.mod", &|_| true, &|_| {
            false
        });
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "Parsing failed: 'Expected a parameter or the end of the parameter list (pkg.sub.mod, line 2)'"
        );
    }

    #[test]
    fn a_pragma_names_a_plugin_message_only_when_the_plugin_is_loaded() {
        let source = "# pylint: disable=dict-init-mutate,no-self-use,made-up-name\nx = 1\n";
        let parsed = parse_unchecked(source, ParseOptions::from(PySourceType::Python))
            .try_into_module()
            .unwrap();
        let index = LineIndex::new(source);
        let codes = |plugin_loaded: &dyn Fn(&str) -> bool| {
            // The registry resolves every name, as it does for a native port.
            check(source, &parsed, &index, "mod", &|_| true, plugin_loaded)
                .into_iter()
                .map(|d| d.code)
                .collect::<Vec<_>>()
        };
        assert_eq!(codes(&|_| false), ["W0012", "R0022"]);
        let loaded = |module: &str| {
            matches!(
                module,
                "pylint.extensions.dict_init_mutate" | "pylint.extensions.no_self_use"
            )
        };
        assert_eq!(codes(&loaded), Vec::<String>::new());
    }
}
