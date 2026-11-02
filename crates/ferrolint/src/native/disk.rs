//! The disk as the module graph sees it: the few questions it asks about
//! files and directories, so the graph runs on a real disk in a run and on
//! memory in a test.

use std::path::{Path, PathBuf};

/// One entry of a directory.
pub struct Entry {
    pub name: String,
    /// Whether it is a directory (a link is followed).
    pub is_dir: bool,
}

pub trait Disk: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Option<String>;
    fn file_len(&self, path: &Path) -> u64;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> Vec<Entry>;
    fn canonicalize(&self, path: &Path) -> PathBuf;
    /// What a file (its size and time) or a directory (its entries) looks like.
    fn stamp(&self, path: &Path) -> String;
    /// Whether what it reads outlives a `Project`: only a real disk does, so
    /// the thread's caches of listings and parsed modules are for it alone.
    fn shared(&self) -> bool;
}

pub struct RealDisk;

impl Disk for RealDisk {
    fn read_to_string(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn file_len(&self, path: &Path) -> u64 {
        std::fs::metadata(path).map_or(0, |meta| meta.len())
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_dir(&self, path: &Path) -> Vec<Entry> {
        std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| Entry {
                name: entry.file_name().to_string_lossy().into_owned(),
                // `file_type` comes with the entry; only a link needs a `stat`.
                is_dir: entry.file_type().is_ok_and(|kind| {
                    if kind.is_symlink() {
                        entry.path().is_dir()
                    } else {
                        kind.is_dir()
                    }
                }),
            })
            .collect()
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn stamp(&self, path: &Path) -> String {
        crate::project_run::stamp_of(path)
    }

    fn shared(&self) -> bool {
        true
    }
}

/// Files in memory, for tests; a directory is whatever holds a file.
#[cfg(test)]
#[derive(Default)]
pub struct MemDisk {
    files: std::collections::BTreeMap<PathBuf, String>,
}

#[cfg(test)]
impl MemDisk {
    pub fn with(files: &[(&str, &str)]) -> Self {
        Self {
            files: files
                .iter()
                .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
                .collect(),
        }
    }
}

#[cfg(test)]
impl Disk for MemDisk {
    fn read_to_string(&self, path: &Path) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn file_len(&self, path: &Path) -> u64 {
        self.files.get(path).map_or(0, |text| text.len() as u64)
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.files
            .keys()
            .any(|file| file.starts_with(path) && file != path)
    }

    fn exists(&self, path: &Path) -> bool {
        self.is_file(path) || self.is_dir(path)
    }

    fn read_dir(&self, path: &Path) -> Vec<Entry> {
        let mut seen = std::collections::BTreeMap::new();
        for file in self.files.keys().filter(|file| file.starts_with(path)) {
            let mut below = file.strip_prefix(path).unwrap().components();
            if let Some(first) = below.next() {
                let name = first.as_os_str().to_string_lossy().into_owned();
                seen.insert(name, below.next().is_some());
            }
        }
        seen.into_iter()
            .map(|(name, is_dir)| Entry { name, is_dir })
            .collect()
    }

    fn canonicalize(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn stamp(&self, path: &Path) -> String {
        self.files
            .get(path)
            .map_or_else(String::new, |text| text.len().to_string())
    }

    fn shared(&self) -> bool {
        false
    }
}
