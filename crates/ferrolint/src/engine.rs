//! One run: load the configuration and the plugins, select the files, lint
//! them in parallel and gather the findings into one deterministic order.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ferrolint_core::{Diagnostic, Finding, LineIndex, Registry, Severity, sort_findings};
use rayon::prelude::*;
use ruff_python_ast::{ModModule, PySourceType};
use ruff_python_parser::Parsed;
use sha2::{Digest, Sha256};

use crate::checks;
use crate::config::{self, Config};
use crate::discover::{self, Excludes};
use crate::error::{Error, Result};
use crate::host::{FileData, Host, LoadedPlugin, Outcome};
use crate::selection::Selection;
use crate::semantic::Model;
use crate::suppress::Suppressions;
use crate::{ast, wit};

/// What a run hands to its report.
pub struct Report {
    pub findings: Vec<Finding>,
    pub files: usize,
}

impl Report {
    /// A plugin aborted on some file, so the run's verdict is incomplete.
    pub fn aborted(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.diagnostic.code == checks::PLUGIN_ABORTED)
    }
}

/// What a run was asked to do.
pub struct Options {
    pub paths: Vec<PathBuf>,
    pub config: Option<PathBuf>,
    pub threads: usize,
}

pub fn run(cwd: &Path, options: &Options) -> Result<Report> {
    let config = config::load(cwd, options.config.as_deref())?;
    let plugin_bytes = read_plugins(&config)?;
    let host = Host::load(plugin_bytes)?;
    let rules = checks::host_rules().into_iter().chain(
        host.plugins
            .iter()
            .flat_map(|plugin| plugin.rules.iter().cloned()),
    );
    let registry = Registry::with_owners(rules, &config.pylint_aliases)
        .map_err(|err| Error::config(err.to_string()))?;
    let selection = Selection::new(&registry, &config)?;
    let excludes = Excludes::new(&config)?;
    let files = discover::discover(cwd, &options.paths, &excludes)?;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.threads)
        .build()
        .map_err(|err| Error::io(format!("cannot start worker threads: {err}")))?;
    let linter = Linter {
        cwd,
        selection: &selection,
        registry: &registry,
        host: &host,
    };
    let per_file: Vec<Result<Vec<Finding>>> = pool.install(|| {
        files
            .par_iter()
            .map(|path| linter.lint_file(path))
            .collect()
    });
    let mut findings = Vec::new();
    for result in per_file {
        findings.extend(result?);
    }
    sort_findings(&mut findings);
    Ok(Report {
        findings,
        files: files.len(),
    })
}

/// Read every plugin and check it against its pinned SHA-256 before any
/// file is linted.
fn read_plugins(config: &Config) -> Result<Vec<(PathBuf, Vec<u8>)>> {
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
            Ok((plugin.path.clone(), bytes))
        })
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

struct Linter<'a> {
    cwd: &'a Path,
    selection: &'a Selection,
    registry: &'a Registry,
    host: &'a Host,
}

impl Linter<'_> {
    fn lint_file(&self, path: &Path) -> Result<Vec<Finding>> {
        let bytes = fs::read(path)
            .map_err(|err| Error::io(format!("cannot read {}: {err}", path.display())))?;
        let enabled = self.selection.for_file(path);
        let display = discover::display_path(self.cwd, path);
        let diagnostics = match String::from_utf8(bytes) {
            Ok(source) => self.check_source(path, &display, &source, &enabled),
            Err(_) => vec![checks::not_utf8()],
        };
        Ok(self.place(&display, diagnostics, &enabled))
    }

    fn check_source(
        &self,
        path: &Path,
        display: &str,
        source: &str,
        enabled: &BTreeSet<String>,
    ) -> Vec<Diagnostic> {
        let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
        let index = LineIndex::new(source);
        let mut diagnostics = checks::check(source, &parsed, &index);
        if parsed.has_no_syntax_errors() {
            diagnostics.extend(self.run_plugins(path, display, source, &index, &parsed, enabled));
        }
        let suppressions = Suppressions::new(source, parsed.tokens(), &index);
        diagnostics.retain(|diagnostic| !suppressions.suppresses(self.registry, diagnostic));
        diagnostics
    }

    /// Run, in configuration order, every plugin with a rule that reports
    /// for this file.
    fn run_plugins(
        &self,
        path: &Path,
        display: &str,
        source: &str,
        index: &LineIndex,
        parsed: &Parsed<ModModule>,
        enabled: &BTreeSet<String>,
    ) -> Vec<Diagnostic> {
        let wanted: Vec<usize> = self
            .host
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, plugin)| plugin.rules.iter().any(|rule| enabled.contains(&rule.code)))
            .map(|(position, _)| position)
            .collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let nodes = ast::flatten(source, index, parsed.syntax());
        let model = Model::build(parsed.syntax(), &nodes, source, index);
        let data = Arc::new(FileData { nodes, model });
        let file = wit::File {
            path: display.to_owned(),
            module: module_name(path),
            source: source.to_owned(),
        };
        let mut diagnostics = Vec::new();
        for position in wanted {
            let plugin = &self.host.plugins[position];
            match self.host.check(position, &file, &data) {
                Outcome::Diagnostics(reported) => match accept(plugin, reported, source, index) {
                    Ok(accepted) => diagnostics.extend(accepted),
                    Err(reason) => diagnostics.push(aborted(plugin, &reason)),
                },
                Outcome::Aborted(reason) => diagnostics.push(aborted(plugin, &reason)),
            }
        }
        diagnostics
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
