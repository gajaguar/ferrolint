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
    per_file: Vec<PerFile<BTreeSet<String>>>,
    good_names: Vec<PerFile<Vec<String>>>,
}

struct PerFile<T> {
    matcher: GlobMatcher,
    /// Match the pattern against the file name alone: it has no `/`.
    name_only: bool,
    value: T,
}

impl<T> PerFile<T> {
    fn new(key: &str, pattern: &str, value: T) -> Result<Self> {
        let glob = Glob::new(pattern)
            .map_err(|err| Error::config(format!("{key} pattern {pattern:?}: {err}")))?;
        Ok(Self {
            matcher: glob.compile_matcher(),
            name_only: !pattern.contains('/'),
            value,
        })
    }

    fn matches(&self, relative: &str, name: &str) -> bool {
        self.matcher.is_match(relative) || self.name_only && self.matcher.is_match(name)
    }
}

impl Selection {
    pub fn new(registry: &Registry, config: &Config) -> Result<Self> {
        let mut enabled: BTreeSet<String> = match &config.select {
            Some(select) => resolve_all(registry, "select", select)?,
            None => registry
                .rules()
                .map(|rule| rule.code.clone())
                .filter(|code| !crate::native::OFF_BY_DEFAULT.contains(&code.as_str()))
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
            let codes = resolve_all(registry, "per-file-ignores", codes)?;
            per_file.push(PerFile::new("per-file-ignores", pattern, codes)?);
        }
        let mut good_names = Vec::new();
        for (pattern, regexes) in &config.per_file_good_names {
            for regex in regexes {
                regex::Regex::new(regex).map_err(|err| {
                    Error::config(format!("per-file-good-names regex {regex:?}: {err}"))
                })?;
            }
            good_names.push(PerFile::new(
                "per-file-good-names",
                pattern,
                regexes.clone(),
            )?);
        }
        Ok(Self {
            root: config.root.clone(),
            enabled,
            per_file,
            good_names,
        })
    }

    /// Whether `code` reports outside any file, as the configuration's own
    /// findings do.
    pub fn is_enabled(&self, code: &str) -> bool {
        self.enabled.contains(code)
    }

    /// The codes that report for the file at `path`.
    pub fn for_file(&self, path: &Path) -> BTreeSet<String> {
        let (relative, name) = self.subject(path);
        let mut codes = self.enabled.clone();
        for entry in self.per_file.iter().filter(|e| e.matches(&relative, &name)) {
            codes.retain(|code| !entry.value.contains(code));
        }
        codes
    }

    /// The `good-names-rgxs` entries `per-file-good-names` adds for the file at `path`.
    pub fn good_names(&self, path: &Path) -> Vec<String> {
        let (relative, name) = self.subject(path);
        self.good_names
            .iter()
            .filter(|e| e.matches(&relative, &name))
            .flat_map(|e| e.value.iter().cloned())
            .collect()
    }

    fn subject(&self, path: &Path) -> (String, String) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        (display_path(&self.root, path), name.unwrap_or_default())
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
    fn per_file_good_names_follow_the_per_file_ignores_globs() {
        let registry = Registry::new([rule("A3", "C0103", "invalid-name")]).unwrap();
        let names = |pattern: &str, regex: &str| {
            let config = Config {
                per_file_good_names: [(pattern.to_owned(), vec![regex.to_owned()])].into(),
                ..Config::default()
            };
            Selection::new(&registry, &config)
        };
        let selection = names("tests/**", "^setUp").unwrap();
        assert_eq!(selection.good_names(Path::new("tests/a/x.py")), ["^setUp"]);
        assert_eq!(
            selection.good_names(Path::new("src/x.py")),
            Vec::<String>::new()
        );
        let selection = names("conftest.py", "^tearDown").unwrap();
        assert_eq!(
            selection.good_names(Path::new("a/conftest.py")),
            ["^tearDown"]
        );
        assert!(names("tests/**", "(").is_err());
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
    fn a_rule_pylint_ships_disabled_needs_an_enable() {
        let registry = Registry::new([
            rule("A1", "W0611", "unused-import"),
            rule(
                "C1804",
                "C1804",
                "use-implicit-booleaness-not-comparison-to-string",
            ),
        ])
        .unwrap();
        let with = |toggles: Vec<(Toggle, String)>| {
            let config = Config {
                pylint_disable: toggles,
                ..Config::default()
            };
            let selection = Selection::new(&registry, &config).unwrap();
            selection
                .for_file(Path::new("x.py"))
                .into_iter()
                .collect::<Vec<_>>()
        };
        assert_eq!(with(vec![]), ["A1"]);
        assert_eq!(
            with(vec![(Toggle::Enable, "C1804".into())]),
            ["A1", "C1804"]
        );
        assert_eq!(with(vec![(Toggle::Enable, "all".into())]), ["A1", "C1804"]);
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

    #[test]
    fn a_message_off_by_default_needs_enable_or_select() {
        let registry = Registry::new([rule("R6105", "R6105", "prefer-typing-namedtuple")]).unwrap();
        let codes = |config: &Config| {
            let selection = Selection::new(&registry, config).unwrap();
            selection.for_file(Path::new("x.py")).len()
        };
        assert_eq!(codes(&Config::default()), 0);
        let enable = Config {
            pylint_disable: vec![(Toggle::Enable, "prefer-typing-namedtuple".to_owned())],
            ..Config::default()
        };
        assert_eq!(codes(&enable), 1);
        let select = Config {
            select: Some(vec!["R6105".to_owned()]),
            ..Config::default()
        };
        assert_eq!(codes(&select), 1);
    }
}
