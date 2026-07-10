//! Frecency: results the user picked often and recently rank higher. This is what makes
//! a launcher feel telepathic. Persisted as a small JSON file; timestamps (unix seconds)
//! are passed in by the caller so the store stays deterministic in tests.

use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FrecencyStore {
    entries: HashMap<String, FrecencyEntry>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct FrecencyEntry {
    count: u32,
    last_used: u64,
}

impl FrecencyStore {
    /// A missing or corrupt file yields an empty store — losing ranking history must
    /// never break the launcher.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string(self).expect("frecency store serializes"))
    }

    pub fn record(&mut self, id: &str, now: u64) {
        let entry = self.entries.entry(id.to_string()).or_insert(FrecencyEntry {
            count: 0,
            last_used: now,
        });
        entry.count = entry.count.saturating_add(1);
        entry.last_used = now;
    }

    /// Additive boost on top of the fuzzy score: repeated picks matter, recent picks
    /// matter more. Capped so frecency refines fuzzy ranking instead of overriding it.
    pub fn boost(&self, id: &str, now: u64) -> i64 {
        let Some(entry) = self.entries.get(id) else { return 0 };
        let count = i64::from(entry.count.min(10)) * 6;
        let recency = match now.saturating_sub(entry.last_used) {
            0..=3_600 => 30,
            3_601..=86_400 => 20,
            86_401..=604_800 => 10,
            604_801..=2_592_000 => 4,
            _ => 0,
        };
        count + recency
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_get_no_boost() {
        let store = FrecencyStore::default();
        assert_eq!(store.boost("apps:foo", 1_000), 0);
    }

    #[test]
    fn repeated_recent_picks_outrank_old_single_picks() {
        let mut store = FrecencyStore::default();
        let day = 86_400;
        store.record("old", 0);
        for t in 0..5 {
            store.record("hot", 29 * day + t);
        }
        let now = 30 * day;
        assert!(store.boost("hot", now) > store.boost("old", now));
    }

    #[test]
    fn boost_is_capped() {
        let mut store = FrecencyStore::default();
        for _ in 0..1_000 {
            store.record("spam", 100);
        }
        assert_eq!(store.boost("spam", 100), 10 * 6 + 30);
    }

    #[test]
    fn corrupt_files_load_as_empty() {
        let dir = std::env::temp_dir().join("funke-frecency-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupt.json");
        fs::write(&path, "not json {").unwrap();
        let store = FrecencyStore::load(&path);
        assert_eq!(store.boost("anything", 0), 0);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join("funke-frecency-test");
        let path = dir.join("roundtrip.json");
        let mut store = FrecencyStore::default();
        store.record("apps:x", 42);
        store.save(&path).unwrap();
        let loaded = FrecencyStore::load(&path);
        assert_eq!(loaded.boost("apps:x", 42), store.boost("apps:x", 42));
        fs::remove_file(&path).ok();
    }
}
