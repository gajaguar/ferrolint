//! Type inference with ty, built only with the `ty` feature.
//!
//! One salsa database serves the whole run. ty infers over the tree its own
//! database parses; the host cannot share it, because its flat node list
//! rewrites the nodes' indices, which ty's lookups rely on.

mod criterion;
pub mod operations;
pub mod values;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use ruff_db::files::system_path_to_file;
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

    /// Run `body` on the database and the file ty holds for `path`, when it
    /// read the same `source`.
    pub fn with_file<R>(
        &self,
        path: &Path,
        source: &str,
        body: impl FnOnce(&ProjectDatabase, ty_python_core::ProgramFile<'_>) -> R,
    ) -> Option<R> {
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
        Some(body(db, program_file))
    }
}
