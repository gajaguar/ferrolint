//! The engine's side of the project phase: which files it covers, how the
//! modules are named, and the one cache entry of its result.
//!
//! The per-file cache cannot hold these rules' findings, since they depend
//! on files other than their own. So the phase runs after the per-file one
//! on every run, but a run whose inputs did not change replays the previous
//! result: the entry is keyed by the bytes, names and enabled rules of every
//! linted file and the options, and carries a stamp of each directory listed
//! and file read while resolving imports (a package installed, a module
//! added or edited anywhere on the search path changes a stamp).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use ferrolint_core::{Finding, Registry};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::cache::{Cache, ProjectEntry};
use crate::engine::hex;
use crate::native::Project;
use crate::native::project_phase::{
    self, CYCLIC, DUPLICATE, IMPORT_ERROR, NO_NAME, Opts, RELATIVE_BEYOND_TOP, Unit,
};
use crate::selection::Selection;

/// What the phase needs of the run.
pub struct Params<'a> {
    pub cwd: &'a Path,
    /// The files and directories the run was asked to lint.
    pub arguments: &'a [PathBuf],
    pub registry: &'a Registry,
    pub selection: &'a Selection,
    pub options: &'a [(String, String)],
    pub py_version: Option<(u32, u32)>,
    pub cache: Option<&'a Cache>,
    /// In diff mode, whether a file changed: the phase runs over every file
    /// and only the findings that concern a changed one are kept.
    pub changed: Option<&'a (dyn Fn(&Path) -> bool + Sync)>,
}

/// The findings of the project rules over `files` (path and report name,
/// in report-name order), the whole tree even in diff mode, then reduced to
/// the changed files: a finding reported at one, or a cycle or a duplicate
/// with one among its members (it stays reported where a full run puts it).
pub fn run(params: &Params<'_>, files: &[(PathBuf, String)]) -> Vec<Finding> {
    let (mut out, units) = all(params, files);
    if let Some(changed) = params.changed {
        let displays: BTreeSet<&str> = files
            .iter()
            .filter(|(path, _)| changed(path))
            .map(|(_, display)| display.as_str())
            .collect();
        let modules: BTreeSet<&str> = units
            .iter()
            .filter(|unit| displays.contains(unit.display.as_str()))
            .map(|unit| unit.module.as_str())
            .collect();
        out.retain(|(display, diagnostic)| match diagnostic.code.as_str() {
            CYCLIC => diagnostic
                .message
                .trim_start_matches("Cyclic import (")
                .trim_end_matches(')')
                .split(" -> ")
                .any(|name| modules.contains(name)),
            // The `==module:[start:end]` lines name the files.
            DUPLICATE => diagnostic
                .message
                .lines()
                .filter_map(|line| line.strip_prefix("==")?.rsplit_once(":["))
                .any(|(name, _)| modules.contains(name)),
            _ => displays.contains(display.as_str()),
        });
    }
    out.into_iter()
        .map(|(display, diagnostic)| finding(params.registry, display, diagnostic))
        .collect()
}

type Raw = Vec<(String, ferrolint_core::Diagnostic)>;

#[allow(clippy::too_many_lines)]
fn all(params: &Params<'_>, files: &[(PathBuf, String)]) -> (Raw, Vec<Unit>) {
    let enabled: Vec<BTreeSet<String>> = files
        .iter()
        .map(|(path, _)| params.selection.for_file(path))
        .collect();
    let imports = [IMPORT_ERROR, RELATIVE_BEYOND_TOP, NO_NAME]
        .iter()
        .any(|code| enabled.iter().any(|set| set.contains(*code)));
    let cyclic = params.selection.is_enabled(CYCLIC);
    let opts = Opts::new(params.options);
    let duplicates = params.selection.is_enabled(DUPLICATE) && opts.duplicates.min > 0;
    if !imports && !cyclic && !duplicates {
        return (Vec::new(), Vec::new());
    }
    let units = units(params, files, &opts, &enabled);
    // The listing order is not a file's content, so the key must name the anchor.
    let anchor = (duplicates || cyclic)
        .then(|| checked_last(arguments(params.cwd, params.arguments), &units))
        .flatten();
    let project_codes = [IMPORT_ERROR, RELATIVE_BEYOND_TOP, NO_NAME, DUPLICATE];
    let key = params.cache.map(|cache| {
        let hashes: Vec<String> = files
            .par_iter()
            .map(|(path, _)| {
                fs::read(path)
                    .map(|bytes| hex(&Sha256::digest(bytes)))
                    .unwrap_or_default()
            })
            .collect();
        let mut parts = vec!["project".to_owned(), format!("{:?}", params.py_version)];
        parts.push(format!("{:?}", params.options));
        parts.push(format!("{cyclic}{duplicates}"));
        parts.push(anchor.map(|u| u.display.clone()).unwrap_or_default());
        // The roots decide how an absolute import resolves, whatever the names.
        parts.push(format!("{:?}", roots(params.cwd, params.arguments, &opts)));
        for name in ["VIRTUAL_ENV", "PYTHONPATH"] {
            parts.push(std::env::var(name).unwrap_or_default());
        }
        for ((unit, hash), set) in units.iter().zip(&hashes).zip(&enabled) {
            let on: Vec<&str> = project_codes
                .iter()
                .copied()
                .filter(|code| set.contains(*code))
                .collect();
            parts.extend([
                unit.display.clone(),
                unit.module.clone(),
                hash.clone(),
                on.join(","),
            ]);
        }
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        cache.key(&parts)
    });
    if let Some((cache, key)) = params.cache.zip(key.as_deref())
        && let Some(entry) = cache.project(key)
        && entry
            .stamps
            .iter()
            .all(|(path, stamp)| stamp_of(Path::new(path)) == *stamp)
    {
        return (entry.findings, units);
    }

    let mut out: Raw = Vec::new();
    let mut touched = Vec::new();
    if imports || cyclic {
        let prepared = project_phase::prepare_all(&units);
        let linted = units
            .iter()
            .map(|u| (u.module.clone(), (u.path.clone(), u.is_package)))
            .collect();
        let environment = crate::native::environment();
        let known = environment.is_some();
        let project = Project::for_run(
            roots(params.cwd, params.arguments, &opts),
            linted,
            opts.extensions.clone(),
            environment,
        );
        let found =
            project_phase::analyse(&units, &prepared, &project, known, &opts, params.registry);
        for ((unit, diagnostics), set) in units.iter().zip(found.files).zip(&enabled) {
            out.extend(
                diagnostics
                    .into_iter()
                    .filter(|d| set.contains(&d.code))
                    .map(|d| (unit.display.clone(), d)),
            );
        }
        // Pylint reports these at the start of the file it checked last.
        if cyclic && let Some(last) = anchor {
            out.extend(
                found
                    .cycles
                    .iter()
                    .map(|cycle| (last.display.clone(), project_phase::cycle_diagnostic(cycle))),
            );
        }
        touched = project.touched();
    }
    if duplicates && let Some(last) = anchor {
        out.extend(
            project_phase::duplicates(&units, &opts, params.registry)
                .into_iter()
                .map(|d| (last.display.clone(), d)),
        );
    }
    if let Some((cache, key)) = params.cache.zip(key.as_deref()) {
        let stamps = touched
            .into_iter()
            .map(|path| (path.to_string_lossy().into_owned(), stamp_of(&path)))
            .collect();
        cache.store_project(
            key,
            &ProjectEntry {
                stamps,
                findings: out.clone(),
            },
        );
    }
    (out, units)
}

/// The file Pylint checks last, where it attaches R0801 and R0401: the one
/// `os.walk` reaches last in the arguments' trees (a directory's files, then
/// its subdirectories, in the order the file system lists them).
fn checked_last(arguments: Vec<PathBuf>, units: &[Unit]) -> Option<&Unit> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let mut subdirs = Vec::new();
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                subdirs.push(entry.path());
            } else {
                out.push(entry.path());
            }
        }
        for subdir in subdirs {
            walk(&subdir, out);
        }
    }
    let mut order = Vec::new();
    for argument in arguments {
        let real = fs::canonicalize(&argument).unwrap_or(argument);
        if real.is_file() {
            order.push(real);
        } else {
            walk(&real, &mut order);
        }
    }
    // A file the walk does not reach ranks first; if none is reached, the last by name.
    let rank: std::collections::HashMap<&PathBuf, usize> = order
        .iter()
        .enumerate()
        .map(|(at, p)| (p, at + 1))
        .collect();
    units
        .iter()
        .max_by_key(|unit| rank.get(&unit.path).copied().unwrap_or(0))
}

fn finding(registry: &Registry, path: String, diagnostic: ferrolint_core::Diagnostic) -> Finding {
    Finding {
        aliases: registry.aliases(&diagnostic.code),
        path,
        diagnostic,
    }
}

/// The search path roots Pylint adds for the arguments: the directory above
/// each one's top package, or the source root that holds it.
pub fn roots(cwd: &Path, paths: &[PathBuf], opts: &Opts) -> Vec<PathBuf> {
    let sources = source_roots(cwd, opts);
    let mut roots: Vec<PathBuf> = Vec::new();
    for argument in arguments(cwd, paths) {
        let root = project_phase::package_root(&argument, &sources);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn source_roots(cwd: &Path, opts: &Opts) -> Vec<PathBuf> {
    opts.source_roots
        .iter()
        .map(|root| {
            let path = cwd.join(root);
            fs::canonicalize(&path).unwrap_or(path)
        })
        .collect()
}

fn arguments(cwd: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    if paths.is_empty() {
        return vec![cwd.to_path_buf()];
    }
    paths.iter().map(|argument| cwd.join(argument)).collect()
}

/// The files with the module name Pylint gives each: relative to the root
/// of the argument that holds it, then to the other roots and the working
/// directory.
fn units(
    params: &Params<'_>,
    files: &[(PathBuf, String)],
    opts: &Opts,
    enabled: &[BTreeSet<String>],
) -> Vec<Unit> {
    let roots = roots(params.cwd, params.arguments, opts);
    let sources = source_roots(params.cwd, opts);
    let cwd = fs::canonicalize(params.cwd).unwrap_or_else(|_| params.cwd.to_path_buf());
    let arguments: Vec<(PathBuf, PathBuf, bool)> = arguments(params.cwd, params.arguments)
        .into_iter()
        .map(|argument| {
            let real = fs::canonicalize(&argument).unwrap_or(argument);
            let root = project_phase::package_root(&real, &sources);
            let namespace = real.is_dir() && !real.join("__init__.py").exists();
            (real, root, namespace)
        })
        .collect();
    files
        .iter()
        .zip(enabled)
        .map(|((path, display), set)| {
            let real = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let owner = arguments
                .iter()
                .find(|(argument, _, _)| real.starts_with(argument))
                .or(arguments.first());
            let mut bases: Vec<PathBuf> =
                owner.map(|(_, root, _)| root.clone()).into_iter().collect();
            bases.extend(
                roots
                    .iter()
                    .filter(|root| !bases.contains(root))
                    .cloned()
                    .collect::<Vec<_>>(),
            );
            bases.push(cwd.clone());
            let namespace = owner.is_some_and(|(_, _, namespace)| *namespace);
            let module =
                project_phase::module_name(&real, &bases, namespace).unwrap_or_else(|| {
                    real.file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
            Unit {
                is_package: real.file_stem().is_some_and(|stem| stem == "__init__"),
                path: real,
                display: display.clone(),
                module,
                duplicates: set.contains(DUPLICATE),
            }
        })
        .collect()
}

/// What a directory (its entries) or a file (its size and time) looks like.
pub(crate) fn stamp_of(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let mut names: Vec<String> = fs::read_dir(path)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| {
                    // A symlink needs the stat; any other entry says what it is.
                    let dir = entry.file_type().map_or_else(
                        |_| entry.path().is_dir(),
                        |kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()),
                    );
                    format!(
                        "{}{}",
                        entry.file_name().to_string_lossy(),
                        if dir { "/" } else { "" }
                    )
                })
                .collect();
            names.sort();
            hex(&Sha256::digest(names.join("\n").as_bytes()))
        }
        Ok(meta) => {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |since| since.as_nanos());
            format!("{}:{modified}", meta.len())
        }
        Err(_) => "absent".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_checked_last_is_the_walk_s_not_the_name_s() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        // By name `z.py` is last; the walk lists a directory's files first.
        let units: Vec<Unit> = ["sub/a.py", "z.py"]
            .iter()
            .map(|name| {
                fs::write(root.join(name), "").unwrap();
                Unit {
                    path: root.join(name),
                    display: (*name).to_owned(),
                    module: (*name).to_owned(),
                    is_package: false,
                    duplicates: true,
                }
            })
            .collect();
        assert_eq!(
            checked_last(vec![root.clone()], &units).unwrap().display,
            "sub/a.py"
        );
        // A file argument is checked where it stands: the last argument wins.
        let files = vec![root.join("z.py"), root.join("sub/a.py")];
        assert_eq!(checked_last(files, &units).unwrap().display, "sub/a.py");
    }
}
