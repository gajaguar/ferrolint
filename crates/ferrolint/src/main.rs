mod ast;
#[cfg(test)]
#[path = "../bundle.rs"]
mod bundle;
mod c_module_names;
mod cache;
mod checks;
mod class_methods;
mod config;
mod diff;
mod discover;
mod engine;
mod error;
mod host;
mod infer;
mod native;
mod pragma;
mod project_run;
mod pylint;
mod pylint_deprecated;
mod pylint_names;
mod report;
mod selection;
mod semantic;
mod suppress;
mod wit;

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(
    name = "ferrolint",
    version,
    about = "Extensible Python linter in Rust"
)]
struct Args {
    /// Files or directories to lint.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Path to a configuration file (ferrolint.toml or pyproject.toml).
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Worker threads; 0 uses every available core.
    #[arg(long, default_value_t = 0)]
    threads: usize,

    /// The report format.
    #[arg(long, value_enum, default_value_t)]
    format: report::Format,

    /// Lint only the files changed since the merge base of this git ref
    /// and HEAD, plus untracked files.
    #[arg(long, value_name = "REF")]
    diff_base: Option<String>,

    /// Neither read nor write the result cache in .ferrolint-cache/.
    #[arg(long)]
    no_cache: bool,

    /// Print what the run did (files, cache hits, parses, plugin runs and
    /// compiles) to stderr.
    #[arg(long)]
    stats: bool,
}

fn main() -> ExitCode {
    let started = std::time::Instant::now();
    let args = Args::parse();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(err) => {
            return fail(&format!(
                "i/o error: cannot read the working directory: {err}"
            ));
        }
    };
    let options = engine::Options {
        paths: args.paths,
        config: args.config,
        threads: args.threads,
        diff_base: args.diff_base,
        cache: !args.no_cache,
        started,
    };
    match engine::run(&cwd, &options) {
        Ok(report) => {
            let out = report::render(&report, args.format);
            if args.stats {
                let stats = &report.stats;
                let first_parse = stats
                    .first_parse_ms
                    .map_or_else(|| "none".to_owned(), |ms| format!("{ms:.1}"));
                let rss = stats
                    .peak_rss_mb
                    .map_or_else(|| "unknown".to_owned(), |mb| mb.to_string());
                eprintln!(
                    "ferrolint: stats: files={} cached={} parsed={} plugin-runs={} compiles={} \
                     first-parse-ms={first_parse} parallelism={:.2} idle-workers={} peak-rss-mb={rss}",
                    stats.files,
                    stats.cached,
                    stats.parsed,
                    stats.plugin_runs,
                    stats.compiles,
                    stats.parallelism,
                    stats.idle_workers
                );
            }
            if std::io::stdout().lock().write_all(out.as_bytes()).is_err() {
                return ExitCode::from(2);
            }
            if report.aborted() {
                eprintln!(
                    "ferrolint: a plugin aborted on at least one file; see the FL001 findings"
                );
                ExitCode::from(2)
            } else if report.findings.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(err) => fail(&err.to_string()),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("ferrolint: {message}");
    ExitCode::from(2)
}
