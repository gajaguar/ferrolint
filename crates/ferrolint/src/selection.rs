//! Which rule codes report for which file: `select`, `ignore` and
//! `per-file-ignores`, each spelling resolved through the registry.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ferrolint_core::Registry;
use globset::{Glob, GlobMatcher};

use crate::checks;
use crate::config::Config;
use crate::discover::display_path;
use crate::error::{Error, Result};
use crate::pylint::Toggle;

pub struct Selection {
    root: PathBuf,
    enabled: BTreeSet<String>,
    per_file: Vec<PerFile>,
}

struct PerFile {
    matcher: GlobMatcher,
    /// Match the pattern against the file name alone: it has no `/`.
    name_only: bool,
    codes: BTreeSet<String>,
}

impl Selection {
    pub fn new(registry: &Registry, config: &Config) -> Result<Self> {
        let mut enabled: BTreeSet<String> = match &config.select {
            Some(select) => resolve_all(registry, "select", select)?,
            None => registry
                .rules()
                .map(|rule| rule.code.clone())
                .filter(|code| !checks::PRAGMA_MESSAGES.iter().any(|(id, _)| id == code))
                .collect(),
        };
        for code in resolve_all(registry, "ignore", &config.ignore)? {
            enabled.remove(&code);
        }
        for (toggle, entry) in &config.pylint_disable {
            let codes = pylint_codes(registry, entry);
            match toggle {
                Toggle::Disable => enabled.retain(|code| !codes.contains(code)),
                Toggle::Enable => enabled.extend(codes),
            }
        }
        let mut per_file = Vec::new();
        for (pattern, codes) in &config.per_file_ignores {
            let glob = Glob::new(pattern).map_err(|err| {
                Error::config(format!("per-file-ignores pattern {pattern:?}: {err}"))
            })?;
            per_file.push(PerFile {
                matcher: glob.compile_matcher(),
                name_only: !pattern.contains('/'),
                codes: resolve_all(registry, "per-file-ignores", codes)?,
            });
        }
        Ok(Self {
            root: config.root.clone(),
            enabled,
            per_file,
        })
    }

    /// Whether `code` reports outside any file, as the configuration's own
    /// findings do.
    pub fn is_enabled(&self, code: &str) -> bool {
        self.enabled.contains(code)
    }

    /// The codes that report for the file at `path`.
    pub fn for_file(&self, path: &Path) -> BTreeSet<String> {
        let relative = display_path(&self.root, path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let mut codes = self.enabled.clone();
        for entry in &self.per_file {
            let subject = if entry.name_only && !entry.matcher.is_match(&relative) {
                name.as_ref()
            } else {
                relative.as_str()
            };
            if entry.matcher.is_match(subject) {
                codes.retain(|code| !entry.codes.contains(code));
            }
        }
        codes
    }
}

/// The rules a Pylint `disable`/`enable` entry names: a code, msg-id or
/// symbol, a category letter, or `all`. An entry naming no known rule (a
/// checker, or a message another tool owns) names none.
fn pylint_codes(registry: &Registry, entry: &str) -> BTreeSet<String> {
    if let Some(code) = registry.resolve(entry) {
        return BTreeSet::from([code.to_owned()]);
    }
    let category = entry.len() == 1;
    registry
        .rules()
        .filter(|rule| {
            entry == "all"
                || category
                    && rule
                        .pylint_id
                        .as_deref()
                        .is_some_and(|id| id.starts_with(entry))
        })
        .map(|rule| rule.code.clone())
        .collect()
}

fn resolve_all(registry: &Registry, key: &str, spellings: &[String]) -> Result<BTreeSet<String>> {
    spellings
        .iter()
        .map(|spelling| {
            registry
                .resolve(spelling)
                .map(str::to_owned)
                .ok_or_else(|| Error::config(format!("{key}: no rule is named {spelling:?}")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrolint_core::{RuleInfo, Severity};

    fn rule(code: &str, id: &str, symbol: &str) -> RuleInfo {
        RuleInfo {
            code: code.into(),
            name: code.to_lowercase(),
            summary: String::new(),
            severity: Severity::Warning,
            pylint_id: Some(id.into()),
            pylint_symbol: Some(symbol.into()),
            plugin: None,
        }
    }

    fn enabled(toggles: &[(Toggle, &str)]) -> Vec<String> {
        let registry = Registry::new([
            rule("A1", "W0611", "unused-import"),
            rule("A2", "W0612", "unused-variable"),
            rule("A3", "C0103", "invalid-name"),
        ])
        .unwrap();
        let config = Config {
            pylint_disable: toggles.iter().map(|(t, e)| (*t, (*e).to_owned())).collect(),
            ..Config::default()
        };
        let selection = Selection::new(&registry, &config).unwrap();
        selection.for_file(Path::new("x.py")).into_iter().collect()
    }

    #[test]
    fn disable_takes_an_id_a_symbol_or_a_code_and_drops_unknown_entries() {
        let toggles = [
            (Toggle::Disable, "W0611"),
            (Toggle::Disable, "unused-variable"),
            (Toggle::Disable, "fixme"),
            (Toggle::Disable, "classes"),
        ];
        assert_eq!(enabled(&toggles), ["A3"]);
        assert_eq!(enabled(&[(Toggle::Disable, "A3")]), ["A1", "A2"]);
    }

    #[test]
    fn pragma_messages_are_off_until_enabled() {
        let registry = Registry::new([
            rule("I0011", "I0011", "locally-disabled"),
            rule("A1", "W0611", "unused-import"),
        ])
        .unwrap();
        let on = |toggles: Vec<(Toggle, String)>| {
            let config = Config {
                pylint_disable: toggles,
                ..Config::default()
            };
            let selection = Selection::new(&registry, &config).unwrap();
            selection.is_enabled("I0011")
        };
        assert!(!on(vec![]));
        assert!(on(vec![(Toggle::Enable, "locally-disabled".into())]));
    }

    #[test]
    fn categories_and_all_expand_and_enable_reverts_in_order() {
        assert_eq!(enabled(&[(Toggle::Disable, "W")]), ["A3"]);
        assert_eq!(enabled(&[(Toggle::Disable, "all")]).len(), 0);
        let toggles = [(Toggle::Disable, "all"), (Toggle::Enable, "unused-import")];
        assert_eq!(enabled(&toggles), ["A1"]);
        let toggles = [(Toggle::Enable, "C"), (Toggle::Disable, "C")];
        assert_eq!(enabled(&toggles), ["A1", "A2"]);
    }
}
