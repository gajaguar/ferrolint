use clap::Parser;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "ferrolint",
    version,
    about = "Extensible Python linter in Rust"
)]
struct Args {
    /// Path to a file or directory to lint.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Path to a configuration file.
    #[arg(short, long)]
    config: Option<PathBuf>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    if let Some(config_path) = args.config {
        let _config = match load_config(&config_path) {
            Ok(config) => config,
            Err(err) => {
                eprintln!("ferrolint: config error: {err}");
                return ExitCode::from(2);
            }
        };
    }

    eprintln!("ferrolint: not implemented: {}", args.path.display());
    ExitCode::from(2)
}

#[derive(Debug, Default)]
struct Config {}

#[allow(clippy::unnecessary_wraps)]
fn load_config(_path: &Path) -> Result<Config, String> {
    Ok(Config::default())
}
