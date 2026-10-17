//! Target selection: expand the command-line targets into the Python files
//! a run lints, honouring git's ignore rules and the configured excludes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::config::Config;
use crate::error::{Error, Result};

/// The configured `exclude` patterns, read with gitignore semantics
/// relative to the configuration root.
#[derive(Clone)]
pub struct Excludes {
    root: PathBuf,
    matcher: Gitignore,
}

impl Excludes {
    pub fn new(config: &Config) -> Result<Self> {
        let mut builder = GitignoreBuilder::new(&config.root);
        for pattern in &config.exclude {
            builder
                .add_line(None, pattern)
                .map_err(|err| Error::config(format!("exclude pattern {pattern:?}: {err}")))?;
        }
        let matcher = builder
            .build()
            .map_err(|err| Error::config(format!("exclude patterns: {err}")))?;
        Ok(Self {
            root: config.root.clone(),
            matcher,
        })
    }

    pub fn is_excluded(&self, path: &Path, is_dir: bool) -> bool {
        path.starts_with(&self.root)
            && self
                .matcher
                .matched_path_or_any_parents(path, is_dir)
                .is_ignore()
    }
}

/// Hand each Python file under `targets` (resolved against `cwd`) to
/// `found` as the walk reaches it, once per file and in no particular
/// order, so linting can start before the walk ends.
///
/// Files named explicitly are kept even when an ignore rule covers them;
/// the filters apply only to what a directory walk discovers.
pub fn stream(
    cwd: &Path,
    targets: &[PathBuf],
    excludes: &Excludes,
    mut found: impl FnMut(PathBuf),
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut emit = |path: PathBuf| {
        if seen.insert(path.clone()) {
            found(path);
        }
    };
    for target in targets {
        let path = normalize(&cwd.join(target));
        if path.is_file() {
            if is_python(&path) {
                emit(path);
            }
        } else if path.is_dir() {
            walk(&path, excludes, &mut emit)?;
        } else {
            return Err(Error::usage(format!(
                "{}: no such file or directory",
                target.display()
            )));
        }
    }
    Ok(())
}

fn walk(dir: &Path, excludes: &Excludes, emit: &mut impl FnMut(PathBuf)) -> Result<()> {
    let filter = excludes.clone();
    let walker = WalkBuilder::new(dir)
        .hidden(false)
        .ignore(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(true)
        .filter_entry(move |entry| {
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            entry.file_name() != ".git" && !filter.is_excluded(entry.path(), is_dir)
        })
        .build();
    for entry in walker {
        let entry = entry.map_err(|err| Error::io(err.to_string()))?;
        let is_file = entry.file_type().is_some_and(|kind| kind.is_file());
        if is_file && is_python(entry.path()) {
            emit(normalize(entry.path()));
        }
    }
    Ok(())
}

fn is_python(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "py")
}

/// Drop `.` components so one file has one spelling.
fn normalize(path: &Path) -> PathBuf {
    path.components().collect()
}

/// The path as a report shows it: relative to `cwd` when under it, with
/// `/` separators.
pub fn display_path(cwd: &Path, path: &Path) -> String {
    match path.strip_prefix(cwd) {
        Ok(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().replace('\\', "/"),
    }
}
