//! Plugins installed with pip. A wheel declares one in the
//! `ferrolint.plugins` group of its `entry_points.txt`:
//!
//! ```ini
//! [ferrolint.plugins]
//! pylint_django = ferrolint_django
//! ```
//!
//! The name is the Pylint module a `load-plugins` line names; the value is
//! the Python package whose `plugin.wasm` is the component. Only a plugin
//! that `load-plugins` names is loaded, so installing one changes nothing
//! until the configuration asks for it. The component runs in the same
//! sandbox as a configured one, but no SHA-256 pins it: pip is the trust.

use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::engine::hex;
use crate::host::PluginSource;

/// The entry-point group a wheel declares its plugins in.
const GROUP: &str = "ferrolint.plugins";
/// The component's file name inside the package.
const COMPONENT: &str = "plugin.wasm";

/// The installed plugins that `load-plugins` names, in the order it names
/// them, found in the `site-packages` directories `packages`.
pub fn read(load_plugins: &str, packages: &[PathBuf]) -> Vec<PluginSource> {
    let wanted: Vec<&str> = load_plugins
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(&str, PluginSource)> = Vec::new();
    for dir in packages {
        for (module, package) in entry_points(dir) {
            let Some(&name) = wanted.iter().find(|name| **name == module) else {
                continue;
            };
            let path = dir.join(&package).join(COMPONENT);
            let Ok(bytes) = fs::read(&path) else { continue };
            // The same component under two names (a package that replaces two
            // Pylint modules) loads once: its rules would register twice.
            if found
                .iter()
                .any(|(taken, source)| *taken == name || source.path == path)
            {
                continue;
            }
            let sha256 = hex(&Sha256::digest(&bytes));
            found.push((
                name,
                PluginSource {
                    path,
                    bytes,
                    sha256,
                    options: Vec::new(),
                },
            ));
        }
    }
    wanted
        .iter()
        .filter_map(|name| {
            let at = found.iter().position(|(taken, _)| taken == name)?;
            Some(found.remove(at).1)
        })
        .collect()
}

/// Whether an installed wheel declares a plugin for the Pylint module
/// `module`, so `load-plugins` naming it is not a module that is missing.
pub fn declares(module: &str, packages: &[PathBuf]) -> bool {
    packages
        .iter()
        .any(|dir| entry_points(dir).iter().any(|(name, _)| name == module))
}

/// The `module = package` pairs of every `ferrolint.plugins` section under
/// the `*.dist-info` directories of one `site-packages` directory.
fn entry_points(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut infos: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "dist-info"))
        .collect();
    infos.sort();
    for info in infos {
        if let Ok(text) = fs::read_to_string(info.join("entry_points.txt")) {
            out.extend(parse_entry_points(&text));
        }
    }
    out
}

/// The `module = package` pairs of the `ferrolint.plugins` section of one
/// `entry_points.txt`.
fn parse_entry_points(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            inside = section == GROUP;
        } else if inside
            && !line.starts_with('#')
            && let Some((module, package)) = line.split_once('=')
        {
            // `package:attr` is the standard spelling; only the package counts.
            let package = package.split(':').next().unwrap_or_default().trim();
            out.push((module.trim().to_owned(), package.to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `site-packages` with one wheel of `package` declaring `module`.
    fn site(module: &str, package: &str, wasm: &[u8]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let info = dir.path().join(format!("{package}-1.0.0.dist-info"));
        fs::create_dir_all(&info).unwrap();
        fs::write(
            info.join("entry_points.txt"),
            format!("[console_scripts]\nx = y:z\n\n[{GROUP}]\n{module} = {package}\n"),
        )
        .unwrap();
        fs::create_dir_all(dir.path().join(package)).unwrap();
        fs::write(dir.path().join(package).join(COMPONENT), wasm).unwrap();
        dir
    }

    #[test]
    fn a_plugin_that_load_plugins_names_is_read() {
        let dir = site("pylint_django", "ferrolint_django", b"wasm");
        let found = read(
            "pylint.extensions.mccabe, pylint_django",
            &[dir.path().into()],
        );
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].bytes, b"wasm");
        assert_eq!(found[0].sha256, hex(&Sha256::digest(b"wasm")));
    }

    #[test]
    fn one_component_named_twice_loads_once() {
        let dir = site("pylint_django", "ferrolint_django", b"wasm");
        let info = dir.path().join("ferrolint_django-1.0.0.dist-info");
        fs::write(
            info.join("entry_points.txt"),
            format!("[{GROUP}]\npylint_django = ferrolint_django\npylint_django.checkers.migrations = ferrolint_django\n"),
        )
        .unwrap();
        let found = read(
            "pylint_django, pylint_django.checkers.migrations",
            &[dir.path().into()],
        );
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn an_installed_plugin_nobody_names_stays_unloaded() {
        let dir = site("pylint_django", "ferrolint_django", b"wasm");
        assert!(read("", &[dir.path().into()]).is_empty());
        assert!(read("other_module", &[dir.path().into()]).is_empty());
    }

    #[test]
    fn a_missing_component_or_environment_loads_nothing() {
        let dir = site("pylint_django", "ferrolint_django", b"wasm");
        fs::remove_file(dir.path().join("ferrolint_django").join(COMPONENT)).unwrap();
        assert!(read("pylint_django", &[dir.path().into()]).is_empty());
        assert!(read("pylint_django", &[]).is_empty());
    }

    fn want(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn only_the_plugins_section_is_read() {
        let text =
            "[console_scripts]\nx = y:z\n\n[ferrolint.plugins]\nmod = pkg\n\n[other]\nq = r\n";
        assert_eq!(parse_entry_points(text), want(&[("mod", "pkg")]));
    }

    #[test]
    fn comments_blank_lines_and_attr_are_handled() {
        let text = "[ferrolint.plugins]\n# no = no\n\n  a = pkg_a:attr \nb=pkg_b\nnot a pair\n";
        assert_eq!(
            parse_entry_points(text),
            want(&[("a", "pkg_a"), ("b", "pkg_b")])
        );
    }

    #[test]
    fn a_file_without_the_section_gives_nothing() {
        assert_eq!(parse_entry_points("[console_scripts]\nx = y\n"), want(&[]));
        assert_eq!(parse_entry_points(""), want(&[]));
    }

    #[test]
    fn a_section_reopened_later_adds_its_lines() {
        let text = "[ferrolint.plugins]\na = p\n[x]\nb = q\n[ferrolint.plugins]\nc = r\n";
        assert_eq!(parse_entry_points(text), want(&[("a", "p"), ("c", "r")]));
    }
}
