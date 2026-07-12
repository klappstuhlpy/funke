//! The shared session model, and the index that keeps it fresh.
//!
//! Two coding agents, one provider — the same shape [`funke-files`] uses for its built-in walk
//! and Everything: Claude Code and Codex answer the same question ("resume what I was working
//! on"), so they share the row, the ranking and the actions, and differ only in how their
//! transcripts are read ([`crate::claude`], [`crate::codex`]) and how they are relaunched. A
//! source whose directory does not exist contributes nothing and costs one `stat`, so having
//! only one of the two installed is free.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// How far into a transcript we look for the lines a row is made of. They sit near the top,
/// but Codex writes a long preamble of tool and world-state records before the first typed
/// word — Codex's own session lister scans 200 records for it, so we do too. The byte cap is
/// what keeps a pathological file (a `world_state` or `file-history-snapshot` line can be
/// large) from being read whole.
const MAX_HEAD_BYTES: u64 = 2 * 1024 * 1024;
const MAX_HEAD_LINES: usize = 200;

/// A rescan only stats files (parsing is cached by mtime), but a held-down key should not walk
/// the tree ten times a second either.
const RESCAN_EVERY: Duration = Duration::from_secs(1);

/// Room for a sentence; past that the row would be an unreadable wall anyway.
pub const MAX_TITLE_CHARS: usize = 100;

/// Which agent wrote the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    ClaudeCode,
    Codex,
}

impl Source {
    pub const ALL: [Source; 2] = [Source::ClaudeCode, Source::Codex];

    pub fn label(self) -> &'static str {
        match self {
            Source::ClaudeCode => "Claude Code",
            Source::Codex => "Codex",
        }
    }

    /// The directory its transcripts live under, if the tool has ever run here.
    pub fn root(self) -> Option<PathBuf> {
        let home = PathBuf::from(std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?);
        let root = match self {
            // `projects/<lossily-encoded-cwd>/<uuid>.jsonl`
            Source::ClaudeCode => home.join(".claude").join("projects"),
            // `sessions/<yyyy>/<mm>/<dd>/rollout-<ts>-<uuid>.jsonl`. Rooting here is also what
            // excludes archived sessions — Codex moves those to a sibling directory.
            Source::Codex => home.join(".codex").join("sessions"),
        };
        root.is_dir().then_some(root)
    }

    fn read(self, path: &Path) -> Option<Head> {
        match self {
            Source::ClaudeCode => crate::claude::read(path),
            Source::Codex => crate::codex::read(path),
        }
    }
}

/// One resumable conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    /// What the tool's own resume command takes.
    pub id: String,
    pub source: Source,
    pub cwd: PathBuf,
    pub project: String,
    pub branch: Option<String>,
    /// The conversation's name, or the prompt that opened it. What the row shows.
    pub title: String,
    /// The opening prompt, when the title is the tool's own name for the session rather than
    /// the prompt itself. Searchable, never shown.
    pub prompt: Option<String>,
    pub modified: SystemTime,
}

impl Session {
    /// Every string this session can be found by; best match wins.
    ///
    /// Separate fields rather than one joined haystack, for the reason `funke_core`'s
    /// `alias_score` keeps its two strings apart: scoring the concatenation would let a fuzzy
    /// subsequence scatter *across* the boundaries — half of it in the title, the rest in a
    /// branch name — and score that as if it were a match.
    pub fn fields(&self) -> impl Iterator<Item = &str> {
        [
            Some(self.title.as_str()),
            self.prompt.as_deref(),
            Some(self.project.as_str()),
            self.branch.as_deref(),
        ]
        .into_iter()
        .flatten()
    }
}

/// What a source's reader digs out of the head of a transcript.
#[derive(Debug, PartialEq, Eq)]
pub struct Head {
    pub id: String,
    pub title: String,
    pub prompt: Option<String>,
    pub cwd: PathBuf,
    pub branch: Option<String>,
}

/// The first [`MAX_HEAD_LINES`] lines of a transcript, bounded by [`MAX_HEAD_BYTES`].
///
/// A line cut short by the byte cap simply fails to parse as JSON and is skipped, which is the
/// same thing the readers do with a line whose shape they don't know.
pub fn head_lines(path: &Path) -> Option<impl Iterator<Item = String>> {
    let file = File::open(path).ok()?;
    Some(
        BufReader::new(file.take(MAX_HEAD_BYTES))
            .lines()
            .map_while(Result::ok)
            .take(MAX_HEAD_LINES),
    )
}

/// A prompt is usually several lines; a result row is one.
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max).collect();
    out.push('…');
    out
}

struct Cached {
    modified: SystemTime,
    /// `None` for a transcript we could not read — remembered so a broken file costs one parse,
    /// not one per rescan.
    session: Option<Session>,
}

/// Every source's transcripts, parsed once each and kept until their file changes.
///
/// There is no background thread: a rescan is a directory walk plus one `stat` per file, and
/// only a file whose mtime moved is read again. That is cheap enough to run on the query that
/// needs it, and it means the list is never stale.
#[derive(Default)]
pub struct SessionIndex {
    cached: HashMap<PathBuf, Cached>,
    last_scan: Option<Instant>,
}

impl SessionIndex {
    pub fn refresh(&mut self) {
        if self.last_scan.is_some_and(|at| at.elapsed() < RESCAN_EVERY) {
            return;
        }
        self.last_scan = Some(Instant::now());

        let mut seen = HashSet::new();
        for source in Source::ALL {
            let Some(root) = source.root() else {
                continue; // that tool has never run on this machine
            };
            for transcript in transcripts(&root) {
                let Ok(modified) = transcript.metadata().and_then(|meta| meta.modified()) else {
                    continue;
                };
                if !self.cached.get(&transcript).is_some_and(|c| c.modified == modified) {
                    let session = source
                        .read(&transcript)
                        .map(|head| session_from(head, source, modified));
                    self.cached.insert(transcript.clone(), Cached { modified, session });
                }
                seen.insert(transcript);
            }
        }
        self.cached.retain(|path, _| seen.contains(path));
    }

    /// Every readable session across both sources, newest first.
    pub fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.cached.values().filter_map(|c| c.session.as_ref()).collect();
        sessions.sort_unstable_by_key(|session| std::cmp::Reverse(session.modified));
        sessions
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.cached
            .values()
            .filter_map(|c| c.session.as_ref())
            .find(|session| session.id == id)
    }
}

fn session_from(head: Head, source: Source, modified: SystemTime) -> Session {
    let project = head
        .cwd
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&head.id)
        .to_string();
    Session {
        id: head.id,
        source,
        cwd: head.cwd,
        project,
        branch: head.branch,
        title: head.title,
        prompt: head.prompt,
        modified,
    }
}

/// Every `*.jsonl` under `root`. The two tools nest their transcripts at different depths
/// (Claude Code one folder per project, Codex a folder per calendar day), so rather than teach
/// this two layouts it just walks, shallowly.
fn transcripts(root: &Path) -> Vec<PathBuf> {
    const MAX_DEPTH: usize = 4;
    let mut found = Vec::new();
    let mut frontier = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = frontier.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.flatten().map(|entry| entry.path()) {
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    frontier.push((path, depth + 1));
                }
            } else if path.extension().is_some_and(|ext| ext == "jsonl") {
                found.push(path);
            }
        }
    }
    found
}

/// "4m ago" — the subtitle's recency, so the browse list reads as a timeline.
pub fn ago(modified: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(modified)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3_599 => format!("{}m ago", secs / 60),
        3_600..=86_399 => format!("{}h ago", secs / 3_600),
        86_400..=604_799 => format!("{}d ago", secs / 86_400),
        _ => format!("{}w ago", secs / 604_800),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_prompt_is_cut_to_a_row() {
        let cut = truncate(&"x".repeat(400), MAX_TITLE_CHARS);
        assert_eq!(cut.chars().count(), MAX_TITLE_CHARS + 1); // + the ellipsis
        assert!(cut.ends_with('…'));
        assert_eq!(truncate("short", MAX_TITLE_CHARS), "short");
    }

    #[test]
    fn a_multi_line_prompt_becomes_one_line() {
        assert_eq!(collapse("funke:\n  - one\n\n  - two"), "funke: - one - two");
    }

    #[test]
    fn recency_reads_as_a_timeline() {
        let ago_of = |secs| ago(SystemTime::now() - Duration::from_secs(secs));
        assert_eq!(ago_of(5), "just now");
        assert_eq!(ago_of(300), "5m ago");
        assert_eq!(ago_of(7_200), "2h ago");
        assert_eq!(ago_of(172_800), "2d ago");
        assert_eq!(ago_of(1_209_600), "2w ago");
    }
}
