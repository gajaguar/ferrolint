//! Which rule codes report for which file: `select`, `ignore` and
//! `per-file-ignores`, each spelling resolved through the registry.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use ferrolint_core::Registry;
use globset::{Glob, GlobMatcher};

use crate::config::Config;
use crate::discover::display_path;
use crate::error::{Error, Result};

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
            None => registry.rules().map(|rule| rule.code.clone()).collect(),
        };
        for code in resolve_all(registry, "ignore", &config.ignore)? {
            enabled.remove(&code);
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
