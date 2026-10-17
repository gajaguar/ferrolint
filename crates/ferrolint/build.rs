//! Bundle the plugins in the directory `FERROLINT_PLUGINS` names (a release
//! build); without it the bundle is empty and plugins come from the config.

use std::error::Error;
use std::fmt::Write;
use std::{env, fs};

mod bundle;

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=FERROLINT_PLUGINS");
    let mut entries = String::new();
    if let Some(dir) = env::var_os("FERROLINT_PLUGINS") {
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
    let out = env::var("OUT_DIR")?;
    fs::write(
        format!("{out}/bundled.rs"),
        format!("pub const BUNDLED: &[(&str, &[u8])] = &[{entries}];"),
    )?;
    Ok(())
}
