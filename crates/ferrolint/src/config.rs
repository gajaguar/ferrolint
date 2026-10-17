//! Configuration loading: `ferrolint.toml`, the `[tool.ferrolint]` table of
//! `pyproject.toml`, or else Pylint's own configuration, in the working
//! directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::pylint::{self, Settings, Toggle};

/// The settings a run uses, with plugin paths made absolute.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The directory relative paths and globs resolve against.
    pub root: PathBuf,
    pub plugins: Vec<PluginEntry>,
    pub select: Option<Vec<String>>,
    pub ignore: Vec<String>,
    pub exclude: Vec<String>,
    pub per_file_ignores: BTreeMap<String, Vec<String>>,
    /// Which rule owns a Pylint alias several rules declare.
    pub pylint_aliases: BTreeMap<String, String>,
    /// The Python `(major, minor)` the code targets, from `py-version`.
    pub py_version: Option<(u32, u32)>,
    /// Pylint's `disable` and `enable` entries, in order; `Selection`
    /// resolves them against the registry.
    pub pylint_disable: Vec<(Toggle, String)>,
    /// The other Pylint options, as written; none has an effect yet.
    pub options: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginEntry {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RawConfig {
    #[serde(default)]
    plugins: Vec<RawPlugin>,
    select: Option<Vec<String>>,
    #[serde(default)]
    ignore: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    per_file_ignores: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pylint_aliases: BTreeMap<String, String>,
    #[serde(default, deserialize_with = "version")]
    py_version: Option<(u32, u32)>,
}

/// Parse `"MAJOR.MINOR"`, the spelling Pylint's `py-version` uses.
fn version<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> std::result::Result<Option<(u32, u32)>, D::Error> {
    let text = String::deserialize(de)?;
    parse_version(&text)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

fn parse_version(text: &str) -> std::result::Result<(u32, u32), String> {
    text.split_once('.')
        .and_then(|(major, minor)| Some((major.parse().ok()?, minor.parse().ok()?)))
        .ok_or_else(|| format!("py-version must be MAJOR.MINOR, like \"3.14\", not \"{text}\""))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlugin {
    path: PathBuf,
    sha256: String,
}

/// Pylint's configuration files, in the order Pylint looks for them.
const PYLINT_FILES: [&str; 5] = [
    ".pylintrc",
    "pylintrc",
    "pyproject.toml",
    "setup.cfg",
    "tox.ini",
];

/// Load the configuration for a run started in `cwd`.
///
/// An explicit `--config` file is read as a Pylint configuration when it is
/// named like one (`.pylintrc`, `pylintrc`, `setup.cfg`, `tox.ini`), as a
/// `pyproject.toml` when it has that name, else as a `ferrolint.toml`.
/// Otherwise the first source that exists wins entirely, never merged:
/// `ferrolint.toml`, `[tool.ferrolint]`, then Pylint's files in `PYLINT_FILES`
/// order; with none, the defaults apply.
pub fn load(cwd: &Path, explicit: Option<&Path>) -> Result<Config> {
    if let Some(path) = explicit {
        let path = cwd.join(path);
        let name = path.file_name().map(|name| name.to_string_lossy());
        let root = path
            .parent()
            .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf);
        let raw = match name.as_deref() {
            Some("pyproject.toml") => read_pyproject(&path)?,
            Some(name) if PYLINT_FILES.contains(&name) => {
                return Ok(
                    read_pylint(&path)?.unwrap_or_else(|| resolve(RawConfig::default(), root))
                );
            }
            _ => read_ferrolint(&path)?,
        };
        return Ok(resolve(raw.unwrap_or_default(), root));
    }
    let ferrolint = cwd.join("ferrolint.toml");
    if ferrolint.is_file() {
        let raw = read_ferrolint(&ferrolint)?.unwrap_or_default();
        return Ok(resolve(raw, cwd.to_path_buf()));
    }
    let pyproject = cwd.join("pyproject.toml");
    if pyproject.is_file()
        && let Some(raw) = read_pyproject(&pyproject)?
    {
        return Ok(resolve(raw, cwd.to_path_buf()));
    }
    for name in PYLINT_FILES {
        let path = cwd.join(name);
        if path.is_file()
            && let Some(config) = read_pylint(&path)?
        {
            return Ok(config);
        }
    }
    Ok(resolve(RawConfig::default(), cwd.to_path_buf()))
}

/// The settings of a Pylint file, or `None` when it holds no Pylint section.
fn read_pylint(path: &Path) -> Result<Option<Config>> {
    let text = read(path)?;
    let name = path.file_name().map(|name| name.to_string_lossy());
    let entries = if name.as_deref() == Some("pyproject.toml") {
        let table: toml::Table = toml::from_str(&text)
            .map_err(|err| Error::config(format!("{}: {}", path.display(), err.message())))?;
        pylint::toml(&table)
    } else {
        let only_pylint = matches!(name.as_deref(), Some("setup.cfg" | "tox.ini"));
        pylint::ini(&text, only_pylint)
    };
    let Some(entries) = entries else {
        return Ok(None);
    };
    let settings = Settings::from_entries(entries);
    let py_version = settings
        .py_version
        .as_deref()
        .map(parse_version)
        .transpose()
        .map_err(|err| Error::config(format!("{}: {err}", path.display())))?;
    let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut config = resolve(RawConfig::default(), root);
    config.exclude = settings.ignore;
    config.py_version = py_version;
    config.pylint_disable = settings.toggles;
    config.options = settings.options;
    Ok(Some(config))
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map_err(|err| Error::config(format!("cannot read {}: {err}", path.display())))
}

fn read_ferrolint(path: &Path) -> Result<Option<RawConfig>> {
    let text = read(path)?;
    toml::from_str(&text)
        .map(Some)
        .map_err(|err| Error::config(format!("{}: {}", path.display(), err.message())))
}

fn read_pyproject(path: &Path) -> Result<Option<RawConfig>> {
    let text = read(path)?;
    let table: toml::Table = toml::from_str(&text)
        .map_err(|err| Error::config(format!("{}: {}", path.display(), err.message())))?;
    let Some(section) = table
        .get("tool")
        .and_then(|tool| tool.get("ferrolint"))
        .cloned()
    else {
        return Ok(None);
    };
    section
        .try_into()
        .map(Some)
        .map_err(|err: toml::de::Error| {
            Error::config(format!(
                "{} [tool.ferrolint]: {}",
                path.display(),
                err.message()
            ))
        })
}

fn resolve(raw: RawConfig, root: PathBuf) -> Config {
    let plugins = raw
        .plugins
        .into_iter()
        .map(|plugin| PluginEntry {
            path: root.join(plugin.path),
            sha256: plugin.sha256.to_ascii_lowercase(),
        })
        .collect();
    Config {
        root,
        plugins,
        select: raw.select,
        ignore: raw.ignore,
        exclude: raw.exclude,
        per_file_ignores: raw.per_file_ignores,
        pylint_aliases: raw.pylint_aliases,
        py_version: raw.py_version,
        pylint_disable: Vec::new(),
        options: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_keys_are_errors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("ferrolint.toml"), "unknown-key = []\n").unwrap();
        let error = load(dir.path(), None).unwrap_err();
        assert!(error.message.contains("unknown-key"), "{error}");
    }

    #[test]
    fn py_version_is_major_dot_minor() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ferrolint.toml");
        fs::write(&file, "py-version = \"3.14\"\n").unwrap();
        assert_eq!(load(dir.path(), None).unwrap().py_version, Some((3, 14)));
        for bad in ["\"3\"", "\"3.x\"", "3.14"] {
            fs::write(&file, format!("py-version = {bad}\n")).unwrap();
            assert!(load(dir.path(), None).is_err(), "{bad}");
        }
        fs::write(&file, "").unwrap();
        assert_eq!(load(dir.path(), None).unwrap().py_version, None);
    }

    #[test]
    fn pyproject_without_the_table_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "[project]\nname = 'x'\n").unwrap();
        let config = load(dir.path(), None).unwrap();
        assert!(config.select.is_none());
    }

    #[test]
    fn plugin_paths_resolve_against_the_config_directory() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("ferrolint.toml"),
            "plugins = [{ path = \"p.wasm\", sha256 = \"AB\" }]\n",
        )
        .unwrap();
        let config = load(dir.path(), None).unwrap();
        assert_eq!(
            config.plugins,
            vec![PluginEntry {
                path: dir.path().join("p.wasm"),
                sha256: "ab".into()
            }]
        );
    }

    fn write(dir: &tempfile::TempDir, name: &str, text: &str) {
        fs::write(dir.path().join(name), text).unwrap();
    }

    #[test]
    fn pylintrc_gives_toggles_options_ignore_and_py_version() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir,
            ".pylintrc",
            "[MASTER]\nignore=CVS\nload-plugins=pylint_django\npy-version=3.12\n\n\
             [MESSAGES CONTROL]\ndisable=fixme,\n  W\nenable=unused-import\n",
        );
        let config = load(dir.path(), None).unwrap();
        assert_eq!(config.exclude, ["CVS"]);
        assert_eq!(config.py_version, Some((3, 12)));
        assert_eq!(config.options["load-plugins"], "pylint_django");
        assert_eq!(config.pylint_disable.len(), 3);
        assert_eq!(config.root, dir.path());
        write(&dir, ".pylintrc", "py-version=3\n");
        assert!(load(dir.path(), None).is_err());
    }

    #[test]
    fn the_first_source_wins_and_nothing_merges() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "tox.ini", "[pylint.messages_control]\ndisable=tox\n");
        write(
            &dir,
            "setup.cfg",
            "[pylint.messages_control]\ndisable=cfg\n",
        );
        let disabled = |dir: &tempfile::TempDir| {
            load(dir.path(), None)
                .unwrap()
                .pylint_disable
                .into_iter()
                .map(|(_, entry)| entry)
                .collect::<Vec<_>>()
        };
        assert_eq!(disabled(&dir), ["cfg"]);
        write(
            &dir,
            "pyproject.toml",
            "[tool.pylint.messages_control]\ndisable=['toml']\n",
        );
        assert_eq!(disabled(&dir), ["toml"]);
        write(&dir, "pylintrc", "[MESSAGES CONTROL]\ndisable=rc\n");
        assert_eq!(disabled(&dir), ["rc"]);
        write(&dir, ".pylintrc", "[MESSAGES CONTROL]\ndisable=dotrc\n");
        assert_eq!(disabled(&dir), ["dotrc"]);
        write(&dir, "pyproject.toml", "[tool.ferrolint]\nignore=['X']\n");
        write(&dir, ".pylintrc", "[MESSAGES CONTROL]\ndisable=dotrc\n");
        let config = load(dir.path(), None).unwrap();
        assert_eq!(config.ignore, ["X"]);
        assert!(config.pylint_disable.is_empty(), "[tool.ferrolint] wins");
        write(&dir, "ferrolint.toml", "");
        assert_eq!(load(dir.path(), None).unwrap().ignore.len(), 0);
    }

    #[test]
    fn a_setup_cfg_without_a_pylint_section_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "setup.cfg", "[flake8]\nmax-line-length=9\n");
        write(&dir, "tox.ini", "[pylint]\nmax-args=3\n");
        assert_eq!(load(dir.path(), None).unwrap().options["max-args"], "3");
    }

    #[test]
    fn an_explicit_pylint_file_is_read_as_pylint() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir, "pylintrc", "[MESSAGES CONTROL]\ndisable=fixme\n");
        let config = load(dir.path(), Some(Path::new("pylintrc"))).unwrap();
        assert_eq!(config.pylint_disable.len(), 1);
    }
}
