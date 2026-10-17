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

/// Bumped when an entry's layout or meaning changes.
const LAYOUT: &str = "v1";

/// What a file's entry stores: everything a replay needs without parsing.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Entry {
    /// The host rules' diagnostics.
    pub host: Vec<Diagnostic>,
    /// Each suppression comment's codes as spelled, by line; resolved
    /// against the registry at replay.
    pub suppressions: BTreeMap<u32, Vec<String>>,
    /// Each plugin's diagnostics, by the plugin binary's SHA-256.
    pub plugins: BTreeMap<String, Vec<Diagnostic>>,
    /// The file does not parse, so no plugin runs on it.
    pub unparsable: bool,
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
}

impl Cache {
    pub fn new(cwd: &Path) -> Self {
        Self {
            root: cwd.join(".ferrolint-cache").join(LAYOUT),
        }
    }

    /// The key of a file's entry: its bytes and the configuration that
    /// decides its results, under this build of the linter.
    pub fn key(file_hash: &str, configuration: &str) -> String {
        let mut hasher = Sha256::new();
        for part in [env!("CARGO_PKG_VERSION"), file_hash, configuration] {
            hasher.update(part.len().to_le_bytes());
            hasher.update(part.as_bytes());
        }
        hex(&hasher.finalize())
    }

    pub fn entry(&self, key: &str) -> Option<Entry> {
        Self::read(&self.path("files", key))
    }

    pub fn store_entry(&self, key: &str, entry: &Entry) {
        Self::write(&self.path("files", key), entry);
    }

    pub fn plugin(&self, sha256: &str) -> Option<PluginMeta> {
        Self::read(&self.path("plugins", sha256))
    }

    pub fn store_plugin(&self, sha256: &str, meta: &PluginMeta) {
        Self::write(&self.path("plugins", sha256), meta);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_round_trip_and_a_tampered_one_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = Cache::new(dir.path());
        let key = Cache::key("abc", "FL002");
        let mut entry = Entry::default();
        entry.suppressions.insert(3, vec!["W0611".into()]);
        cache.store_entry(&key, &entry);
        assert_eq!(cache.entry(&key).unwrap().suppressions[&3], ["W0611"]);

        let path = cache.path("files", &key);
        let text = fs::read_to_string(&path).unwrap().replace("W0611", "W0612");
        fs::write(&path, text).unwrap();
        assert!(cache.entry(&key).is_none());
    }

    #[test]
    fn keys_change_with_each_part() {
        let base = Cache::key("abc", "FL002");
        assert_ne!(base, Cache::key("abd", "FL002"));
        assert_ne!(base, Cache::key("abc", "FL003"));
        assert_ne!(Cache::key("ab", "cFL002"), Cache::key("abc", "FL002"));
    }
}
