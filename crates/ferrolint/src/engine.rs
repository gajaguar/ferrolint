//! One run: load the configuration and the plugins, select the files, lint
//! them in parallel (or replay them from the cache) and gather the findings
//! into one deterministic order.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use ferrolint_core::{Diagnostic, Finding, LineIndex, Registry, Severity, sort_findings};
use rayon::prelude::*;
use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_parser::Parsed;
use sha2::{Digest, Sha256};

use crate::cache::{Cache, Entry};
use crate::checks;
use crate::config::{self, Config};
use crate::discover::{self, Excludes};
use crate::error::{Error, Result};
use crate::host::{FileData, Host, LoadedPlugin, Outcome, PluginSource};
use crate::selection::Selection;
use crate::semantic::Model;
use crate::suppress::{self, Suppressions};
use crate::{ast, diff, wit};

/// What a run hands to its report.
pub struct Report {
    pub findings: Vec<Finding>,
    /// Every linted file, as reports show its path, in path order.
    pub files: Vec<String>,
    pub registry: Registry,
    /// The `--diff-base` ref, when the run linted only changed files.
    pub diff_base: Option<String>,
    pub stats: Stats,
}

impl Report {
    /// A plugin aborted on some file, so the run's verdict is incomplete.
    pub fn aborted(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.diagnostic.code == checks::PLUGIN_ABORTED)
    }
}

/// What a run did, for `--stats`.
#[derive(Debug, Default)]
pub struct Stats {
    pub files: usize,
    /// Files replayed from the cache without parsing.
    pub cached: usize,
    pub parsed: usize,
    pub plugin_runs: usize,
    pub compiles: usize,
}

/// What a run was asked to do.
pub struct Options {
    pub paths: Vec<PathBuf>,
    pub config: Option<PathBuf>,
    pub threads: usize,
    pub diff_base: Option<String>,
    pub cache: bool,
}

pub fn run(cwd: &Path, options: &Options) -> Result<Report> {
    let config = config::load(cwd, options.config.as_deref())?;
    let changed = match &options.diff_base {
        Some(reference) => Some(diff::changed_files(cwd, reference)?),
        None => None,
    };
    let plugins = read_plugins(&config)?;
    let cache = options.cache.then(|| Cache::new(cwd));
    let host = Host::load(plugins, cache.as_ref())?;
    let rules = checks::host_rules().into_iter().chain(
        host.plugins
            .iter()
            .flat_map(|plugin| plugin.rules.iter().cloned()),
    );
    let registry = Registry::with_owners(rules, &config.pylint_aliases)
        .map_err(|err| Error::config(err.to_string()))?;
    let selection = Selection::new(&registry, &config)?;
    let excludes = Excludes::new(&config)?;
    let mut files = discover::discover(cwd, &options.paths, &excludes)?;
    if let Some(changed) = &changed {
        files.retain(|file| {
            let file = fs::canonicalize(file).unwrap_or_else(|_| file.clone());
            changed.contains(&file)
        });
    }

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.threads)
        .build()
        .map_err(|err| Error::io(format!("cannot start worker threads: {err}")))?;
    let linter = Linter {
        selection: &selection,
        registry: &registry,
        host: &host,
        cache: cache.as_ref(),
        owners: owners_key(&config),
        counters: Counters::default(),
    };
    let displays: Vec<String> = files
        .iter()
        .map(|path| discover::display_path(cwd, path))
        .collect();
    let per_file: Vec<Result<Vec<Finding>>> = pool.install(|| {
        files
            .par_iter()
            .zip(&displays)
            .map(|(path, display)| linter.lint_file(path, display))
            .collect()
    });
    let mut findings = Vec::new();
    for result in per_file {
        findings.extend(result?);
    }
    sort_findings(&mut findings);
    // The same string order as the findings', which sort by path first.
    let mut files = displays;
    files.sort();
    let stats = Stats {
        files: files.len(),
        cached: linter.counters.cached.load(Ordering::Relaxed),
        parsed: linter.counters.parsed.load(Ordering::Relaxed),
        plugin_runs: linter.counters.plugin_runs.load(Ordering::Relaxed),
        compiles: host.compiles(),
    };
    Ok(Report {
        findings,
        files,
        registry,
        diff_base: options.diff_base.clone(),
        stats,
    })
}

/// Read every plugin and check it against its pinned SHA-256 before any
/// file is linted.
fn read_plugins(config: &Config) -> Result<Vec<PluginSource>> {
    config
        .plugins
        .iter()
        .map(|plugin| {
            let bytes = fs::read(&plugin.path).map_err(|err| {
                Error::plugin(format!("cannot read {}: {err}", plugin.path.display()))
            })?;
            let actual = hex(&Sha256::digest(&bytes));
            if actual != plugin.sha256 {
                return Err(Error::plugin(format!(
                    "{}: sha256 mismatch: the configuration pins {}, the file is {actual}",
                    plugin.path.display(),
                    plugin.sha256
                )));
            }
            Ok(PluginSource {
                path: plugin.path.clone(),
                bytes,
                sha256: actual,
            })
        })
        .collect()
}

/// The `pylint-aliases` table as one string, part of every cache key: it
/// decides which rule a suppression's alias silences.
fn owners_key(config: &Config) -> String {
    config
        .pylint_aliases
        .iter()
        .map(|(alias, code)| format!("{alias}={code}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

#[derive(Default)]
struct Counters {
    cached: AtomicUsize,
    parsed: AtomicUsize,
    plugin_runs: AtomicUsize,
}

struct Linter<'a> {
    selection: &'a Selection,
    registry: &'a Registry,
    host: &'a Host,
    cache: Option<&'a Cache>,
    owners: String,
    counters: Counters,
}

impl Linter<'_> {
    fn lint_file(&self, path: &Path, display: &str) -> Result<Vec<Finding>> {
        let bytes = fs::read(path)
            .map_err(|err| Error::io(format!("cannot read {}: {err}", path.display())))?;
        let enabled = self.selection.for_file(path);
        let wanted: Vec<usize> = self
            .host
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, plugin)| plugin.rules.iter().any(|rule| enabled.contains(&rule.code)))
            .map(|(position, _)| position)
            .collect();
        let key = self.cache.map(|_| {
            let configuration = format!(
                "{}|{}",
                enabled.iter().cloned().collect::<Vec<_>>().join(","),
                self.owners
            );
            Cache::key(&hex(&Sha256::digest(&bytes)), &configuration)
        });
        let stored = self
            .cache
            .zip(key.as_deref())
            .and_then(|(cache, key)| cache.entry(key));
        let complete = stored.as_ref().is_some_and(|entry| {
            entry.unparsable
                || wanted.iter().all(|&position| {
                    entry
                        .plugins
                        .contains_key(&self.host.plugins[position].sha256)
                })
        });
        let (entry, aborts) = match stored {
            Some(entry) if complete => {
                self.counters.cached.fetch_add(1, Ordering::Relaxed);
                (entry, Vec::new())
            }
            stored => {
                self.counters.parsed.fetch_add(1, Ordering::Relaxed);
                let (entry, aborts) = self.check(path, display, bytes, &wanted, stored);
                if let (Some(cache), Some(key)) = (self.cache, key.as_deref()) {
                    cache.store_entry(key, &entry);
                }
                (entry, aborts)
            }
        };
        let mut diagnostics = entry.host;
        for &position in &wanted {
            if let Some(reported) = entry.plugins.get(&self.host.plugins[position].sha256) {
                diagnostics.extend(reported.iter().cloned());
            }
        }
        let suppressions = Suppressions::resolve(&entry.suppressions, self.registry);
        diagnostics.retain(|diagnostic| !suppressions.suppresses(diagnostic));
        diagnostics.extend(aborts);
        Ok(self.place(display, diagnostics, &enabled))
    }

    /// Parse the file and run the wanted plugins a stored entry lacks. An
    /// aborted plugin's output is not stored, so the next run retries it.
    fn check(
        &self,
        path: &Path,
        display: &str,
        bytes: Vec<u8>,
        wanted: &[usize],
        stored: Option<Entry>,
    ) -> (Entry, Vec<Diagnostic>) {
        let mut entry = stored.unwrap_or_default();
        let Ok(source) = String::from_utf8(bytes) else {
            entry.host = vec![checks::not_utf8()];
            entry.suppressions = BTreeMap::new();
            entry.unparsable = true;
            return (entry, Vec::new());
        };
        let parsed = ruff_python_parser::parse_unchecked_source(&source, PySourceType::Python);
        let index = LineIndex::new(&source);
        entry.host = checks::check(&source, &parsed, &index);
        entry.suppressions = suppress::spelled(&source, parsed.tokens(), &index);
        entry.unparsable = parsed.has_syntax_errors();
        let missing: Vec<usize> = wanted
            .iter()
            .copied()
            .filter(|&position| {
                !entry
                    .plugins
                    .contains_key(&self.host.plugins[position].sha256)
            })
            .collect();
        let mut aborts = Vec::new();
        if parsed.has_no_syntax_errors() && !missing.is_empty() {
            let context = PluginContext {
                path,
                display,
                source: &source,
                index: &index,
                parsed: &parsed,
            };
            for (position, outcome) in self.run_plugins(&context, &missing) {
                let plugin = &self.host.plugins[position];
                match outcome {
                    Ok(diagnostics) => {
                        entry.plugins.insert(plugin.sha256.clone(), diagnostics);
                    }
                    Err(abort) => aborts.push(abort),
                }
            }
        }
        (entry, aborts)
    }

    /// Run the plugins at `positions`, in configuration order, on one file:
    /// each one's diagnostics, or the abort it ended in.
    fn run_plugins(
        &self,
        context: &PluginContext<'_>,
        positions: &[usize],
    ) -> Vec<(usize, std::result::Result<Vec<Diagnostic>, Diagnostic>)> {
        let nodes = ast::flatten(context.source, context.index, context.parsed.syntax());
        let model = Model::build(
            context.parsed.syntax(),
            &nodes,
            context.source,
            context.index,
        );
        let data = Arc::new(FileData { nodes, model });
        let file = wit::File {
            path: context.display.to_owned(),
            module: module_name(context.path),
            source: context.source.to_owned(),
        };
        positions
            .iter()
            .map(|&position| {
                let plugin = &self.host.plugins[position];
                self.counters.plugin_runs.fetch_add(1, Ordering::Relaxed);
                let outcome = match self.host.check(position, &file, &data) {
                    Outcome::Diagnostics(reported) => {
                        accept(plugin, reported, context.source, context.index)
                            .map_err(|reason| aborted(plugin, &reason))
                    }
                    Outcome::Aborted(reason) => Err(aborted(plugin, &reason)),
                };
                (position, outcome)
            })
            .collect()
    }

    /// Keep the diagnostics whose code reports for this file and attach the
    /// path and aliases a report needs. A plugin abort always reports.
    fn place(
        &self,
        display: &str,
        diagnostics: Vec<Diagnostic>,
        enabled: &BTreeSet<String>,
    ) -> Vec<Finding> {
        diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code == checks::PLUGIN_ABORTED || enabled.contains(&diagnostic.code)
            })
            .map(|diagnostic| Finding {
                path: display.to_owned(),
                aliases: self.registry.aliases(&diagnostic.code),
                diagnostic,
            })
            .collect()
    }
}

/// What a plugin run needs of the file.
struct PluginContext<'a> {
    path: &'a Path,
    display: &'a str,
    source: &'a str,
    index: &'a LineIndex,
    parsed: &'a Parsed<ModModule>,
}

/// The dotted module name Python imports a file under: its stem, prefixed
/// by every enclosing directory that is a package (holds `__init__.py`).
/// A package's own `__init__.py` is named after the package.
fn module_name(path: &Path) -> String {
    let mut parts = Vec::new();
    if let Some(stem) = path.file_stem().filter(|stem| *stem != "__init__") {
        parts.push(stem.to_string_lossy().into_owned());
    }
    let mut directory = path.parent();
    while let Some(dir) = directory.filter(|dir| dir.join("__init__.py").is_file()) {
        let Some(name) = dir.file_name() else { break };
        parts.push(name.to_string_lossy().into_owned());
        directory = dir.parent();
    }
    parts.reverse();
    parts.join(".")
}

/// Check a plugin's diagnostics against its declared rules and recompute
/// every position from its byte offsets, so a plugin cannot report a
/// line or column that does not match its range.
fn accept(
    plugin: &LoadedPlugin,
    reported: Vec<wit::Diagnostic>,
    source: &str,
    index: &LineIndex,
) -> std::result::Result<Vec<Diagnostic>, String> {
    reported
        .into_iter()
        .map(|diagnostic| {
            if !plugin.rules.iter().any(|rule| rule.code == diagnostic.code) {
                return Err(format!(
                    "reported the undeclared code {:?}",
                    diagnostic.code
                ));
            }
            Ok(Diagnostic {
                code: diagnostic.code,
                message: diagnostic.message,
                range: range(source, index, &diagnostic.range),
                severity: crate::host::severity(diagnostic.severity),
                fix: diagnostic.fix.map(|fix| ferrolint_core::Fix {
                    description: fix.description,
                    replacement: fix.replacement,
                    range: range(source, index, &fix.range),
                }),
            })
        })
        .collect()
}

fn range(source: &str, index: &LineIndex, range: &wit::Range) -> ferrolint_core::Range {
    let start = boundary(source, range.start.offset);
    let end = boundary(source, range.end.offset).max(start);
    index.range(source, start, end)
}

/// Clamp an offset into the source and back to a character boundary.
fn boundary(source: &str, offset: u32) -> u32 {
    let mut offset = (offset as usize).min(source.len());
    while !source.is_char_boundary(offset) {
        offset -= 1;
    }
    u32::try_from(offset).unwrap_or(u32::MAX)
}

fn aborted(plugin: &LoadedPlugin, reason: &str) -> Diagnostic {
    let start = ferrolint_core::Position {
        offset: 0,
        line: 1,
        column: 1,
    };
    Diagnostic {
        code: checks::PLUGIN_ABORTED.into(),
        message: format!("plugin {} aborted on this file: {reason}", plugin.name),
        range: ferrolint_core::Range { start, end: start },
        severity: Severity::Error,
        fix: None,
    }
}

#[cfg(test)]
mod tests {
    use super::module_name;

    #[test]
    fn module_names_follow_the_packages() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("pkg/sub")).unwrap();
        for init in ["pkg/__init__.py", "pkg/sub/__init__.py"] {
            std::fs::write(src.join(init), "").unwrap();
        }
        assert_eq!(module_name(&src.join("pkg/sub/mod.py")), "pkg.sub.mod");
        assert_eq!(module_name(&src.join("pkg/__init__.py")), "pkg");
        assert_eq!(module_name(&src.join("top.py")), "top");
    }
}
