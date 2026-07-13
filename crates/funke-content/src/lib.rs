//! Content search — the last of the file-indexing phases in docs/DESIGN.md §4.
//!
//! `f` finds a file by its name. `ff` finds it by what is written *inside* it: the invoice
//! whose number you remember but whose filename you never chose, the note that mentions a
//! person, the contract with the clause in it.
//!
//! Funke does not read those files. Windows already has — the same index behind Explorer's
//! search box and the Start menu has opened every document it was allowed to, run it through
//! the right filter, and remembered the words. Building a second, worse copy of that would
//! mean a background process reading every file the user owns, which is a large thing to ask
//! of a launcher and an even larger one to ask of the person running it. So this provider
//! asks the index a question and ranks the answer. It has no index of its own, no crawler,
//! and nothing on disk.
//!
//! Two consequences fall out of that, and both are the deal rather than defects:
//!
//! - **It finds what Windows indexed, and nothing else.** Folders outside the indexed
//!   locations, file types with no filter installed, and a service the user has turned off
//!   all mean no rows. There is nothing to fall back *to* — an empty answer is the honest
//!   one, and [`winsearch::Unavailable`] says so in the log exactly once.
//! - **It is slow by the standards of a keystroke** — tens to hundreds of milliseconds,
//!   because the answer comes from another process. That is why it may not ride a global
//!   query (`prefix_only`), and why it waited for the search orchestrator: its rows are
//!   allowed to arrive after the ones that came from memory, instead of holding them up.

mod winsearch;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use funke_core::{
    is_junk_path, resolve_index_roots, t, Action, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider,
    Settings,
};

/// Rows shown, and rows asked for.
///
/// More are asked for than shown because the index does not know about the junk denylist:
/// a home directory with a `node_modules` in it will happily spend a whole budget of hits
/// inside one, and the filter below would then leave an empty list where there were real
/// answers just past the cut. The index ranks; we filter and take what is left.
const MAX_RESULTS: usize = 25;
const CANDIDATES: usize = 75;

/// Below this, a content search is a corpus scan with a word attached: "a" matches every
/// document that contains the letter, which is all of them, and the index is made to work for
/// nothing. Two characters is a typo, three is a question.
const MIN_QUERY_CHARS: usize = 3;

/// The top row's score, with each row below it worth one less.
///
/// Content hits are only ever shown behind their keyword, so this never has to compete with
/// another provider — but it is deliberately below what a good filename match scores, because
/// a file *named* what you typed is a better answer than one that merely mentions it, and if
/// these two ever share a list that must stay true.
const TOP_SCORE: i64 = 60;

/// Extensions whose icon belongs to the file rather than to its type.
const PER_FILE_ICON_EXTS: &[&str] = &["exe", "lnk", "ico", "url", "appref-ms"];

pub struct ContentProvider {
    settings: Arc<RwLock<Settings>>,
    search: winsearch::WinSearch,
    /// Icon data URLs by extension, filled lazily at query time — the same trick
    /// `funke-files` uses, for the same reason: only the handful of types that actually turn
    /// up in results pay the shell's price.
    icons: Mutex<HashMap<String, Option<String>>>,
}

impl ContentProvider {
    pub fn spawn(settings: Arc<RwLock<Settings>>) -> Self {
        Self {
            settings,
            search: winsearch::WinSearch::spawn(),
            icons: Mutex::new(HashMap::new()),
        }
    }

    fn item(&self, score: i64, path: &Path) -> ResultItem {
        let path = path.to_string_lossy().into_owned();
        let name = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        ResultItem {
            id: format!("content:{path}"),
            provider: "content".into(),
            title: name,
            subtitle: Some(path.clone()),
            icon: self.icon_for(&path),
            score,
            actions: vec![
                NamedAction::new(t("action.open"), Action::OpenPath { path: path.clone() }),
                NamedAction::new(t("action.reveal"), Action::RevealPath { path: path.clone() }),
                NamedAction::new(t("action.copy_path"), Action::CopyText { text: path }),
            ],
        }
    }

    fn icon_for(&self, path: &str) -> Option<String> {
        let key = Path::new(path)
            .extension()
            .map(|ext| ext.to_string_lossy().to_lowercase())
            .unwrap_or_else(|| "<none>".to_string());
        if PER_FILE_ICON_EXTS.contains(&key.as_str()) {
            return funke_shell::icon_data_url(path);
        }
        self.icons
            .lock()
            .unwrap()
            .entry(key)
            .or_insert_with(|| funke_shell::icon_data_url(path))
            .clone()
    }
}

impl SearchProvider for ContentProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "content",
            name: t("provider.content"),
            prefix: Some("ff"),
            // Never on a global keystroke: this asks another process a question that costs
            // real time, and it must be the user who decided to ask it.
            prefix_only: true,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let text = query.text.trim();
        if text.chars().count() < MIN_QUERY_CHARS {
            return Vec::new();
        }
        let roots = resolve_index_roots(&self.settings.read().unwrap().index_roots);
        let Ok(hits) = self.search.query(text, &roots, CANDIDATES) else {
            return Vec::new();
        };

        // The index ranked them by relevance and it knows things we don't — how often a term
        // occurs, where in the document, how large the document is. Re-scoring the rows
        // against the filename with the fuzzy matcher would be worse than useless: the words
        // the user typed are, by construction, the ones that are *not* in the name.
        hits.iter()
            .filter(|path| !is_junk_path(&path.to_string_lossy()))
            .take(MAX_RESULTS)
            .enumerate()
            .map(|(rank, path)| self.item(TOP_SCORE - rank as i64, path))
            .collect()
    }
}
