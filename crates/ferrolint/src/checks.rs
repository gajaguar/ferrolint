//! The host's own rules: what the linter reports without any plugin.

use ferrolint_core::{Diagnostic, LineIndex, RuleInfo, Severity};
use ruff_python_ast::ModModule;
use ruff_python_parser::Parsed;

use crate::suppress::{self, Noqa};

/// A plugin aborted on a file: it trapped, ran out of fuel or time.
pub const PLUGIN_ABORTED: &str = "FL001";
/// A `# noqa` comment without codes.
pub const BARE_NOQA: &str = "FL002";
/// The file does not parse.
pub const SYNTAX_ERROR: &str = "FL003";

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
    vec![
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
    ]
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
pub fn check(source: &str, parsed: &Parsed<ModModule>, index: &LineIndex) -> Vec<Diagnostic> {
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
    for comment in suppress::comments(source, parsed.tokens()) {
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
