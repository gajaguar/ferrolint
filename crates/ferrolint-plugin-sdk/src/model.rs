//! What a plugin declares and reports, with builders.

use crate::bindings::ferrolint::plugin::types as raw;
use crate::{Range, Severity};

/// A plugin's name, version and rules.
#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub version: String,
    pub rules: Vec<Rule>,
}

impl Info {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            rules: Vec::new(),
        }
    }

    /// Add a rule.
    #[must_use]
    pub fn rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    pub(crate) fn into_raw(self) -> raw::PluginInfo {
        raw::PluginInfo {
            name: self.name,
            version: self.version,
            rules: self.rules.into_iter().map(Rule::into_raw).collect(),
        }
    }
}

/// A rule a plugin reports, with the Pylint msg-id and symbol it ports.
#[derive(Clone, Debug)]
pub struct Rule {
    pub code: String,
    pub name: String,
    pub summary: String,
    pub severity: Severity,
    pub pylint_id: Option<String>,
    pub pylint_symbol: Option<String>,
}

impl Rule {
    /// A warning rule with no Pylint aliases.
    pub fn new(
        code: impl Into<String>,
        name: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            summary: summary.into(),
            severity: Severity::Warning,
            pylint_id: None,
            pylint_symbol: None,
        }
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    /// The Pylint msg-id (`W0611`) and symbol (`unused-import`) this rule
    /// ports; `select`, `ignore` and suppressions accept either.
    #[must_use]
    pub fn pylint(mut self, msg_id: impl Into<String>, symbol: impl Into<String>) -> Self {
        self.pylint_id = Some(msg_id.into());
        self.pylint_symbol = Some(symbol.into());
        self
    }

    fn into_raw(self) -> raw::Rule {
        raw::Rule {
            code: self.code,
            name: self.name,
            summary: self.summary,
            severity: self.severity,
            pylint_id: self.pylint_id,
            pylint_symbol: self.pylint_symbol,
        }
    }
}

/// A suggested edit: replace `range` with `replacement`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fix {
    pub description: String,
    pub replacement: String,
    pub range: Range,
}

impl Fix {
    pub fn replace(
        range: Range,
        replacement: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            description: description.into(),
            replacement: replacement.into(),
            range,
        }
    }

    pub fn delete(range: Range, description: impl Into<String>) -> Self {
        Self::replace(range, "", description)
    }
}

/// A diagnostic to report. The severity defaults to the rule's own when
/// left unset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    pub range: Range,
    pub severity: Option<Severity>,
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(code: impl Into<String>, message: impl Into<String>, range: Range) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            range,
            severity: None,
            fix: None,
        }
    }

    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }

    #[must_use]
    pub fn fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    pub(crate) fn into_raw(self) -> raw::Diagnostic {
        raw::Diagnostic {
            code: self.code,
            message: self.message,
            range: self.range,
            severity: self.severity.unwrap_or(Severity::Warning),
            fix: self.fix.map(|fix| raw::Fix {
                description: fix.description,
                replacement: fix.replacement,
                range: fix.range,
            }),
        }
    }
}
