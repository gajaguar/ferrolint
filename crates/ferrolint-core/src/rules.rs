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
    /// A rule declares as an alias the code of another rule; no owner can settle it.
    AliasIsCode { alias: String, rule: String },
    /// An owner names a rule that does not declare the alias.
    BadOwner { alias: String, code: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateCode { code } => write!(f, "rule code {code} is declared twice"),
            Self::AmbiguousSpelling {
                spelling,
                first,
                second,
            } => write!(
                f,
                "{spelling} names both rule {first} and rule {second}; pick one in pylint-aliases"
            ),
            Self::AliasIsCode { alias, rule } => write!(
                f,
                "rule {rule} declares {alias} as an alias, but {alias} is the code of another rule"
            ),
            Self::BadOwner { alias, code } => {
                write!(
                    f,
                    "pylint-aliases gives {alias} to {code}, which does not declare it"
                )
            }
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
        Self::with_owners(rules, &BTreeMap::new())
    }

    /// The table with `owners` deciding which rule a Pylint alias claimed
    /// by several rules belongs to: alias to rule code.
    pub fn with_owners(
        rules: impl IntoIterator<Item = RuleInfo>,
        owners: &BTreeMap<String, String>,
    ) -> Result<Self, RegistryError> {
        let mut registry = Self::default();
        for rule in rules {
            if registry.rules.contains_key(&rule.code) {
                return Err(RegistryError::DuplicateCode { code: rule.code });
            }
            registry.rules.insert(rule.code.clone(), rule);
        }
        for (alias, code) in owners {
            let declares = registry
                .rules
                .get(code)
                .is_some_and(|rule| rule.aliases().contains(alias));
            if !declares {
                return Err(RegistryError::BadOwner {
                    alias: alias.clone(),
                    code: code.clone(),
                });
            }
        }
        for rule in registry.rules.values() {
            registry
                .spellings
                .insert(rule.code.clone(), rule.code.clone());
        }
        for rule in registry.rules.values() {
            for alias in rule.aliases() {
                // A rule's own code is never an alias of another rule.
                if registry.rules.contains_key(&alias) && alias != rule.code {
                    return Err(RegistryError::AliasIsCode {
                        alias,
                        rule: rule.code.clone(),
                    });
                }
                if let Some(owner) = owners.get(&alias) {
                    registry.spellings.insert(alias, owner.clone());
                    continue;
                }
                match registry.spellings.get(&alias) {
                    Some(first) if *first != rule.code => {
                        return Err(RegistryError::AmbiguousSpelling {
                            spelling: alias,
                            first: first.clone(),
                            second: rule.code.clone(),
                        });
                    }
                    _ => {
                        registry.spellings.insert(alias, rule.code.clone());
                    }
                }
            }
        }
        Ok(registry)
    }

    /// The Pylint aliases that resolve to `code`, msg-id first.
    pub fn aliases(&self, code: &str) -> Vec<String> {
        self.rules
            .get(code)
            .map(RuleInfo::aliases)
            .unwrap_or_default()
            .into_iter()
            .filter(|alias| self.resolve(alias) == Some(code))
            .collect()
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
    fn owners_settle_a_shared_alias() {
        let owners = BTreeMap::from([("W0611".to_owned(), "X2".to_owned())]);
        let registry = Registry::with_owners(
            [
                rule("X1", Some("W0611"), Some("unused-import")),
                rule("X2", Some("W0611"), None),
            ],
            &owners,
        )
        .unwrap();
        assert_eq!(registry.resolve("W0611"), Some("X2"));
        assert_eq!(registry.aliases("X1"), ["unused-import"]);
        assert_eq!(registry.aliases("X2"), ["W0611"]);
        let bad = BTreeMap::from([("W0612".to_owned(), "X1".to_owned())]);
        assert!(Registry::with_owners([rule("X1", Some("W0611"), None)], &bad).is_err());
    }

    #[test]
    fn an_owner_cannot_take_another_rules_code() {
        let owners = BTreeMap::from([("W0611".to_owned(), "X2".to_owned())]);
        let rules = [rule("W0611", None, None), rule("X2", Some("W0611"), None)];
        assert!(Registry::with_owners(rules, &owners).is_err());
    }

    #[test]
    fn an_alias_equal_to_another_rules_code_is_an_error() {
        let error = Registry::new([rule("W9006", None, None), rule("X2", Some("W9006"), None)])
            .unwrap_err();
        assert_eq!(
            error,
            RegistryError::AliasIsCode {
                alias: "W9006".into(),
                rule: "X2".into()
            }
        );
    }

    #[test]
    fn a_code_declared_twice_is_an_error() {
        let error = Registry::new([rule("X1", None, None), rule("X1", None, None)]).unwrap_err();
        assert_eq!(error, RegistryError::DuplicateCode { code: "X1".into() });
    }
}
