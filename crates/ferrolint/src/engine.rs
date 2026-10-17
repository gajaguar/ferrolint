//! One run: load the configuration and the plugins, select the files, lint
//! them in parallel (or replay them from the cache) and gather the findings
//! into one deterministic order.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::{Duration, Instant};

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
    /// Milliseconds from process start to the first parse.
    pub first_parse_ms: Option<f64>,
    /// The share of the workers' time spent on files once the plugins
    /// were ready.
    pub parallelism: f64,
    /// Workers that linted no file.
    pub idle_workers: usize,
    pub peak_rss_mb: Option<u64>,
}

/// What a run was asked to do.
pub struct Options {
    pub paths: Vec<PathBuf>,
    pub config: Option<PathBuf>,
    pub threads: usize,
    pub diff_base: Option<String>,
    pub cache: bool,
    /// When the process started, for the first-parse figure.
    pub started: Instant,
}

pub fn run(cwd: &Path, options: &Options) -> Result<Report> {
    let config = config::load(cwd, options.config.as_deref())?;
    let changed = match &options.diff_base {
        Some(reference) => Some(diff::changed_files(cwd, reference)?),
        None => None,
    };
    let plugins = read_plugins(&config)?;
    let cache = options.cache.then(|| Cache::new(cwd));
    let workers = if options.threads == 0 {
        std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
    } else {
        options.threads
    };
    let excludes = Excludes::new(&config)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(workers)
        .build()
        .map_err(|err| Error::io(format!("cannot start worker threads: {err}")))?;

    // Plugins compile on their own thread while the files are discovered
    // and the first ones parse; a file waits for them only to run them.
    let setup: OnceLock<Result<Setup>> = OnceLock::new();
    let loaded_at: OnceLock<Instant> = OnceLock::new();
    let linter = Linter {
        setup: &setup,
        cache: cache.as_ref(),
        counters: Counters::new(workers),
        started: options.started,
    };
    let (displays, per_file, lint_time) = std::thread::scope(|scope| {
        scope.spawn(|| {
            // A panic must still fill the cell, or every waiting worker hangs.
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Setup::new(plugins, cache.as_ref(), workers, &config)
            }))
            .unwrap_or_else(|_| Err(Error::plugin("loading the plugins panicked")));
            let _ = setup.set(loaded);
            let _ = loaded_at.set(Instant::now());
        });
        // The walk feeds the workers as it goes.
        let (sender, receiver) = mpsc::channel();
        let walker = scope.spawn(move || {
            discover::stream(cwd, &options.paths, &excludes, |file| {
                let keep = changed.as_ref().is_none_or(|changed| {
                    changed.contains(&file) || changed.contains(&resolved_parent(&file))
                });
                if keep {
                    let _ = sender.send(file);
                }
            })
        });
        let mut per_file: Vec<(String, Result<Vec<Finding>>)> = pool.install(|| {
            receiver
                .into_iter()
                .par_bridge()
                .map(|path| {
                    let display = discover::display_path(cwd, &path);
                    let findings = linter.lint_file(&path, &display);
                    (display, findings)
                })
                .collect()
        });
        // Files finish in any order; the first error reported is the one
        // of the first path, whatever the schedule.
        per_file.sort_by(|a, b| a.0.cmp(&b.0));
        let ended = Instant::now();
        let _ = setup.wait();
        walker
            .join()
            .unwrap_or_else(|_| Err(Error::io("the file walk panicked")))?;
        let (displays, per_file): (Vec<String>, Vec<Result<Vec<Finding>>>) =
            per_file.into_iter().unzip();
        // Parallelism counts from the moment the plugins are ready.
        let lint_time = loaded_at
            .get()
            .map_or(Duration::ZERO, |at| ended.saturating_duration_since(*at));
        Ok::<_, Error>((displays, per_file, lint_time))
    })?;
    let compiles = setup
        .get()
        .and_then(|setup| setup.as_ref().ok())
        .map_or(0, |setup| setup.host.compiles());
    let stats = linter.counters.stats(displays.len(), compiles, lint_time);
    drop(linter);
    let Setup { registry, .. } = setup
        .into_inner()
        .unwrap_or_else(|| Err(Error::plugin("plugin loading did not finish")))?;
    let mut findings = Vec::new();
    for result in per_file {
        findings.extend(result?);
    }
    sort_findings(&mut findings);
    // The same string order as the findings', which sort by path first.
    let mut displays = displays;
    displays.sort();
    Ok(Report {
        findings,
        files: displays,
        registry,
        diff_base: options.diff_base.clone(),
        stats,
    })
}

/// What linting needs from the plugins: the loaded plugins, the rule
/// table their rules complete, and the selection it resolves.
struct Setup {
    host: Host,
    registry: Registry,
    selection: Selection,
    py_version: Option<(u32, u32)>,
}

impl Setup {
    fn new(
        plugins: Vec<PluginSource>,
        cache: Option<&Cache>,
        workers: usize,
        config: &Config,
    ) -> Result<Self> {
        let host = Host::load(plugins, cache, workers)?;
        let rules = checks::host_rules().into_iter().chain(
            host.plugins
                .iter()
                .flat_map(|plugin| plugin.rules.iter().cloned()),
        );
        let registry = Registry::with_owners(rules, &config.pylint_aliases)
            .map_err(|err| Error::config(err.to_string()))?;
        let selection = Selection::new(&registry, config)?;
        Ok(Self {
            host,
            registry,
            selection,
            py_version: config.py_version,
        })
    }
}

include!(concat!(env!("OUT_DIR"), "/bundled.rs"));

/// The bundled plugins, then the configured ones, each checked against its
/// pinned SHA-256 before any file is linted.
fn read_plugins(config: &Config) -> Result<Vec<PluginSource>> {
    let bundled = BUNDLED.iter().map(|(name, bytes)| PluginSource {
        path: Path::new("<bundled>").join(name),
        bytes: bytes.to_vec(),
        sha256: hex(&Sha256::digest(bytes)),
    });
    let configured = config
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
        .collect::<Result<Vec<_>>>()?;
    Ok(bundled.chain(configured).collect())
}

/// The path with its directory's symlinks resolved and its own name kept,
/// as git lists a file under the repository's real root.
fn resolved_parent(file: &Path) -> PathBuf {
    match (file.parent().map(fs::canonicalize), file.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => file.to_path_buf(),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

struct Counters {
    cached: AtomicUsize,
    parsed: AtomicUsize,
    plugin_runs: AtomicUsize,
    /// Nanoseconds from process start to the first parse, or `u64::MAX`.
    first_parse: AtomicU64,
    /// Nanoseconds each worker spent on files.
    busy: Vec<AtomicU64>,
}

impl Counters {
    fn new(workers: usize) -> Self {
        Self {
            cached: AtomicUsize::new(0),
            parsed: AtomicUsize::new(0),
            plugin_runs: AtomicUsize::new(0),
            first_parse: AtomicU64::new(u64::MAX),
            busy: (0..workers).map(|_| AtomicU64::new(0)).collect(),
        }
    }

    // The figures are for reading; a float's precision is plenty.
    #[allow(clippy::cast_precision_loss)]
    fn stats(&self, files: usize, compiles: usize, lint_time: Duration) -> Stats {
        let busy: Vec<u64> = self
            .busy
            .iter()
            .map(|nanos| nanos.load(Ordering::Relaxed))
            .collect();
        let capacity = lint_time.as_nanos() as f64 * busy.len() as f64;
        let first_parse = self.first_parse.load(Ordering::Relaxed);
        Stats {
            files,
            cached: self.cached.load(Ordering::Relaxed),
            parsed: self.parsed.load(Ordering::Relaxed),
            plugin_runs: self.plugin_runs.load(Ordering::Relaxed),
            compiles,
            first_parse_ms: (first_parse != u64::MAX).then(|| first_parse as f64 / 1e6),
            parallelism: if capacity > 0.0 {
                busy.iter().sum::<u64>() as f64 / capacity
            } else {
                0.0
            },
            idle_workers: busy.iter().filter(|nanos| **nanos == 0).count(),
            peak_rss_mb: peak_rss_mb(),
        }
    }
}

/// The process's peak resident memory, where the system reports it.
fn peak_rss_mb() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let kilobytes: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kilobytes / 1024)
}

struct Linter<'a> {
    setup: &'a OnceLock<Result<Setup>>,
    cache: Option<&'a Cache>,
    counters: Counters,
    started: Instant,
}

/// A file read and parsed, before any plugin runs.
enum Prepared {
    NotUtf8,
    Source {
        source: String,
        parsed: Parsed<ModModule>,
        index: LineIndex,
    },
}

impl Linter<'_> {
    /// Count `time` as busy for the current worker.
    fn busy(&self, time: Duration) {
        if let Some(slot) =
            rayon::current_thread_index().and_then(|index| self.counters.busy.get(index))
        {
            let nanos = u64::try_from(time.as_nanos()).unwrap_or(u64::MAX);
            slot.fetch_add(nanos, Ordering::Relaxed);
        }
    }

    /// Decode and parse a file, noting when the run's first parse began.
    fn prepare(&self, bytes: &[u8]) -> Prepared {
        let since_start = u64::try_from(self.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.counters
            .first_parse
            .fetch_min(since_start, Ordering::Relaxed);
        let Ok(source) = std::str::from_utf8(bytes) else {
            return Prepared::NotUtf8;
        };
        let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
        Prepared::Source {
            source: source.to_owned(),
            parsed,
            index: LineIndex::new(source),
        }
    }

    /// Lint one file; the time after the plugins are ready counts as busy.
    fn lint_file(&self, path: &Path, display: &str) -> Result<Vec<Finding>> {
        let bytes = fs::read(path)
            .map_err(|err| Error::io(format!("cannot read {}: {err}", path.display())))?;
        // While the plugins still load, parse ahead instead of waiting.
        let early = self.setup.get().is_none().then(|| self.prepare(&bytes));
        let Ok(setup) = self.setup.wait() else {
            return Ok(Vec::new());
        };
        let resumed = Instant::now();
        let findings = self.lint_loaded(setup, path, display, &bytes, early);
        self.busy(resumed.elapsed());
        findings
    }

    fn lint_loaded(
        &self,
        setup: &Setup,
        path: &Path,
        display: &str,
        bytes: &[u8],
        early: Option<Prepared>,
    ) -> Result<Vec<Finding>> {
        let enabled = setup.selection.for_file(path);
        let wanted: Vec<usize> = setup
            .host
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, plugin)| plugin.rules.iter().any(|rule| enabled.contains(&rule.code)))
            .map(|(position, _)| position)
            .collect();
        let module = module_name(path);
        let key = self.cache.map(|cache| {
            let rules = enabled.iter().cloned().collect::<Vec<_>>().join(",");
            let version = format!("{:?}", setup.py_version);
            cache.key(&[
                &hex(&Sha256::digest(bytes)),
                display,
                &module,
                &rules,
                &version,
            ])
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
                        .contains_key(&setup.host.plugins[position].sha256)
                })
        });
        let (entry, aborts) = match stored {
            Some(entry) if complete => {
                self.counters.cached.fetch_add(1, Ordering::Relaxed);
                (entry, Vec::new())
            }
            stored => {
                self.counters.parsed.fetch_add(1, Ordering::Relaxed);
                let prepared = early.unwrap_or_else(|| self.prepare(bytes));
                let file = FileRef {
                    display,
                    module: &module,
                };
                let (entry, aborts) = self.check(setup, &file, prepared, &wanted, stored)?;
                if let (Some(cache), Some(key)) = (self.cache, key.as_deref()) {
                    cache.store_entry(key, &entry);
                }
                (entry, aborts)
            }
        };
        let mut diagnostics = entry.host;
        for &position in &wanted {
            if let Some(reported) = entry.plugins.get(&setup.host.plugins[position].sha256) {
                diagnostics.extend(reported.iter().cloned());
            }
        }
        let suppressions = Suppressions::resolve(&entry.suppressions, &setup.registry);
        diagnostics.retain(|diagnostic| !suppressions.suppresses(diagnostic));
        diagnostics.extend(aborts);
        Ok(place(&setup.registry, display, diagnostics, &enabled))
    }

    /// Check a parsed file and run the wanted plugins a stored entry lacks.
    /// An aborted plugin's output is not stored, so the next run retries it.
    fn check(
        &self,
        setup: &Setup,
        file: &FileRef<'_>,
        prepared: Prepared,
        wanted: &[usize],
        stored: Option<Entry>,
    ) -> Result<(Entry, Vec<Diagnostic>)> {
        let host = &setup.host;
        let mut entry = stored.unwrap_or_default();
        // Keep only the outputs of the plugins this run loads.
        entry
            .plugins
            .retain(|sha256, _| host.plugins.iter().any(|plugin| plugin.sha256 == *sha256));
        let Prepared::Source {
            source,
            parsed,
            index,
        } = prepared
        else {
            entry.host = vec![checks::not_utf8()];
            entry.suppressions = Vec::new();
            entry.unparsable = true;
            return Ok((entry, Vec::new()));
        };
        entry.host = checks::check(&source, &parsed, &index);
        entry.suppressions = suppress::spelled(&source, &parsed, &index);
        entry.unparsable = parsed.has_syntax_errors();
        let missing: Vec<usize> = wanted
            .iter()
            .copied()
            .filter(|&position| !entry.plugins.contains_key(&host.plugins[position].sha256))
            .collect();
        let mut aborts = Vec::new();
        if parsed.has_no_syntax_errors() && !missing.is_empty() {
            let context = PluginContext {
                module: file.module,
                display: file.display,
                source: &source,
                index: &index,
                parsed: &parsed,
                py_version: setup.py_version,
            };
            for (position, outcome) in self.run_plugins(host, &context, &missing)? {
                let plugin = &host.plugins[position];
                match outcome {
                    Ok(diagnostics) => {
                        entry.plugins.insert(plugin.sha256.clone(), diagnostics);
                    }
                    Err(abort) => aborts.push(abort),
                }
            }
        }
        Ok((entry, aborts))
    }

    /// Run the plugins at `positions`, in configuration order, on one file:
    /// each one's diagnostics, or the abort it ended in.
    fn run_plugins(
        &self,
        host: &Host,
        context: &PluginContext<'_>,
        positions: &[usize],
    ) -> Result<Vec<(usize, PluginOutcome)>> {
        let flat = ast::flatten(context.source, context.index, context.parsed.syntax());
        let model = Model::build(
            context.parsed.syntax(),
            &flat.nodes,
            context.source,
            context.index,
            context.py_version,
        );
        let ast = flat.pack();
        let data = Arc::new(FileData {
            flat,
            ast,
            model,
            source: context.source.to_owned(),
        });
        let file = wit::File {
            path: context.display.to_owned(),
            module: context.module.to_owned(),
            source: context.source.to_owned(),
        };
        positions
            .iter()
            .map(|&position| {
                let plugin = &host.plugins[position];
                self.counters.plugin_runs.fetch_add(1, Ordering::Relaxed);
                let outcome = match host.check(position, &file, &data) {
                    Outcome::Diagnostics(reported) => {
                        accept(plugin, reported, context.source, context.index)
                            .map_err(|reason| aborted(plugin, &reason))
                    }
                    Outcome::Aborted(reason) => Err(aborted(plugin, &reason)),
                    Outcome::Broken(reason) => return Err(Error::plugin(reason)),
                };
                Ok((position, outcome))
            })
            .collect()
    }
}

/// Keep the diagnostics whose code reports for this file and attach the
/// path and aliases a report needs. A plugin abort always reports.
fn place(
    registry: &Registry,
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
            aliases: registry.aliases(&diagnostic.code),
            diagnostic,
        })
        .collect()
}

/// How a file is named to the plugins.
struct FileRef<'a> {
    display: &'a str,
    module: &'a str,
}

/// A plugin's diagnostics on a file, or the `FL001` abort it ended in.
type PluginOutcome = std::result::Result<Vec<Diagnostic>, Diagnostic>;

/// What a plugin run needs of the file.
struct PluginContext<'a> {
    module: &'a str,
    display: &'a str,
    source: &'a str,
    index: &'a LineIndex,
    parsed: &'a Parsed<ModModule>,
    py_version: Option<(u32, u32)>,
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
