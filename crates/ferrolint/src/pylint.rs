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

// Pylint upper-cases the name and looks it up among the one-letter categories only.
const CATEGORIES: [&str; 6] = ["c", "r", "w", "e", "f", "i"];

/// The W0012 or R0022 for a `disable`/`enable`/pragma name that is no Pylint
/// message, category, checker or `all`. `plugin_loaded` tells whether
/// `load-plugins` names a plugin module: a name a plugin owns is known only
/// while the plugin is loaded, as in Pylint.
pub fn message_issue(
    action: &str,
    name: &str,
    plugin_loaded: &dyn Fn(&str) -> bool,
) -> Option<Issue> {
    let lower = name.to_ascii_lowercase();
    let upper = name.to_ascii_uppercase();
    let pylint_name = contains(names::CHECKERS, &lower)
        || contains(names::MESSAGES, name)
        || contains(names::MESSAGES, &upper);
    let owner = names::owner(name)
        .or_else(|| names::owner(&upper))
        .or_else(|| names::owner(&lower));
    let known = name == "all"
        || lower.starts_with("rp")
        || CATEGORIES.contains(&lower.as_str())
        || (pylint_name && owner.is_none_or(plugin_loaded));
    if known {
        return None;
    }
    if contains(names::DELETED, name)
        || contains(names::DELETED, &upper)
        || contains(names::MOVED, name)
        || contains(names::MOVED, &upper)
    {
        return Some(moved_issue(action, name));
    }
    Some(Issue {
        code: "W0012",
        message: format!(
            "Unknown option value for '{action}', expected a valid pylint message and got '{name}'"
        ),
        name: Some(name.to_owned()),
    })
}

fn moved_issue(action: &str, name: &str) -> Issue {
    Issue {
        code: "R0022",
        message: format!(
            "Useless option value for '{action}', '{name}' was removed from pylint or moved to an optional extension."
        ),
        name: Some(name.to_owned()),
    }
}

/// Whether `load-plugins` names a plugin that `exists` and whose options and
/// messages the generated tables do not list. A plugin Pylint cannot import
/// adds none, so the other findings still stand.
fn foreign_plugin(entries: &[(String, String)], exists: impl Fn(&str) -> bool) -> bool {
    entries
        .iter()
        .filter(|(key, _)| key == "load-plugins")
        .flat_map(|(_, value)| value.split(','))
        .map(str::trim)
        .filter(|plugin| !plugin.is_empty())
        .any(|plugin| !plugin.starts_with("pylint.extensions") && exists(plugin))
}

/// What Pylint would say about the names in a configuration. `ini` is true for
/// INI files, whose keys Pylint lowercases.
pub fn issues(
    entries: &[(String, String)],
    ini: bool,
    exists: impl Fn(&str) -> bool,
) -> Vec<Issue> {
    let mut out = Vec::new();
    if foreign_plugin(entries, exists) {
        return out;
    }
    let loaded: Vec<&str> = entries
        .iter()
        .filter(|(key, _)| key == "load-plugins")
        .flat_map(|(_, value)| value.split(','))
        .map(str::trim)
        .filter(|plugin| !plugin.is_empty())
        .collect();
    let plugin_loaded = |module: &str| loaded.contains(&module);
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
                    .filter_map(|item| {
                        let action = format!("--{key}");
                        message_issue(&action, item, &plugin_loaded)
                    }),
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
load-plugins=pylint.extensions.mccabe   # inline

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
        assert_eq!(entries[1].1, "pylint.extensions.mccabe");
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
        let entries = ini(
            "[MASTER]\nload-plugins=pylint.extensions.mccabe, nowhere.mod\n",
            false,
        )
        .unwrap();
        let issues = plugin_issues(&entries, |module| module == "pylint.extensions.mccabe");
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "E0013");
        assert!(issues[0].message.contains("'nowhere.mod'"));
    }

    #[test]
    fn only_a_plugin_that_exists_silences_the_option_check() {
        let entries = ini("[MASTER]\nload-plugins=other\nbogus=1\n", false).unwrap();
        assert_eq!(issues(&entries, true, |_| true), []);
        let issues = issues(&entries, true, |_| false);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].code, "E0015");
    }

    #[test]
    fn a_plugin_message_is_w0012_unless_its_plugin_is_loaded() {
        let off = ini(
            "[MASTER]\ndisable=dict-init-mutate,consider-using-any-or-all,R0204\n",
            false,
        )
        .unwrap();
        let codes: Vec<_> = issues(&off, true, |_| true)
            .iter()
            .map(|i| i.code)
            .collect();
        assert_eq!(codes, ["W0012", "W0012", "W0012"]);
        let on = ini(
            "[MASTER]\nload-plugins=pylint.extensions.dict_init_mutate,pylint.extensions.for_any_all\n\
             disable=dict-init-mutate,consider-using-any-or-all\n",
            false,
        )
        .unwrap();
        assert_eq!(issues(&on, true, |_| true), []);
    }

    #[test]
    fn a_plugin_checker_name_is_w0012_unless_its_plugin_is_loaded() {
        let off = ini("[MASTER]\ndisable=multiple_types\n", false).unwrap();
        let found = issues(&off, true, |_| true);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code, "W0012");
        assert!(found[0].message.contains("'multiple_types'"));
        let on = ini(
            "[MASTER]\nload-plugins=pylint.extensions.redefined_variable_type\ndisable=multiple_types\n",
            false,
        )
        .unwrap();
        assert_eq!(issues(&on, true, |_| true), []);
    }

    #[test]
    fn names_are_case_sensitive_but_codes_and_categories_are_not() {
        for ok in ["w0012", "R1710", "c", "E", "all", "IMPORTS", "rp0001"] {
            assert!(message_issue("disable", ok, &|_| false).is_none(), "{ok}");
        }
        for bad in [
            "ALL",
            "convention",
            "Warning",
            "Unused-Import",
            "UNUSED-IMPORT",
            "c0501",
        ] {
            let issue = message_issue("disable", bad, &|_| false).unwrap();
            assert_eq!(issue.code, "W0012", "{bad}");
        }
        assert!(message_issue("disable", "c0501", &|m| m.ends_with("for_any_all")).is_none());
    }

    #[test]
    fn no_self_use_is_r0022_unless_its_extension_is_loaded() {
        let off = ini("[MASTER]\ndisable=no-self-use,R0201\n", false).unwrap();
        let codes: Vec<_> = issues(&off, true, |_| true)
            .iter()
            .map(|i| i.code)
            .collect();
        assert_eq!(codes, ["R0022", "R0022"]);
        let on = ini(
            "[MASTER]\nload-plugins=pylint.extensions.no_self_use\ndisable=no-self-use\n",
            false,
        )
        .unwrap();
        assert_eq!(issues(&on, true, |_| true), []);
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
