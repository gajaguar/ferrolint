//! One run: load the configuration and the plugins, select the files, lint
//! them in parallel (or replay them from the cache) and gather the findings
//! into one deterministic order.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, mpsc};
use std::time::{Duration, Instant};

use ferrolint_core::{
    Diagnostic, Finding, LineIndex, Registry, RegistryError, RuleInfo, Severity, sort_findings,
};
use rayon::prelude::*;
use ruff_python_ast::{ModModule, PySourceType, PythonVersion};
use ruff_python_parser::{ParseOptions, Parsed};
use sha2::{Digest, Sha256};

use crate::cache::{Cache, Entry};
use crate::checks;
use crate::config::{self, Config};
use crate::discover::{self, Excludes};
use crate::error::{Error, Result};
use crate::host::{FileData, Host, LoadedPlugin, Outcome, PluginSource};
use crate::native::{self, project_phase};
use crate::project_run;
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

/// What one file gave: its findings, or why it could not be linted.
type FileResult = Result<Vec<Finding>>;

#[allow(clippy::too_many_lines)]
pub fn run(cwd: &Path, options: &Options) -> Result<Report> {
    let config = config::load(cwd, options.config.as_deref())?;
    let changed = match &options.diff_base {
        Some(reference) => Some(diff::changed_files(cwd, reference)?),
        None => None,
    };
    let is_changed = |file: &Path| {
        changed.as_ref().is_none_or(|changed| {
            changed.contains(file) || changed.contains(&resolved_parent(file))
        })
    };
    // Diff mode: the project phase still covers the whole tree.
    let tree = Mutex::new(Vec::new());
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
    let (displays, names, per_file, lint_time) = std::thread::scope(|scope| {
        scope.spawn(|| {
            // A panic must still fill the cell, or every waiting worker hangs.
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Setup::new(
                    plugins,
                    cache.as_ref(),
                    workers,
                    &config,
                    cwd,
                    &options.paths,
                )
            }))
            .unwrap_or_else(|_| Err(Error::plugin("loading the plugins panicked")));
            let _ = setup.set(loaded);
            let _ = loaded_at.set(Instant::now());
        });
        // The walk feeds the workers as it goes.
        let (sender, receiver) = mpsc::channel();
        let (diff, tree) = (changed.is_some(), &tree);
        let walker = scope.spawn(move || {
            discover::stream(cwd, &options.paths, &excludes, |file| {
                if !diff || is_changed(&file) {
                    let _ = sender.send(file.clone());
                }
                if diff {
                    tree.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(file);
                }
            })
        });
        let mut per_file: Vec<(String, PathBuf, Result<Vec<Finding>>)> = pool.install(|| {
            receiver
                .into_iter()
                .par_bridge()
                .map(|path| {
                    let display = discover::display_path(cwd, &path);
                    let findings = linter.lint_file(&path, &display);
                    (display, path, findings)
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
        let (names, per_file): (Vec<(PathBuf, String)>, Vec<FileResult>) = per_file
            .into_iter()
            .map(|(display, path, findings)| ((path, display), findings))
            .unzip();
        let displays: Vec<String> = names.iter().map(|(_, display)| display.clone()).collect();
        // Parallelism counts from the moment the plugins are ready.
        let lint_time = loaded_at
            .get()
            .map_or(Duration::ZERO, |at| ended.saturating_duration_since(*at));
        Ok::<_, Error>((displays, names, per_file, lint_time))
    })?;
    let compiles = setup
        .get()
        .and_then(|setup| setup.as_ref().ok())
        .map_or(0, |setup| setup.host.compiles());
    let stats = linter.counters.stats(displays.len(), compiles, lint_time);
    drop(linter);
    let Setup {
        registry,
        selection,
        py_version,
        options: rule_options,
        ..
    } = setup
        .into_inner()
        .unwrap_or_else(|| Err(Error::plugin("plugin loading did not finish")))?;
    let mut findings = Vec::new();
    for result in per_file {
        findings.extend(result?);
    }
    findings.extend(config_findings(cwd, &config, &registry, &selection));
    let names = if changed.is_some() {
        let mut all: Vec<(PathBuf, String)> = tree
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
            .into_iter()
            .map(|path| {
                let display = discover::display_path(cwd, &path);
                (path, display)
            })
            .collect();
        all.sort_by(|a, b| a.1.cmp(&b.1));
        all
    } else {
        names
    };
    findings.extend(project_run::run(
        &project_run::Params {
            cwd,
            arguments: &options.paths,
            registry: &registry,
            selection: &selection,
            options: &rule_options,
            py_version,
            cache: cache.as_ref(),
            changed: changed.is_some().then_some(&is_changed),
        },
        &names,
    ));
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

/// What Pylint would report about its own configuration file, for the rules
/// the selection enables and the message names the registry does not know.
fn config_findings(
    cwd: &Path,
    config: &Config,
    registry: &Registry,
    selection: &Selection,
) -> Vec<Finding> {
    let Some(file) = &config.pylint_file else {
        return Vec::new();
    };
    let path = discover::display_path(cwd, file);
    config
        .issues
        .iter()
        .filter(|issue| {
            let known = issue
                .name
                .as_deref()
                .is_some_and(|name| registry.resolve(name).is_some());
            !known && selection.is_enabled(issue.code)
        })
        .map(|issue| Finding {
            path: path.clone(),
            aliases: registry.aliases(issue.code),
            diagnostic: checks::config_diagnostic(issue.code, &issue.message),
        })
        .collect()
}

/// What linting needs from the plugins: the loaded plugins, the rule
/// table their rules complete, and the selection it resolves.
struct Setup {
    host: Host,
    registry: Registry,
    selection: Selection,
    py_version: Option<(u32, u32)>,
    /// The configuration's other options, sorted by key, as the plugins get them.
    options: Vec<(String, String)>,
    /// Where an absolute import is looked up, as the project phase does.
    roots: Vec<PathBuf>,
    opts: project_phase::Opts,
    /// The stamps taken so far: the files share the modules they read.
    stamps: Mutex<HashMap<PathBuf, String>>,
    /// The type engine, only when a rule that asks it is enabled and it
    /// starts (it does not in a virtualenv without `site-packages`).
    #[cfg(feature = "ty")]
    types: Option<Arc<crate::ty::Types>>,
}

impl Setup {
    /// The stamp of `path`, read once per run.
    fn stamp(&self, path: &Path) -> String {
        let mut known = self.stamps.lock().unwrap_or_else(PoisonError::into_inner);
        known
            .entry(path.to_path_buf())
            .or_insert_with(|| project_run::stamp_of(path))
            .clone()
    }

    fn new(
        plugins: Vec<PluginSource>,
        cache: Option<&Cache>,
        workers: usize,
        config: &Config,
        cwd: &Path,
        paths: &[PathBuf],
    ) -> Result<Self> {
        let host = Host::load(plugins, cache, workers)?;
        let plugin_rules: Vec<_> = host
            .plugins
            .iter()
            .flat_map(|plugin| plugin.rules.iter().cloned())
            .collect();
        let registry = registry_for(plugin_rules, config)?;
        let selection = Selection::new(&registry, config)?;
        let options: Vec<(String, String)> = config.options.clone().into_iter().collect();
        let opts = project_phase::Opts::new(&options);
        #[cfg(feature = "ty")]
        let types = type_engine(&selection, cwd, !host.plugins.is_empty());
        Ok(Self {
            host,
            registry,
            selection,
            py_version: config.py_version,
            roots: project_run::roots(cwd, paths, &opts),
            opts,
            options,
            stamps: Mutex::default(),
            #[cfg(feature = "ty")]
            types,
        })
    }
}

/// Start ty when an enabled rule needs it, and say so when it cannot.
#[cfg(feature = "ty")]
fn type_engine(selection: &Selection, cwd: &Path, plugins: bool) -> Option<Arc<crate::ty::Types>> {
    // C0103 and the plugins ask ty for the value of a name they cannot follow themselves.
    if !crate::ty::operations::rules()
        .iter()
        .any(|rule| selection.is_enabled(&rule.code))
        && !selection.is_enabled("E1101")
        && !selection.is_enabled("C0103")
        && !plugins
    {
        return None;
    }
    crate::ty::Types::new(cwd)
        .map(Arc::new)
        .map_err(|err| eprintln!("ferrolint: warning: {err}; the ty rules report nothing"))
        .ok()
}

/// The rule table: the host's rules, the native ones the configuration
/// reaches and the plugins'.
fn registry_for(plugin_rules: Vec<RuleInfo>, config: &Config) -> Result<Registry> {
    let load_plugins = config.options.get("load-plugins").map(String::as_str);
    let taken: HashSet<String> = plugin_rules
        .iter()
        .flat_map(|rule| std::iter::once(rule.code.clone()).chain(rule.aliases()))
        .collect();
    let native = native::rules_for(load_plugins, &taken);
    let rules = checks::host_rules()
        .into_iter()
        .chain(native.iter().cloned())
        .chain(plugin_rules);
    Registry::with_owners(rules, &config.pylint_aliases).map_err(|err| {
        // Only a loaded extension keeps its rules in a clash.
        let module = match &err {
            RegistryError::AliasIsCode { alias, .. } => native
                .iter()
                .find(|rule| rule.code == *alias)
                .and_then(native::extensions::needed),
            _ => None,
        };
        Error::config(match module {
            Some(module) => format!(
                "{err}; that native rule belongs to `pylint.extensions.{module}`, which load-plugins loads: \
                 drop the extension or give the plugin rule another msg-id"
            ),
            None => err.to_string(),
        })
    })
}

include!(concat!(env!("OUT_DIR"), "/bundled.rs"));

/// The bundled plugins, then the configured ones, each checked against its
/// pinned SHA-256 before any file is linted.
fn read_plugins(config: &Config) -> Result<Vec<PluginSource>> {
    let bundled = BUNDLED.iter().map(|(name, bytes)| PluginSource {
        path: Path::new("<bundled>").join(name),
        bytes: bytes.to_vec(),
        sha256: hex(&Sha256::digest(bytes)),
        options: Vec::new(),
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
                options: plugin.options.clone().into_iter().collect(),
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
        // The newest grammar: an older default would reject PEP 695 syntax and
        // silently skip the file.
        let options =
            ParseOptions::from(PySourceType::Python).with_target_version(PythonVersion::latest());
        let parsed = checks::parse(source, options);
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
            // A plugin's verdict depends on the options it is sent.
            let options = format!("{:?}", setup.options);
            // ... and on the options of its own `[[plugins]]` entry.
            let own: Vec<_> = wanted
                .iter()
                .map(|&position| &setup.host.plugins[position].options)
                .collect();
            let own = format!("{own:?}");
            // A star import binds what the search path finds.
            let search = format!(
                "{:?}{:?}{:?}",
                setup.roots,
                std::env::var("VIRTUAL_ENV"),
                std::env::var("PYTHONPATH")
            );
            // A run where ty did not start must not reuse a run where it did.
            // shortcut: ty's reads of other modules are not stamped (the native
            // rules' are, through `touch`), so a change in one does not
            // invalidate a ty result; use `--no-cache`.
            #[cfg(feature = "ty")]
            let ty = setup.types.is_some().to_string();
            #[cfg(not(feature = "ty"))]
            let ty = String::new();
            cache.key(&[
                &ty,
                &hex(&Sha256::digest(bytes)),
                display,
                &module,
                &rules,
                &version,
                &options,
                &own,
                &search,
            ])
        });
        let stored = self
            .cache
            .zip(key.as_deref())
            .and_then(|(cache, key)| cache.entry(key))
            .filter(|entry| {
                entry
                    .stamps
                    .iter()
                    .all(|(path, stamp)| setup.stamp(Path::new(path)) == *stamp)
            });
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
                    path,
                };
                let (entry, aborts) =
                    self.check(setup, &file, prepared, &wanted, &enabled, stored)?;
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
        // Pylint stops at a pragma that ends the file (I0013): only the messages
        // about the pragmas themselves, and a syntax error, survive it.
        if diagnostics.iter().any(|d| d.code == "I0013") {
            diagnostics.retain(|d| {
                d.code == checks::SYNTAX_ERROR
                    || checks::PRAGMA_MESSAGES.iter().any(|(id, _)| *id == d.code)
            });
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
        enabled: &BTreeSet<String>,
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
        let known = |name: &str| setup.registry.resolve(name).is_some();
        let plugin_loaded = |module: &str| {
            setup
                .options
                .iter()
                .find(|(key, _)| key == "load-plugins")
                .is_some_and(|(_, value)| value.split(',').any(|name| name.trim() == module))
        };
        native::take_reads();
        entry.host = checks::check(
            &source,
            &parsed,
            &index,
            file.module,
            &known,
            &plugin_loaded,
        );
        if parsed.has_no_syntax_errors() && native::wanted(enabled) {
            entry.host.extend(native_findings(
                setup, file, &source, &parsed, &index, enabled,
            ));
        }
        entry.stamps = native::take_reads()
            .into_iter()
            .map(|path| (path.to_string_lossy().into_owned(), setup.stamp(&path)))
            .collect();
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
                path: file.path,
                module: file.module,
                display: file.display,
                source: &source,
                index: &index,
                parsed: &parsed,
                py_version: setup.py_version,
                options: &setup.options,
                roots: &setup.roots,
                opts: &setup.opts,
                #[cfg(feature = "ty")]
                types: setup.types.as_ref(),
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
        let overridden = native::overridden_parameters(&native::Ctx {
            source: context.source,
            index: context.index,
            parsed: context.parsed,
            flat: &flat,
            model: &model,
            options: context.options,
            py_version: context.py_version,
            module: context.module,
            path: context.path,
            roots: context.roots,
            types: None,
        });
        // `listed_in_all` answers for a file without `__all__` too, but costs a project.
        let all_names = if context.source.contains("__all__") {
            native::Project::with_packages(context.path, context.module, context.roots)
                .listed_in_all(&context.parsed.syntax().body)
        } else {
            Some(Vec::new())
        };
        let data = Arc::new(FileData {
            overridden,
            all_names,
            #[cfg(feature = "ty")]
            types: context.types.cloned(),
            #[cfg(feature = "ty")]
            path: context.path.to_owned(),
            stars: star_names(&flat, context),
            flat,
            ast,
            model,
            source: context.source.to_owned(),
            comments: suppress::comments(context.source, context.parsed.tokens())
                .iter()
                .map(|comment| {
                    let range = context
                        .index
                        .range(context.source, comment.start, comment.end);
                    let position = |at: ferrolint_core::Position| wit::Position {
                        offset: at.offset,
                        line: at.line,
                        column: at.column,
                    };
                    wit::Comment {
                        range: wit::Range {
                            start: position(range.start),
                            end: position(range.end),
                        },
                        own_line: suppress::own_line(context.source, comment.start),
                    }
                })
                .collect(),
        });
        let file = wit::File {
            path: context.display.to_owned(),
            module: context.module.to_owned(),
            source: context.source.to_owned(),
            options: context.options.to_vec(),
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

/// The native rules' findings on one file, and ty's operations rules when it runs.
fn native_findings(
    setup: &Setup,
    file: &FileRef<'_>,
    source: &str,
    parsed: &Parsed<ModModule>,
    index: &LineIndex,
    enabled: &BTreeSet<String>,
) -> Vec<Diagnostic> {
    let flat = ast::flatten(source, index, parsed.syntax());
    let model = Model::build(
        parsed.syntax(),
        &flat.nodes,
        source,
        index,
        setup.py_version,
    );
    let ctx = native::Ctx {
        source,
        index,
        parsed,
        flat: &flat,
        model: &model,
        options: &setup.options,
        py_version: setup.py_version,
        module: file.module,
        path: file.path,
        roots: &setup.roots,
        types: None,
    };
    #[cfg(feature = "ty")]
    let mut found = setup
        .types
        .as_ref()
        .and_then(|types| {
            types.with_file(file.path, source, |db, program_file| {
                let values = crate::ty::values::Values::new(db, program_file);
                native::check(
                    &native::Ctx {
                        types: Some(&values),
                        ..ctx
                    },
                    enabled,
                )
            })
        })
        .unwrap_or_else(|| native::check(&ctx, enabled));
    #[cfg(not(feature = "ty"))]
    let found = native::check(&ctx, enabled);
    #[cfg(feature = "ty")]
    if let Some(types) = &setup.types
        && crate::ty::operations::wanted(enabled)
    {
        found.extend(crate::ty::operations::check(types, &ctx, enabled));
    }
    #[cfg(feature = "ty")]
    if let Some(types) = &setup.types
        && crate::ty::members::wanted(enabled)
    {
        found.extend(crate::ty::members::check(types, &ctx));
    }
    found
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
    path: &'a Path,
}

/// A plugin's diagnostics on a file, or the `FL001` abort it ended in.
type PluginOutcome = std::result::Result<Vec<Diagnostic>, Diagnostic>;

/// What a plugin run needs of the file.
struct PluginContext<'a> {
    path: &'a Path,
    module: &'a str,
    display: &'a str,
    source: &'a str,
    index: &'a LineIndex,
    parsed: &'a Parsed<ModModule>,
    py_version: Option<(u32, u32)>,
    options: &'a [(String, String)],
    roots: &'a [PathBuf],
    opts: &'a project_phase::Opts,
    #[cfg(feature = "ty")]
    types: Option<&'a Arc<crate::ty::Types>>,
}

/// What each `from m import *` of the file binds, by alias node, for the
/// plugins' `modules` query. A module the project cannot tell is left out.
fn star_names(flat: &ast::Flat, context: &PluginContext<'_>) -> HashMap<u32, Vec<String>> {
    let mut near = None;
    let mut run = None;
    let mut stars = HashMap::new();
    for (id, node) in (0..).zip(&flat.nodes) {
        if node.kind != wit::NodeKind::Alias || flat.string(node.name) != Some("*") {
            continue;
        }
        let import = &flat.nodes[node.parent.unwrap_or_default() as usize];
        let origin = flat.string(import.detail).unwrap_or_default();
        // An absolute import is looked up on the run's search path, as
        // astroid does; a relative one beside the file.
        let project = if origin.starts_with('.') {
            near.get_or_insert_with(|| {
                native::Project::new(context.path, context.module, context.roots)
            })
        } else {
            run.get_or_insert_with(|| {
                native::Project::for_run(
                    context.roots.to_vec(),
                    HashMap::new(),
                    context.opts.extensions.clone(),
                    native::environment(),
                )
            })
        };
        let origin = project.absolute(origin);
        // `*` among the names stands for a source that does not parse.
        if let Some(names) = project.public_names(&origin)
            && !names.iter().any(|name| name == "*")
        {
            stars.insert(id, names);
        } else if project.is_missing(&origin) {
            // As astroid, which cannot import it, the star binds nothing.
            stars.insert(id, Vec::new());
        }
    }
    stars
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
    use std::path::Path;

    use ferrolint_core::{RuleInfo, Severity};

    use super::{Config, module_name, registry_for};
    use crate::selection::Selection;

    /// A plugin rule that takes a docparams msg-id, as `pylint-gajaguar` does.
    fn gaj006() -> RuleInfo {
        RuleInfo {
            code: "GAJ006".into(),
            name: "gaj006".into(),
            summary: String::new(),
            severity: Severity::Warning,
            pylint_id: Some("W9006".into()),
            pylint_symbol: None,
            plugin: Some("gaj".into()),
        }
    }

    fn with_load_plugins(value: &str) -> Config {
        let mut config = Config::default();
        config
            .options
            .insert("load-plugins".into(), value.to_owned());
        config
    }

    #[test]
    fn a_plugin_owns_a_docparams_msg_id_while_the_extension_is_not_loaded() {
        let config = Config {
            select: Some(vec!["W9006".into()]),
            ..with_load_plugins("pylint.extensions.mccabe")
        };
        let registry = registry_for(vec![gaj006()], &config).unwrap();
        assert_eq!(registry.resolve("W9006"), Some("GAJ006"));
        assert!(registry.get("W9006").is_none());
        assert!(registry.get("W9005").is_some(), "an unclaimed rule stays");
        let selection = Selection::new(&registry, &config).unwrap();
        let on: Vec<_> = selection.for_file(Path::new("x.py")).into_iter().collect();
        assert_eq!(on, ["GAJ006"]);
    }

    #[test]
    fn the_native_rule_answers_to_its_msg_id_without_a_clash() {
        for config in [
            Config::default(),
            with_load_plugins("pylint.extensions.docparams"),
        ] {
            let registry = registry_for(Vec::new(), &config).unwrap();
            assert_eq!(registry.resolve("W9006"), Some("W9006"));
        }
    }

    #[test]
    fn a_clash_with_a_loaded_extension_says_so() {
        let config = with_load_plugins("pylint.extensions.mccabe, pylint.extensions.docparams");
        let message = registry_for(vec![gaj006()], &config)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("GAJ006 declares W9006 as an alias")
                && message.contains("`pylint.extensions.docparams`, which load-plugins loads"),
            "{message}"
        );
    }

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
