//! Filename search provider — indexing Phase A from docs/DESIGN.md §4.
//!
//! A background thread walks the user's home directory (skipping dot-dirs and a junk
//! denylist) into an in-memory filename index, then watches the roots with `notify`:
//! any filesystem event marks the index dirty and it is rebuilt wholesale, at most once
//! per [`REBUILD_MIN_INTERVAL`]. Coarse, but simple and correct — per-event index
//! surgery and USN-journal precision are Phase B (see the plan).
//!
//! Queries prefilter with a cheap byte-subsequence check before nucleo scores the
//! survivors, so a six-figure index stays comfortably inside a keystroke budget.
//!
//! **Unless Everything is running.** voidtools' Everything already keeps a live index of
//! every NTFS volume off the USN journal — exactly the work Phase B has yet to do — so when
//! it is there we ask it instead ([`funke_everything`]) and don't walk at all: no index of
//! our own to build, hold in memory, or rebuild a minute after the disk changes. Its answers
//! are current to the second. The walk resumes by itself if Everything is closed; it is
//! detected, never required, and it is the same provider either way.
//!
//! It changes *how* the index is built, not *what* is searched: the query is scoped to the
//! same `index_roots` the walk would have used (home by default). Searching every volume is
//! one root away — add `C:\` — but it is not the default, and [`everything_search`] says why.
//!
//! The two backends do not answer identically, and the difference is worth knowing:
//! Everything matches **substrings** (spaces AND together), while the built-in index matches
//! fuzzy subsequences — `rprt` finds `report.txt` in one and nothing in the other. Ranking is
//! ours in both cases; only candidate selection differs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use funke_core::{t, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider, Settings};
use notify::{RecursiveMode, Watcher};
use walkdir::WalkDir;

const MAX_ENTRIES: usize = 400_000;
const MAX_RESULTS: usize = 40;
const MIN_QUERY_CHARS: usize = 2;
/// How many candidates to take from Everything before ranking them ourselves. Generous,
/// because Everything truncates by *its* sort order: ask for exactly the 40 we will show and
/// the best fuzzy match may never be among them.
const EVERYTHING_CANDIDATES: u32 = 300;
/// Files compete with apps in unscoped searches; nudge them below equally good app hits.
/// (Scoped `f …` queries only contain files, so relative order is unaffected.)
const GLOBAL_SCORE_PENALTY: i64 = 8;
/// Loop tick: how quickly a settings-side roots change is noticed, and the granularity
/// for the watcher's dirty flag. Two atomic loads per tick — effectively free.
const REBUILD_POLL: Duration = Duration::from_secs(2);
const REBUILD_MIN_INTERVAL: Duration = Duration::from_secs(60);

/// Directory names (lowercase) that are never worth indexing.
const DIR_DENYLIST: &[&str] = &[
    "appdata",
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "$recycle.bin",
];

/// Extensions whose icon is per-file rather than per-type, so the per-extension cache
/// must not be used for them.
const PER_FILE_ICON_EXTS: &[&str] = &["exe", "lnk", "ico", "url", "appref-ms"];

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    /// Pre-lowered for the prefilter, so the hot loop never allocates.
    name_lower: String,
    path: String,
    is_dir: bool,
}

pub struct FilesProvider {
    entries: Arc<RwLock<Vec<FileEntry>>>,
    settings: Arc<RwLock<Settings>>,
    everything: funke_everything::Everything,
    /// Icon data URLs keyed by extension (or `<dir>`/`<none>`), filled lazily at query
    /// time — only the handful of extensions that actually appear in results pay the
    /// shell-extraction cost.
    icon_cache: Mutex<HashMap<String, Option<String>>>,
}

impl FilesProvider {
    /// The index roots come from settings (`index_roots`; empty = home directory) and
    /// are re-read every loop tick, so a change in the settings window takes effect
    /// within seconds — no restart, no explicit nudge channel.
    pub fn spawn(settings: Arc<RwLock<Settings>>) -> Self {
        let entries = Arc::new(RwLock::new(Vec::new()));
        let handle = Arc::clone(&entries);
        let indexed = Arc::clone(&settings);
        thread::spawn(move || index_loop(handle, indexed));
        Self {
            entries,
            settings,
            everything: funke_everything::Everything::spawn(),
            icon_cache: Mutex::new(HashMap::new()),
        }
    }

    /// Everything's answer, ranked by our scorer. Everything decides which files are
    /// candidates; we decide the order they appear in, so an `f` search feels the same
    /// whichever index is behind it.
    fn query_everything(&self, text: &str, matcher: &FuzzyMatcher) -> Vec<ResultItem> {
        // The same roots the walk would have used — Everything changes how the index is
        // built, not which files the user asked to search.
        let roots = resolve_roots(&self.settings.read().unwrap().index_roots);
        let hits = self
            .everything
            .search(&everything_search(text, &roots), EVERYTHING_CANDIDATES);

        let mut scored: Vec<(i64, funke_everything::Hit)> = hits
            .into_iter()
            .filter(|hit| !junk_path(&hit.path))
            .filter_map(|hit| {
                matcher
                    .score(&hit.name)
                    .map(|score| (score - GLOBAL_SCORE_PENALTY, hit))
            })
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.truncate(MAX_RESULTS);

        scored
            .into_iter()
            .map(|(score, hit)| {
                let entry = FileEntry {
                    name_lower: hit.name.to_lowercase(),
                    name: hit.name,
                    path: hit.path,
                    is_dir: hit.is_dir,
                };
                self.item(score, &entry)
            })
            .collect()
    }

    fn item(&self, score: i64, entry: &FileEntry) -> ResultItem {
        ResultItem {
            id: format!("files:{}", entry.path),
            provider: "files".into(),
            title: entry.name.clone(),
            subtitle: Some(entry.path.clone()),
            icon: self.icon_for(entry),
            score,
            actions: vec![
                NamedAction::new(
                    t("action.open"),
                    Action::OpenPath {
                        path: entry.path.clone(),
                    },
                ),
                NamedAction::new(
                    t("action.reveal"),
                    Action::RevealPath {
                        path: entry.path.clone(),
                    },
                ),
                NamedAction::new(
                    t("action.copy_path"),
                    Action::CopyText {
                        text: entry.path.clone(),
                    },
                ),
            ],
        }
    }

    fn icon_for(&self, entry: &FileEntry) -> Option<String> {
        let key = if entry.is_dir {
            "<dir>".to_string()
        } else {
            std::path::Path::new(&entry.name)
                .extension()
                .map(|ext| ext.to_string_lossy().to_lowercase())
                .unwrap_or_else(|| "<none>".to_string())
        };
        if PER_FILE_ICON_EXTS.contains(&key.as_str()) {
            return funke_shell::icon_data_url(&entry.path);
        }
        let mut cache = self.icon_cache.lock().unwrap();
        cache
            .entry(key)
            .or_insert_with(|| funke_shell::icon_data_url(&entry.path))
            .clone()
    }
}

impl SearchProvider for FilesProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "files",
            name: t("provider.files"),
            prefix: Some("f"),
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let text = query.text.trim();
        if text.chars().count() < MIN_QUERY_CHARS {
            return Vec::new();
        }
        let Some(matcher) = FuzzyMatcher::new(text) else {
            return Vec::new();
        };
        // Asked per keystroke on purpose: Everything can be started or quit while Funke
        // stays up, and a stale answer to "is it there?" is the one way this feature could
        // silently return nothing at all.
        if funke_everything::is_running() {
            return self.query_everything(text, &matcher);
        }

        let needle_lower = text.to_lowercase();

        let entries = self.entries.read().unwrap();
        let mut scored: Vec<(i64, &FileEntry)> = entries
            .iter()
            .filter(|entry| is_subsequence(&entry.name_lower, &needle_lower))
            .filter_map(|entry| {
                matcher
                    .score(&entry.name)
                    .map(|score| (score - GLOBAL_SCORE_PENALTY, entry))
            })
            .collect();
        scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
        scored.truncate(MAX_RESULTS);

        scored
            .into_iter()
            .map(|(score, entry)| self.item(score, entry))
            .collect()
    }
}

/// Build Everything's query: its own `path:` filter per root, then the user's text.
///
/// The roots are [`resolve_roots`]' — the same ones the walk indexes, home by default — and
/// **not** the whole disk, however tempting an index of every volume is. Everything caps a
/// reply at [`EVERYTHING_CANDIDATES`] and fills it in its own order, so the whole disk means
/// a common word like "report" (four thousand matches on this machine) spends the entire
/// budget on `C:\Windows\WinSxS` and `C:\ProgramData` before reaching anything of the
/// user's. Whole-disk search is a root away for anyone who wants it: add `C:\`.
fn everything_search(text: &str, roots: &[PathBuf]) -> String {
    let scope = roots
        .iter()
        .map(|root| format!("path:\"{}\"", root.to_string_lossy().trim_end_matches('\\')))
        .collect::<Vec<_>>()
        .join(" | ");
    if scope.is_empty() {
        text.to_string()
    } else {
        // `<…>` groups the alternatives, so the OR binds to the paths and not to the text.
        format!("<{scope}> {text}")
    }
}

/// Is file search currently being served by Everything rather than the built-in walk? For
/// the settings pane, whose account of what gets indexed would otherwise be a fiction.
pub fn everything_is_indexing() -> bool {
    funke_everything::is_running()
}

/// The walk skips junk directories; Everything doesn't know to, so its hits are filtered
/// the same way. Whole-disk search is only a gift if it isn't three screens of
/// `node_modules`.
fn junk_path(path: &str) -> bool {
    path.split('\\').any(|segment| denied_dir_name(&segment.to_lowercase()))
}

fn index_loop(handle: Arc<RwLock<Vec<FileEntry>>>, settings: Arc<RwLock<Settings>>) {
    let fs_dirty = Arc::new(AtomicBool::new(false));
    // Held for its Drop: replacing it un-watches the previous roots.
    let mut _watcher = None;
    let mut watched: Vec<PathBuf> = Vec::new();
    let mut last_build = Instant::now();
    let mut first = true;

    loop {
        // Everything is already indexing every volume off the USN journal; walking the disk
        // to build a second, worse copy of that would be pure waste — of startup, of CPU on
        // every filesystem event, and of the memory the index sits in. Drop what we have and
        // idle. If Everything is closed, `first` sends us straight back to work.
        if funke_everything::is_running() {
            if !first {
                let mut entries = handle.write().unwrap();
                entries.clear();
                entries.shrink_to_fit(); // Give the memory back, not just the slots.
                drop(entries);
                _watcher = None;
                watched.clear();
                first = true;
            }
            thread::sleep(REBUILD_POLL);
            continue;
        }

        let roots = resolve_roots(&settings.read().unwrap().index_roots);
        if first || roots != watched {
            // New roots (or startup): rebuild immediately and move the watcher over.
            *handle.write().unwrap() = build_index(&roots);
            last_build = Instant::now();
            fs_dirty.store(false, Ordering::Relaxed);

            let flag = Arc::clone(&fs_dirty);
            let mut watcher = notify::recommended_watcher(move |result: Result<notify::Event, notify::Error>| {
                if result.is_ok() {
                    flag.store(true, Ordering::Relaxed);
                }
            })
            .ok();
            if let Some(watcher) = watcher.as_mut() {
                for root in &roots {
                    let _ = watcher.watch(root, RecursiveMode::Recursive);
                }
            }
            _watcher = watcher;
            watched = roots;
            first = false;
        } else if fs_dirty.load(Ordering::Relaxed) && last_build.elapsed() >= REBUILD_MIN_INTERVAL {
            fs_dirty.store(false, Ordering::Relaxed);
            *handle.write().unwrap() = build_index(&watched);
            last_build = Instant::now();
        }
        thread::sleep(REBUILD_POLL);
    }
}

/// Settings roots → walkable roots: existing directories only, nested roots pruned
/// (walking a parent already covers its children); an empty result falls back to the
/// user's home directory.
fn resolve_roots(configured: &[String]) -> Vec<PathBuf> {
    let existing: Vec<PathBuf> = configured
        .iter()
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .collect();
    let roots = prune_nested(existing);
    if roots.is_empty() {
        dirs::home_dir().into_iter().collect()
    } else {
        roots
    }
}

/// Drop roots that live inside another root, so no subtree is walked twice.
fn prune_nested(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.sort();
    roots.dedup();
    let mut kept: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !kept.iter().any(|parent| root.starts_with(parent)) {
            kept.push(root);
        }
    }
    kept
}

fn build_index(roots: &[PathBuf]) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for root in roots {
        let walker = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| entry.depth() == 0 || !excluded_dir(entry));
        for entry in walker.flatten() {
            if entry.depth() == 0 {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            out.push(FileEntry {
                name_lower: name.to_lowercase(),
                path: entry.path().to_string_lossy().into_owned(),
                is_dir: entry.file_type().is_dir(),
                name,
            });
            if out.len() >= MAX_ENTRIES {
                return out;
            }
        }
    }
    out
}

fn excluded_dir(entry: &walkdir::DirEntry) -> bool {
    entry.file_type().is_dir() && denied_dir_name(&entry.file_name().to_string_lossy().to_lowercase())
}

/// Expects a lowercase name.
fn denied_dir_name(name: &str) -> bool {
    name.starts_with('.') || DIR_DENYLIST.contains(&name)
}

/// Cheap prefilter before nucleo scoring: every needle byte must appear in the haystack
/// in order. Both sides are pre-lowercased; entries whose match relies on nucleo's
/// unicode normalization may be rejected here — an accepted trade-off for Phase A.
fn is_subsequence(haystack: &str, needle: &str) -> bool {
    let mut bytes = haystack.as_bytes().iter();
    needle.bytes().all(|n| bytes.any(|&b| b == n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_prefilter_accepts_in_order_and_rejects_out_of_order() {
        assert!(is_subsequence("quarterly report q3.xlsx", "rep q3"));
        assert!(is_subsequence("readme.md", "rdme"));
        assert!(!is_subsequence("readme.md", "xyz"));
        assert!(!is_subsequence("abc", "cba"));
        assert!(is_subsequence("anything", ""));
    }

    #[test]
    fn nested_and_duplicate_roots_are_pruned() {
        let pruned = prune_nested(vec![
            PathBuf::from(r"C:\Users\me\Documents"),
            PathBuf::from(r"C:\Users\me"),
            PathBuf::from(r"C:\Users\me"),
            PathBuf::from(r"D:\Media"),
        ]);
        assert_eq!(pruned, vec![PathBuf::from(r"C:\Users\me"), PathBuf::from(r"D:\Media")]);

        // Sibling with a shared name prefix is NOT nested.
        let pruned = prune_nested(vec![PathBuf::from(r"C:\data"), PathBuf::from(r"C:\database")]);
        assert_eq!(pruned.len(), 2);
    }

    #[test]
    fn missing_roots_fall_back_to_home() {
        let roots = resolve_roots(&["Z:\\does\\not\\exist".to_string()]);
        assert_eq!(roots, dirs::home_dir().into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn an_everything_query_is_scoped_to_the_roots_the_walk_would_have_used() {
        assert_eq!(
            everything_search("report", &[PathBuf::from(r"C:\Users\me\Documents")]),
            r#"<path:"C:\Users\me\Documents"> report"#
        );

        // Several roots are alternatives, grouped so the OR binds to the paths — not to
        // the search text, which would make every root optional.
        assert_eq!(
            everything_search("report", &[PathBuf::from(r"C:\work"), PathBuf::from(r"D:\archive\")]),
            r#"<path:"C:\work" | path:"D:\archive"> report"#
        );

        // Never reachable through the provider (`resolve_roots` always yields at least the
        // home directory), and deliberately not special-cased into a whole-disk search: an
        // unscoped query is what an empty root list *means*, not what the user gets.
        assert_eq!(everything_search("report", &[]), "report");
    }

    #[test]
    fn everythings_hits_are_filtered_like_the_walk_is() {
        assert!(junk_path(r"C:\dev\app\node_modules\left-pad\index.js"));
        assert!(junk_path(r"C:\Users\me\AppData\Local\cache.db"));
        assert!(junk_path(r"C:\Users\me\.git\config"));
        assert!(junk_path(r"C:\$Recycle.Bin\S-1-5-21\deleted.docx"));

        assert!(!junk_path(r"C:\Users\me\Documents\report.xlsx"));
        assert!(!junk_path(r"C:\Windows\explorer.exe"));
    }

    #[test]
    fn junk_directories_are_denied() {
        assert!(denied_dir_name("node_modules"));
        assert!(denied_dir_name("appdata"));
        assert!(denied_dir_name(".git"));
        assert!(denied_dir_name("$recycle.bin"));
        assert!(!denied_dir_name("documents"));
        assert!(!denied_dir_name("projects"));
    }
}
