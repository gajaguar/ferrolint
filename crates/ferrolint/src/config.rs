//! Configuration loading: `ferrolint.toml`, or the `[tool.ferrolint]` table
//! of `pyproject.toml`, in the working directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};

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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlugin {
    path: PathBuf,
    sha256: String,
}

/// Load the configuration for a run started in `cwd`.
///
/// An explicit `--config` file is read as a `ferrolint.toml`, or as a
/// `pyproject.toml` when it has that name. Otherwise `ferrolint.toml` in
/// `cwd` wins entirely over `pyproject.toml`; with neither, the defaults
/// apply.
pub fn load(cwd: &Path, explicit: Option<&Path>) -> Result<Config> {
    if let Some(path) = explicit {
        let path = cwd.join(path);
        let is_pyproject = path
            .file_name()
            .is_some_and(|name| name == "pyproject.toml");
        let raw = if is_pyproject {
            read_pyproject(&path)?
        } else {
            read_ferrolint(&path)?
        };
        let root = path
            .parent()
            .map_or_else(|| cwd.to_path_buf(), Path::to_path_buf);
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
    Ok(resolve(RawConfig::default(), cwd.to_path_buf()))
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
}
