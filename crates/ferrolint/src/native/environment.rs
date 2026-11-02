//! The Python environment a run lints against: the standard library, the
//! `site-packages` directories and `PYTHONPATH`. Reading `VIRTUAL_ENV` and its
//! files is here; what the files say is parsed by functions of text.

use std::path::{Path, PathBuf};

/// The standard library and the `site-packages` directories of the
/// environment that `VIRTUAL_ENV` names.
pub fn environment() -> Option<(PathBuf, Vec<PathBuf>)> {
    let env = std::env::var_os("VIRTUAL_ENV").map(PathBuf::from)?;
    let library = stdlib(&env)?;
    Some((library, packages_of(&env)))
}

/// The `site-packages` directories of the environment that `VIRTUAL_ENV`
/// names, which need no standard library behind them.
pub fn site_packages() -> Vec<PathBuf> {
    std::env::var_os("VIRTUAL_ENV")
        .map(|env| packages_of(Path::new(&env)))
        .unwrap_or_default()
}

/// The directories `PYTHONPATH` names, in order.
pub fn pythonpath() -> Vec<PathBuf> {
    std::env::var_os("PYTHONPATH")
        .iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect()
}

fn packages_of(env: &Path) -> Vec<PathBuf> {
    // `lib/pythonX.Y/site-packages` on Unix, `Lib/site-packages` on Windows.
    let unix = std::fs::read_dir(env.join("lib"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path().join("site-packages"));
    unix.chain([env.join("Lib").join("site-packages")])
        .filter(|dir| dir.is_dir())
        .collect()
}

/// The directories the `.pth` files of a `site-packages` directory add to
/// the search path, in file order (`site.addpackage`).
pub fn path_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|e| e == "pth")
                && !path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().starts_with('.'))
        })
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|file| std::fs::read_to_string(file).ok())
        .flat_map(|text| pth_entries(&text, dir))
        .filter(|path| path.is_dir())
        .collect()
}

/// The paths one `.pth` file lists: no blank, comment or `import` line.
fn pth_entries(text: &str, dir: &Path) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("import"))
        .map(|line| dir.join(line))
        .collect()
}

/// The standard library of the interpreter that made the environment
/// `env`, read from its `pyvenv.cfg`.
fn stdlib(env: &Path) -> Option<PathBuf> {
    let config = std::fs::read_to_string(env.join("pyvenv.cfg")).ok()?;
    stdlib_candidates(&config)?
        .into_iter()
        .find(|dir| dir.join("os.py").is_file())
}

/// Where the standard library of the interpreter that `pyvenv.cfg` names may
/// be: `lib/pythonX.Y` (Unix) or `Lib` (Windows) under its prefix.
fn stdlib_candidates(config: &str) -> Option<Vec<PathBuf>> {
    let value_of = |keys: [&str; 2]| {
        config.lines().find_map(|line| {
            let (key, value) = line.split_once('=')?;
            keys.contains(&key.trim()).then(|| value.trim().to_owned())
        })
    };
    let home = PathBuf::from(value_of(["base-prefix", "home"])?);
    // `home` names the `bin` directory; `base-prefix` the one above it.
    let prefix = if home.ends_with("bin") {
        home.parent()?.to_path_buf()
    } else {
        home
    };
    let version = value_of(["version", "version_info"])?;
    let minor: Vec<&str> = version.split('.').take(2).collect();
    Some(vec![
        prefix
            .join("lib")
            .join(format!("python{}", minor.join("."))),
        prefix.join("Lib"),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_of_pyvenv_cfg_gives_the_library_candidates() {
        let config = "home = /opt/py/bin\nversion = 3.14.1\n";
        assert_eq!(
            stdlib_candidates(config).unwrap(),
            [
                PathBuf::from("/opt/py/lib/python3.14"),
                PathBuf::from("/opt/py/Lib")
            ]
        );
    }

    #[test]
    fn the_first_prefix_line_wins_and_version_info_is_read() {
        let config = "base-prefix = /y\nhome = /x/bin\nversion_info = 3.13.0.final.0\n";
        assert_eq!(
            stdlib_candidates(config).unwrap()[0],
            PathBuf::from("/y/lib/python3.13")
        );
    }

    #[test]
    fn a_pyvenv_cfg_without_home_or_version_gives_nothing() {
        assert!(stdlib_candidates("version = 3.14\n").is_none());
        assert!(stdlib_candidates("home = /a/bin\n").is_none());
    }

    #[test]
    fn a_pth_file_lists_its_paths_only() {
        let text = "# c\n\nimport site\n  extra  \nother\n";
        assert_eq!(
            pth_entries(text, Path::new("/sp")),
            [PathBuf::from("/sp/extra"), PathBuf::from("/sp/other")]
        );
    }
}
