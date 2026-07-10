//! Last-opened results, shown on the empty overlay as the "Recent" section. Stored as
//! full [`ResultItem`]s (including their icon data URLs) so the overview renders without
//! re-querying providers.

use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::ResultItem;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RecentsStore {
    items: Vec<ResultItem>,
}

impl RecentsStore {
    pub const CAP: usize = 12;

    /// A missing or corrupt file yields an empty store, same policy as frecency.
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
        fs::write(path, serde_json::to_string(self).expect("recents store serializes"))
    }

    /// Move-or-insert to the front, deduplicated by id, capped at [`Self::CAP`].
    pub fn record(&mut self, item: ResultItem) {
        self.items.retain(|existing| existing.id != item.id);
        self.items.insert(0, item);
        self.items.truncate(Self::CAP);
    }

    pub fn top(&self, n: usize) -> Vec<ResultItem> {
        self.items.iter().take(n).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, NamedAction};

    fn item(id: &str) -> ResultItem {
        ResultItem {
            id: id.to_string(),
            provider: "test".into(),
            title: id.to_string(),
            subtitle: None,
            icon: None,
            score: 0,
            actions: vec![NamedAction::new("Open", Action::OpenPath { path: id.to_string() })],
        }
    }

    #[test]
    fn re_recording_moves_to_front_without_duplicating() {
        let mut store = RecentsStore::default();
        store.record(item("a"));
        store.record(item("b"));
        store.record(item("a"));
        let top = store.top(10);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].id, "a");
        assert_eq!(top[1].id, "b");
    }

    #[test]
    fn store_is_capped() {
        let mut store = RecentsStore::default();
        for i in 0..(RecentsStore::CAP + 5) {
            store.record(item(&format!("id{i}")));
        }
        assert_eq!(store.top(usize::MAX).len(), RecentsStore::CAP);
    }
}
