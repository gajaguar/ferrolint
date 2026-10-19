//! The host's own rules: what the linter reports without any plugin.

use ferrolint_core::{Diagnostic, LineIndex, RuleInfo, Severity};
use ruff_python_ast::ModModule;
use ruff_python_parser::Parsed;

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

/// The host rules' diagnostics for one parsed file.
///
/// `known` tells whether the rule registry resolves a message name, so a
/// plugin's rule named in a pragma is not reported as unknown.
pub fn check(
    source: &str,
    parsed: &Parsed<ModModule>,
    index: &LineIndex,
    known: &dyn Fn(&str) -> bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if let Some(error) = parsed.errors().first() {
        diagnostics.push(Diagnostic {
            code: SYNTAX_ERROR.into(),
            message: error.error.to_string(),
            range: index.range(
                source,
                error.location.start().to_u32(),
                error.location.end().to_u32(),
            ),
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
                    .filter(|name| !known(name))
                    .filter_map(|name| crate::pylint::message_issue(key, name))
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
