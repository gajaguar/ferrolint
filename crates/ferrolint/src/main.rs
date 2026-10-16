mod ast;
mod checks;
mod config;
mod discover;
mod engine;
mod error;
mod host;
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
}

fn main() -> ExitCode {
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
    };
    match engine::run(&cwd, &options) {
        Ok(report) => {
            let out = report::render(&report, args.format);
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
