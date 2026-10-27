//! Type inference with ty, built only with the `ty` feature.
//!
//! One salsa database serves the whole run. It also parses: ty infers only
//! over the tree its own database holds, so the host takes that tree instead
//! of parsing a second time.

// The rules that ask it come with the next wave.
#[allow(dead_code)]
pub mod criterion;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use ruff_db::files::system_path_to_file;
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_db::source::source_text;
use ruff_db::system::{OsSystem, SystemPath, SystemPathBuf};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

use crate::error::{Error, Result};

/// The type database of one run.
pub struct Types {
    /// A handle is not `Sync`: each file works on a clone, which shares the caches.
    db: Mutex<ProjectDatabase>,
}

impl Types {
    /// Open the project at `cwd`; `VIRTUAL_ENV` tells ty where `site-packages` is.
    pub fn new(cwd: &Path) -> Result<Self> {
        let fail =
            |err: &dyn std::fmt::Display| Error::io(format!("cannot start the type engine: {err}"));
        // `OsSystem::new` aborts on a relative path.
        let root = std::path::absolute(cwd).map_err(|err| fail(&err))?;
        let root = SystemPathBuf::from_path_buf(root)
            .map_err(|path| fail(&format!("{} is not UTF-8", path.display())))?;
        let system = OsSystem::new(&root);
        let mut metadata = ProjectMetadata::discover(&root, &system).map_err(|err| fail(&err))?;
        metadata
            .apply_configuration_files(&system)
            .map_err(|err| fail(&err))?;
        let db = ProjectDatabase::fallible(metadata, system).map_err(|err| fail(&err))?;
        Ok(Self { db: Mutex::new(db) })
    }

    #[cfg(test)]
    pub fn database(&self) -> ProjectDatabase {
        self.db
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The tree ty parsed for `path`, when it read the same `source` and the
    /// file has no syntax error, nor one only because of the configured Python
    /// version. Otherwise the host parses it itself, which
    /// keeps its `raise;` repair and its E0001.
    pub fn parsed(&self, path: &Path, source: &str) -> Option<ParsedModuleRef> {
        let path = SystemPath::from_std_path(&std::path::absolute(path).ok()?)?.to_path_buf();
        let db = &self
            .db
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let file = system_path_to_file(db, &path).ok()?;
        if source_text(db, file).as_str() != source {
            return None;
        }
        let program_file = db.project().program(db).program_file(db, file);
        let module = parsed_module(db, program_file.python_file(db)).load(db);
        module.has_no_syntax_errors().then_some(module)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_comes_from_ty_and_a_syntax_error_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let good = dir.path().join("good.py");
        let bad = dir.path().join("bad.py");
        std::fs::write(&good, "x = 1\n").unwrap();
        std::fs::write(&bad, "def (:\n").unwrap();
        let types = Types::new(dir.path()).unwrap();
        assert!(types.parsed(&good, "x = 1\n").is_some());
        assert!(types.parsed(&good, "x = 2\n").is_none());
        assert!(types.parsed(&bad, "def (:\n").is_none());
    }

    #[test]
    fn syntax_newer_than_the_configured_python_goes_back_to_the_host() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.ty.environment]\npython-version = \"3.8\"\n",
        )
        .unwrap();
        let file = dir.path().join("generic.py");
        let source = "class A[T]:\n    pass\n";
        std::fs::write(&file, source).unwrap();
        let types = Types::new(dir.path()).unwrap();
        // The host parses with the newest grammar, so it keeps this file.
        assert!(types.parsed(&file, source).is_none());
    }
}
