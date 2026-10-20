//! The result cache in `.ferrolint-cache/`: per-file entries that let an
//! unchanged file skip parsing and plugins, and per-plugin metadata that
//! lets a warm run skip compiling.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ferrolint_core::{Diagnostic, RuleInfo};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::hex;
use crate::suppress::Span;

/// Bumped when an entry's layout or meaning changes.
const LAYOUT: &str = "v2";

/// What a file's entry stores: everything a replay needs without parsing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Entry {
    /// The host rules' diagnostics.
    pub host: Vec<Diagnostic>,
    /// Each suppression comment's codes as spelled, over the lines it covers; resolved
    /// against the registry at replay.
    pub suppressions: Vec<Span>,
    /// Each plugin's diagnostics, by the plugin binary's SHA-256.
    pub plugins: BTreeMap<String, Vec<Diagnostic>>,
    /// The file does not parse, so no plugin runs on it.
    pub unparsable: bool,
}

/// What the project phase stored for a run: its diagnostics by file, and the
/// state of everything it read outside the linted files' own bytes.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Each path (a directory or a file) and what it looked like.
    pub stamps: Vec<(String, String)>,
    pub findings: Vec<(String, Diagnostic)>,
}

/// A plugin's name and rules, by its binary's SHA-256.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PluginMeta {
    pub name: String,
    pub rules: Vec<RuleInfo>,
}

/// A stored value with a checksum of its own body; a value that fails the
/// check reads as absent, so the run recomputes it.
#[derive(Serialize, Deserialize)]
struct Receipt {
    checksum: String,
    body: String,
}

pub struct Cache {
    root: PathBuf,
    /// This build of the linter: its version, plus the size and time of
    /// its executable, so a rebuild at the same version starts afresh.
    build: String,
}

impl Cache {
    pub fn new(cwd: &Path) -> Self {
        let executable = std::env::current_exe()
            .and_then(fs::metadata)
            .map(|meta| {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_nanos());
                format!("{}:{modified}", meta.len())
            })
            .unwrap_or_default();
        Self {
            root: cwd.join(".ferrolint-cache").join(LAYOUT),
            build: format!("{}:{executable}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// The key of a file's entry: every input of its results (its bytes,
    /// path, module name and effective rules), under this build.
    pub fn key(&self, parts: &[&str]) -> String {
        digest(std::iter::once(self.build.as_str()).chain(parts.iter().copied()))
    }

    pub fn entry(&self, key: &str) -> Option<Entry> {
        Self::read(&self.path("files", key))
    }

    pub fn store_entry(&self, key: &str, entry: &Entry) {
        Self::write(&self.path("files", key), entry);
    }

    pub fn project(&self, key: &str) -> Option<ProjectEntry> {
        Self::read(&self.path("project", key))
    }

    pub fn store_project(&self, key: &str, entry: &ProjectEntry) {
        Self::write(&self.path("project", key), entry);
    }

    pub fn plugin(&self, sha256: &str) -> Option<PluginMeta> {
        Self::read(&self.path("plugins", &self.key(&[sha256])))
    }

    pub fn store_plugin(&self, sha256: &str, meta: &PluginMeta) {
        Self::write(&self.path("plugins", &self.key(&[sha256])), meta);
    }

    fn path(&self, kind: &str, key: &str) -> PathBuf {
        let shard = key.get(..2).unwrap_or("00");
        self.root.join(kind).join(shard).join(format!("{key}.json"))
    }

    fn read<T: DeserializeOwned>(path: &Path) -> Option<T> {
        let text = fs::read_to_string(path).ok()?;
        let receipt: Receipt = serde_json::from_str(&text).ok()?;
        if hex(&Sha256::digest(receipt.body.as_bytes())) != receipt.checksum {
            return None;
        }
        serde_json::from_str(&receipt.body).ok()
    }

    /// Write through a temporary file and a rename, so a reader never sees
    /// half an entry. A cache that cannot be written only costs speed.
    fn write<T: Serialize>(path: &Path, value: &T) {
        let Ok(body) = serde_json::to_string(value) else {
            return;
        };
        let receipt = Receipt {
            checksum: hex(&Sha256::digest(body.as_bytes())),
            body,
        };
        let (Some(parent), Ok(text)) = (path.parent(), serde_json::to_string(&receipt)) else {
            return;
        };
        if fs::create_dir_all(parent).is_err() {
            return;
        }
        let temporary = path.with_extension(format!(
            "tmp.{}.{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        if fs::write(&temporary, text).is_ok() && fs::rename(&temporary, path).is_err() {
            let _ = fs::remove_file(&temporary);
        }
    }
}

/// A SHA-256 over length-prefixed parts, so no two lists of parts collide.
fn digest<'a>(parts: impl Iterator<Item = &'a str>) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }
    hex(&hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_and_a_tampered_one_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path());
        let key = cache.key(&["abc", "FL002"]);
        let mut entry = Entry::default();
        entry.suppressions.push(Span {
            first: 3,
            last: 3,
            codes: vec!["W0611".into()],
        });
        cache.store_entry(&key, &entry);
        assert_eq!(cache.entry(&key).unwrap().suppressions[0].codes, ["W0611"]);

        let path = cache.path("files", &key);
        let text = fs::read_to_string(&path).unwrap().replace("W0611", "W0612");
        fs::write(&path, text).unwrap();
        assert!(cache.entry(&key).is_none());
    }

    #[test]
    fn keys_change_with_each_part() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path());
        let base = cache.key(&["abc", "FL002"]);
        assert_ne!(base, cache.key(&["abd", "FL002"]));
        assert_ne!(base, cache.key(&["abc", "FL003"]));
        assert_ne!(cache.key(&["ab", "cFL002"]), base);
    }
}
