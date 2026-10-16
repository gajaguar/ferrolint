use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Severity;

/// A rule a check can report, with its Pylint aliases when it ports one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuleInfo {
    pub code: String,
    pub name: String,
    pub summary: String,
    pub severity: Severity,
    pub pylint_id: Option<String>,
    pub pylint_symbol: Option<String>,
    /// The plugin that owns the rule; `None` for the host's own rules.
    pub plugin: Option<String>,
}

impl RuleInfo {
    /// The Pylint msg-id and symbol, in that order, that resolve to this rule.
    pub fn aliases(&self) -> Vec<String> {
        self.pylint_id
            .iter()
            .chain(self.pylint_symbol.iter())
            .cloned()
            .collect()
    }
}

/// Why a set of rules cannot form one mapping table.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    /// Two rules declare the same code.
    DuplicateCode { code: String },
    /// A spelling (code, msg-id or symbol) points at two different rules.
    AmbiguousSpelling {
        spelling: String,
        first: String,
        second: String,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCode { code } => write!(f, "rule code {code} is declared twice"),
            Self::AmbiguousSpelling {
                spelling,
                first,
                second,
            } => write!(f, "{spelling} names both rule {first} and rule {second}"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// The mapping table: every known rule, by code, and every spelling that
/// resolves to one.
#[derive(Clone, Debug, Default)]
pub struct Registry {
    rules: BTreeMap<String, RuleInfo>,
    spellings: HashMap<String, String>,
}

impl Registry {
    pub fn new(rules: impl IntoIterator<Item = RuleInfo>) -> Result<Self, RegistryError> {
        let mut registry = Self::default();
        for rule in rules {
            if registry.rules.contains_key(&rule.code) {
                return Err(RegistryError::DuplicateCode { code: rule.code });
            }
            for spelling in std::iter::once(rule.code.clone()).chain(rule.aliases()) {
                if let Some(first) = registry.spellings.get(&spelling) {
                    if *first != rule.code {
                        return Err(RegistryError::AmbiguousSpelling {
                            spelling,
                            first: first.clone(),
                            second: rule.code.clone(),
                        });
                    }
                    continue;
                }
                registry.spellings.insert(spelling, rule.code.clone());
            }
            registry.rules.insert(rule.code.clone(), rule);
        }
        Ok(registry)
    }

    /// The rule code a plugin code, msg-id or symbol names.
    pub fn resolve(&self, spelling: &str) -> Option<&str> {
        self.spellings.get(spelling).map(String::as_str)
    }

    pub fn get(&self, code: &str) -> Option<&RuleInfo> {
        self.rules.get(code)
    }

    /// Every rule, ordered by code.
    pub fn rules(&self) -> impl Iterator<Item = &RuleInfo> {
        self.rules.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(code: &str, id: Option<&str>, symbol: Option<&str>) -> RuleInfo {
        RuleInfo {
            code: code.into(),
            name: code.to_lowercase(),
            summary: String::new(),
            severity: Severity::Warning,
            pylint_id: id.map(Into::into),
            pylint_symbol: symbol.map(Into::into),
            plugin: None,
        }
    }

    #[test]
    fn every_spelling_resolves_to_the_code() {
        let registry = Registry::new([rule("X1", Some("W0611"), Some("unused-import"))]).unwrap();
        for spelling in ["X1", "W0611", "unused-import"] {
            assert_eq!(registry.resolve(spelling), Some("X1"));
        }
        assert_eq!(registry.resolve("W0612"), None);
    }

    #[test]
    fn an_alias_claimed_twice_is_an_error() {
        let error = Registry::new([
            rule("X1", Some("W0611"), None),
            rule("X2", Some("W0611"), None),
        ])
        .unwrap_err();
        assert!(matches!(error, RegistryError::AmbiguousSpelling { .. }));
    }

    #[test]
    fn a_code_declared_twice_is_an_error() {
        let error = Registry::new([rule("X1", None, None), rule("X1", None, None)]).unwrap_err();
        assert_eq!(error, RegistryError::DuplicateCode { code: "X1".into() });
    }
}
