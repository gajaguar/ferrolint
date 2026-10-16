//! One run: load the configuration, select the files, lint them in
//! parallel and gather the findings into one deterministic order.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use ferrolint_core::{Diagnostic, Finding, LineIndex, Registry, sort_findings};
use rayon::prelude::*;
use ruff_python_ast::PySourceType;
use sha2::{Digest, Sha256};

use crate::checks;
use crate::config::{self, Config};
use crate::discover::{self, Excludes};
use crate::error::{Error, Result};
use crate::selection::Selection;

/// What a run hands to its report.
pub struct Report {
    pub findings: Vec<Finding>,
    pub files: usize,
}

/// What a run was asked to do.
pub struct Options {
    pub paths: Vec<PathBuf>,
    pub config: Option<PathBuf>,
    pub threads: usize,
}

pub fn run(cwd: &Path, options: &Options) -> Result<Report> {
    let config = config::load(cwd, options.config.as_deref())?;
    verify_plugins(&config)?;
    if !config.plugins.is_empty() {
        return Err(Error::plugin("loading WASM plugins is not implemented yet"));
    }
    let registry =
        Registry::new(checks::host_rules()).map_err(|err| Error::config(err.to_string()))?;
    let selection = Selection::new(&registry, &config)?;
    let excludes = Excludes::new(&config)?;
    let files = discover::discover(cwd, &options.paths, &excludes)?;

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.threads)
        .build()
        .map_err(|err| Error::io(format!("cannot start worker threads: {err}")))?;
    let per_file: Vec<Result<Vec<Finding>>> = pool.install(|| {
        files
            .par_iter()
            .map(|path| lint_file(cwd, path, &selection, &registry))
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

/// Check every plugin's file against its pinned SHA-256 before any file
/// is linted.
fn verify_plugins(config: &Config) -> Result<()> {
    for plugin in &config.plugins {
        let actual = sha256_file(&plugin.path).map_err(|err| {
            Error::plugin(format!("cannot read {}: {err}", plugin.path.display()))
        })?;
        if actual != plugin.sha256 {
            return Err(Error::plugin(format!(
                "{}: sha256 mismatch: the configuration pins {}, the file is {actual}",
                plugin.path.display(),
                plugin.sha256
            )));
        }
    }
    Ok(())
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn lint_file(
    cwd: &Path,
    path: &Path,
    selection: &Selection,
    registry: &Registry,
) -> Result<Vec<Finding>> {
    let bytes = fs::read(path)
        .map_err(|err| Error::io(format!("cannot read {}: {err}", path.display())))?;
    let diagnostics = match String::from_utf8(bytes) {
        Ok(source) => check_source(&source),
        Err(_) => vec![checks::not_utf8()],
    };
    let enabled = selection.for_file(path);
    Ok(place(cwd, path, diagnostics, &enabled, registry))
}

fn check_source(source: &str) -> Vec<Diagnostic> {
    let parsed = ruff_python_parser::parse_unchecked_source(source, PySourceType::Python);
    let index = LineIndex::new(source);
    checks::check(source, &parsed, &index)
}

/// Keep the diagnostics whose code reports for this file and attach the
/// path and aliases a report needs.
fn place(
    cwd: &Path,
    path: &Path,
    diagnostics: Vec<Diagnostic>,
    enabled: &BTreeSet<String>,
    registry: &Registry,
) -> Vec<Finding> {
    let display = discover::display_path(cwd, path);
    diagnostics
        .into_iter()
        .filter(|diagnostic| enabled.contains(&diagnostic.code))
        .map(|diagnostic| Finding {
            path: display.clone(),
            aliases: registry
                .get(&diagnostic.code)
                .map(ferrolint_core::RuleInfo::aliases)
                .unwrap_or_default(),
            diagnostic,
        })
        .collect()
}
