//! Pylint's own configuration files, read into the settings ferrolint honours.
//!
//! Every file is first flattened to ordered `(option, value)` pairs, so the
//! INI and the TOML spellings share one translation.

use std::collections::BTreeMap;

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
