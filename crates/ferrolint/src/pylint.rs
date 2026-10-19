//! Pylint's own configuration files, read into the settings ferrolint honours.
//!
//! Every file is first flattened to ordered `(option, value)` pairs, so the
//! INI and the TOML spellings share one translation.

use std::collections::BTreeMap;

use crate::pylint_names::{self as names, contains};

/// A finding about the configuration file itself, before the registry decides
/// whether it applies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Issue {
    /// The Pylint msg-id.
    pub code: &'static str,
    pub message: String,
    /// The message name a W0012/R0022 is about; the registry may know it.
    pub name: Option<String>,
}

const CATEGORIES: [&str; 12] = [
    "c",
    "r",
    "w",
    "e",
    "f",
    "i",
    "convention",
    "refactor",
    "warning",
    "error",
    "fatal",
    "info",
];

/// The W0012 or R0022 for a `disable`/`enable`/pragma name that is no Pylint
/// message, category, checker or `all`.
pub fn message_issue(action: &str, name: &str) -> Option<Issue> {
    let lower = name.to_ascii_lowercase();
    let upper = name.to_ascii_uppercase();
    let known = lower == "all"
        || lower.starts_with("rp")
        || CATEGORIES.contains(&lower.as_str())
        || contains(names::CHECKERS, &lower)
        || contains(names::MESSAGES, name)
        || contains(names::MESSAGES, &upper);
    if known {
        return None;
    }
    let name_owned = Some(name.to_owned());
    if contains(names::DELETED, name) || contains(names::DELETED, &upper) {
        return Some(Issue {
            code: "R0022",
            message: format!(
                "Useless option value for '{action}', '{name}' was removed from pylint or moved to an optional extension."
            ),
            name: name_owned,
        });
    }
    Some(Issue {
        code: "W0012",
        message: format!(
            "Unknown option value for '{action}', expected a valid pylint message and got '{name}'"
        ),
        name: name_owned,
    })
}

/// Whether `load-plugins` names a plugin whose options and messages the
/// generated tables do not list.
fn foreign_plugin(entries: &[(String, String)]) -> bool {
    entries
        .iter()
        .filter(|(key, _)| key == "load-plugins")
        .flat_map(|(_, value)| value.split(','))
        .map(str::trim)
        .filter(|plugin| !plugin.is_empty())
        .any(|plugin| plugin != "pylint_django" && !plugin.starts_with("pylint.extensions"))
}

/// What Pylint would say about the names in a configuration. `ini` is true for
/// INI files, whose keys Pylint lowercases; `sections` are the TOML tables
/// under `[tool.pylint]`.
pub fn issues(entries: &[(String, String)], ini: bool, sections: &[String]) -> Vec<Issue> {
    let mut out = Vec::new();
    let normal = |name: &str| name.replace(['_', '-'], " ").to_ascii_lowercase();
    for section in sections {
        let wanted = normal(section);
        if !names::SECTIONS.iter().any(|known| normal(known) == wanted) {
            out.push(Issue {
                code: "E0014",
                message: format!(
                    "Out-of-place setting encountered in top level configuration-section '{section}' : '{section}'"
                ),
                name: None,
            });
        }
    }
    if foreign_plugin(entries) {
        return out;
    }
    let mut unknown: Vec<String> = Vec::new();
    for (key, value) in entries {
        let key = if ini {
            key.to_ascii_lowercase()
        } else {
            key.clone()
        };
        if !contains(names::OPTIONS, &key) {
            if !unknown.contains(&key) {
                unknown.push(key);
            }
        } else if key == "disable" || key == "enable" {
            out.extend(
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .filter_map(|item| message_issue(&format!("--{key}"), item)),
            );
        }
    }
    if !unknown.is_empty() {
        out.push(Issue {
            code: "E0015",
            message: format!("Unrecognized option found: {}", unknown.join(", ")),
            name: None,
        });
    }
    out
}

/// The E0013 for each `load-plugins` module that `exists` does not find.
pub fn plugin_issues(entries: &[(String, String)], exists: impl Fn(&str) -> bool) -> Vec<Issue> {
    entries
        .iter()
        .filter(|(key, _)| key == "load-plugins")
        .flat_map(|(_, value)| value.split(','))
        .map(str::trim)
        .filter(|plugin| !plugin.is_empty() && !exists(plugin))
        .map(|plugin| Issue {
            code: "E0013",
            message: format!(
                "Plugin '{plugin}' is impossible to load, is it installed ? ('No module named '{plugin}'')"
            ),
            name: None,
        })
        .collect()
}

/// The names of the tables directly under `[tool.pylint]`.
pub fn toml_sections(table: &toml::Table) -> Vec<String> {
    table
        .get("tool")
        .and_then(|tool| tool.get("pylint"))
        .and_then(toml::Value::as_table)
        .map(|section| {
            section
                .iter()
                .filter(|(_, value)| value.is_table())
                .map(|(name, _)| name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a `disable`/`enable` entry turns rules off or back on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Toggle {
    Disable,
    Enable,
}

/// What a Pylint configuration says that ferrolint acts on or keeps.
#[derive(Debug, Default)]
pub struct Settings {
    /// The `disable` and `enable` entries, in the order Pylint applies them.
    pub toggles: Vec<(Toggle, String)>,
    /// Every other option, as written.
    pub options: BTreeMap<String, String>,
    /// The `ignore` base names.
    pub ignore: Vec<String>,
    pub py_version: Option<String>,
}

impl Settings {
    pub fn from_entries(entries: Vec<(String, String)>) -> Self {
        let mut settings = Self::default();
        for (key, value) in entries {
            let items = || {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            };
            match key.as_str() {
                "disable" => settings
                    .toggles
                    .extend(items().into_iter().map(|item| (Toggle::Disable, item))),
                "enable" => settings
                    .toggles
                    .extend(items().into_iter().map(|item| (Toggle::Enable, item))),
                "ignore" => settings.ignore.extend(items()),
                "py-version" => settings.py_version = Some(value),
                _ => {
                    settings.options.insert(key, value);
                }
            }
        }
        settings
    }
}

/// The options of an INI file, in file order. `only_pylint` keeps just the
/// `pylint*` sections (`setup.cfg`, `tox.ini`) and gives `None` without one.
pub fn ini(text: &str, only_pylint: bool) -> Option<Vec<(String, String)>> {
    let mut entries: Vec<(String, String)> = Vec::new();
    let mut found = !only_pylint;
    let mut kept = !only_pylint;
    let mut open = false;
    for raw in text.lines() {
        let line = without_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            kept = !only_pylint || name.to_ascii_lowercase().starts_with("pylint");
            found |= kept;
            open = false;
        } else if open && raw.starts_with(char::is_whitespace) {
            if let Some((_, value)) = entries.last_mut() {
                if !value.is_empty() {
                    value.push('\n');
                }
                value.push_str(line);
            }
        } else if kept && let Some((key, value)) = line.split_once(['=', ':']) {
            entries.push((key.trim().to_owned(), value.trim().to_owned()));
            open = true;
        }
    }
    found.then_some(entries)
}

/// The options under `[tool.pylint]`, sections flattened, or `None` without it.
pub fn toml(table: &toml::Table) -> Option<Vec<(String, String)>> {
    fn walk(table: &toml::Table, out: &mut Vec<(String, String)>) {
        for (key, value) in table {
            match value {
                toml::Value::Table(inner) => walk(inner, out),
                toml::Value::Array(items) => out.push((key.clone(), join(items))),
                other => out.push((key.clone(), text(other))),
            }
        }
    }
    let section = table.get("tool")?.get("pylint")?.as_table()?;
    let mut out = Vec::new();
    walk(section, &mut out);
    Some(out)
}

fn join(items: &[toml::Value]) -> String {
    items.iter().map(text).collect::<Vec<_>>().join(",")
}

fn text(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Boolean(true) => "yes".into(),
        toml::Value::Boolean(false) => "no".into(),
        toml::Value::Array(items) => join(items),
        other => other.to_string(),
    }
}

/// Drop a trailing `#` or `;` comment: Pylint requires whitespace before it.
fn without_comment(line: &str) -> &str {
    let mut previous_blank = true;
    for (index, ch) in line.char_indices() {
        if previous_blank && (ch == '#' || ch == ';') {
            return &line[..index];
        }
        previous_blank = ch.is_whitespace();
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const RC: &str = "\
[MASTER]
# a comment
ignore=CVS
load-plugins=pylint_django   # inline

[MESSAGES CONTROL]
disable=missing-module-docstring,
        fixme,   ; tail

        unused-import
enable=C,
       fixme

[FORMAT]
ignore-long-lines=^\\s*(# )?<?https?://\\S+>?$
";

    #[test]
    fn ini_keeps_order_continuations_and_drops_comments() {
        let entries = ini(RC, false).unwrap();
        let keys: Vec<_> = entries.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            [
                "ignore",
                "load-plugins",
                "disable",
                "enable",
                "ignore-long-lines"
            ]
        );
        assert_eq!(entries[1].1, "pylint_django");
        assert_eq!(entries[3].1, "C,\nfixme");
        assert_eq!(entries[4].1, "^\\s*(# )?<?https?://\\S+>?$");
    }

    #[test]
    fn settings_split_the_consumed_options_from_the_rest() {
        let settings = Settings::from_entries(ini(RC, false).unwrap());
        let toggles: Vec<_> = settings
            .toggles
            .iter()
            .map(|(t, e)| (*t == Toggle::Disable, e.as_str()))
            .collect();
        assert_eq!(
            toggles,
            [
                (true, "missing-module-docstring"),
                (true, "fixme"),
                (true, "unused-import"),
                (false, "C"),
                (false, "fixme"),
            ]
        );
        assert_eq!(settings.ignore, ["CVS"]);
        assert!(settings.options.contains_key("load-plugins"));
        assert!(!settings.options.contains_key("disable"));
    }

    #[test]
    fn a_missing_plugin_module_is_e0013() {
        let entries = ini("[MASTER]\nload-plugins=pylint_django, nowhere.mod\n", false).unwrap();
        let issues = plugin_issues(&entries, |module| module == "pylint_django");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "E0013");
        assert!(issues[0].message.contains("'nowhere.mod'"));
    }

    #[test]
    fn only_pylint_sections_count_in_setup_cfg() {
        let text = "[flake8]\nmax-line-length = 9\n\n[pylint.messages_control]\ndisable = fixme\n";
        let entries = ini(text, true).unwrap();
        assert_eq!(entries, [("disable".to_owned(), "fixme".to_owned())]);
        assert!(ini("[flake8]\nx = 1\n", true).is_none());
    }

    #[test]
    fn toml_flattens_sections_in_key_order_and_stringifies() {
        let table: toml::Table = "\
[tool.pylint.messages_control]
enable = [\"fixme\"]
disable = [\"fixme\", \"C\"]
[tool.pylint.main]
py-version = \"3.14\"
jobs = 0
persistent = false
"
        .parse()
        .unwrap();
        let settings = Settings::from_entries(toml(&table).unwrap());
        assert_eq!(settings.toggles.len(), 3);
        assert_eq!(settings.toggles[0], (Toggle::Enable, "fixme".to_owned()));
        assert_eq!(settings.py_version.as_deref(), Some("3.14"));
        assert_eq!(settings.options["jobs"], "0");
        assert_eq!(settings.options["persistent"], "no");
        assert!(toml(&"[tool.black]\n".parse().unwrap()).is_none());
    }
}
