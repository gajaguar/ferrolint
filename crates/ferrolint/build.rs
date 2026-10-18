//! Bundle the plugins in the directory `FERROLINT_PLUGINS` names (a release
//! build). With the `bundle` feature and no variable, build them first, so a
//! source install carries them; otherwise the bundle is empty and plugins come
//! from the config.

use std::error::Error;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

mod bundle;

/// Build the workspace plugins as WASM components into `<out>/plugins`.
fn build_plugins(out: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR")?).join("../..");
    for dir in ["plugins", "crates/ferrolint-plugin-sdk", "Cargo.lock"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    let target_dir = out.join("plugins");
    let status = Command::new(env::var("CARGO")?)
        .args([
            "build",
            "--locked",
            "--release",
            "--target",
            "wasm32-wasip2",
        ])
        .args([
            "--workspace",
            "--exclude",
            "ferrolint",
            "--exclude",
            "ferrolint-core",
        ])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(&target_dir)
        .status()?;
    if !status.success() {
        return Err(
            "building the plugins failed; is the wasm32-wasip2 target installed \
                    (rustup target add wasm32-wasip2)?"
                .into(),
        );
    }
    Ok(target_dir.join("wasm32-wasip2/release"))
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=FERROLINT_PLUGINS");
    let out = env::var("OUT_DIR")?;
    let mut entries = String::new();
    let mut dir = env::var_os("FERROLINT_PLUGINS").map(PathBuf::from);
    if dir.is_none() && env::var_os("CARGO_FEATURE_BUNDLE").is_some() {
        dir = Some(build_plugins(Path::new(&out))?);
    }
    if let Some(dir) = dir {
        println!("cargo:rerun-if-changed={}", dir.to_string_lossy());
        let dir = fs::canonicalize(dir)?;
        let names = fs::read_dir(&dir)?
            .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, Box<dyn Error>>>()?;
        let names = bundle::plugin_files(names);
        if names.is_empty() {
            return Err(format!("no ferrolint_plugin_*.wasm in {}", dir.display()).into());
        }
        for name in names {
            let path = dir.join(&name);
            write!(
                entries,
                "({name:?}, include_bytes!({:?})),",
                path.display().to_string()
            )?;
        }
    }
    fs::write(
        format!("{out}/bundled.rs"),
        format!("pub const BUNDLED: &[(&str, &[u8])] = &[{entries}];"),
    )?;
    Ok(())
}
