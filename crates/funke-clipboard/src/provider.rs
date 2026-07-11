//! The `SearchProvider` face of the clipboard history.
//!
//! `c ` (the bare prefix and a space) is the browse view: the whole ring, newest first.
//! `c foo` fuzzy-matches the text of the clips. Nothing surfaces in a global search —
//! `prefix_only`, for the same reason as the vault.

use std::sync::Arc;

use funke_core::{glyph_data_url, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

use crate::{Clip, ClipboardHistory};

pub const PROVIDER_ID: &str = "clipboard";

/// A clipboard: two stacked sheets.
const CLIP_GLYPH: &str = "<rect x='8' y='3' width='11' height='14' rx='2'/><path d='M16 7h-11a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2'/>";
/// A broom, for "clear everything".
const CLEAR_GLYPH: &str = "<path d='M4 20l6-6'/><path d='M13 5l6 6-5 5-6-6z'/><path d='M11 7l6 6'/>";

/// The browse view is ordered by recency, so the newest clip must outrank the oldest by
/// more than any fuzzy score can close. Scores descend from here, one per clip.
const BROWSE_TOP_SCORE: i64 = 10_000;
/// Below every clip, so it is the last thing in the list and never the default.
const CLEAR_SCORE: i64 = -1;
/// The preview is one line: enough to recognize a clip, never enough to reflow the row.
const PREVIEW_CHARS: usize = 90;

pub struct ClipboardProvider {
    history: Arc<ClipboardHistory>,
}

impl ClipboardProvider {
    pub fn new(history: Arc<ClipboardHistory>) -> Self {
        Self { history }
    }
}

impl SearchProvider for ClipboardProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: PROVIDER_ID,
            name: "Clipboard",
            prefix: Some("c"),
            // Privacy: what you copied stays out of unscoped results.
            prefix_only: true,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let clips = self.history.clips();
        if clips.is_empty() {
            return vec![empty_row()];
        }

        // `c ` — the browse view: everything, newest first, no filtering.
        if query.is_empty() {
            let mut rows: Vec<ResultItem> = clips
                .into_iter()
                .enumerate()
                .map(|(rank, clip)| clip_row(clip, BROWSE_TOP_SCORE - rank as i64))
                .collect();
            rows.push(clear_row());
            return rows;
        }

        let Some(matcher) = FuzzyMatcher::new(&query.text) else {
            return Vec::new();
        };
        clips
            .into_iter()
            .filter_map(|clip| matcher.score(&clip.text).map(|score| clip_row(clip, score)))
            .collect()
    }
}

fn clip_row(clip: Clip, score: i64) -> ResultItem {
    let id = clip.id;
    ResultItem {
        id: format!("clipboard:{id}"),
        provider: PROVIDER_ID.into(),
        title: preview(&clip.text),
        subtitle: Some(describe(&clip)),
        icon: Some(glyph_data_url(CLIP_GLYPH)),
        score,
        actions: vec![
            NamedAction::new(
                "Paste into last window",
                Action::PasteText {
                    text: clip.text.clone(),
                },
            ),
            NamedAction::new(
                "Copy to clipboard",
                Action::CopyText {
                    text: clip.text.clone(),
                },
            ),
            NamedAction::confirmed("Remove from history", Action::ClipboardForget { id }),
        ],
    }
}

/// The whole ring, gone. Confirmed, because it cannot be undone — the history is memory,
/// so there is nowhere to restore it from.
fn clear_row() -> ResultItem {
    ResultItem {
        id: "clipboard:clear".into(),
        provider: PROVIDER_ID.into(),
        title: "Clear clipboard history".into(),
        subtitle: Some("Forgets every clip — press Enter again to confirm".into()),
        icon: Some(glyph_data_url(CLEAR_GLYPH)),
        score: CLEAR_SCORE,
        actions: vec![NamedAction::confirmed(
            "Clear clipboard history",
            Action::AppControl {
                command: "clipboard-clear".into(),
            },
        )],
    }
}

/// Nothing recorded yet — say why rather than showing an empty list, because "empty" has
/// two very different causes (nothing copied yet vs. Funke was restarted).
fn empty_row() -> ResultItem {
    ResultItem {
        id: "clipboard:empty".into(),
        provider: PROVIDER_ID.into(),
        title: "Clipboard history is empty".into(),
        subtitle: Some("Copy something — history is kept in memory only, so it starts empty each launch".into()),
        icon: Some(glyph_data_url(CLIP_GLYPH)),
        score: 0,
        actions: Vec::new(),
    }
}

/// One line, whitespace collapsed, truncated on a character boundary — a clip may be a
/// paragraph, and the row is one line tall.
fn preview(text: &str) -> String {
    let mut preview = String::with_capacity(PREVIEW_CHARS);
    let mut spaced = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            spaced = true;
            continue;
        }
        if spaced && !preview.is_empty() {
            preview.push(' ');
        }
        spaced = false;
        if preview.chars().count() >= PREVIEW_CHARS {
            preview.push('…');
            break;
        }
        preview.push(c);
    }
    preview
}

/// "3 min ago · 214 characters, 5 lines" — enough to tell two similar clips apart.
fn describe(clip: &Clip) -> String {
    let chars = clip.text.chars().count();
    let lines = clip.text.lines().count();
    let size = match (chars, lines) {
        (1, _) => "1 character".to_string(),
        (chars, lines) if lines > 1 => format!("{chars} characters, {lines} lines"),
        (chars, _) => format!("{chars} characters"),
    };
    format!("{} · {size}", ago(clip.copied_at, crate::unix_now()))
}

/// Relative time, coarse on purpose: nobody needs the second they copied something.
fn ago(then: u64, now: u64) -> String {
    let seconds = now.saturating_sub(then);
    match seconds {
        0..=9 => "just now".into(),
        10..=59 => format!("{seconds} s ago"),
        60..=3599 => format!("{} min ago", seconds / 60),
        3600..=86_399 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use super::*;
    use funke_core::Settings;

    fn history_with(clips: &[&str]) -> Arc<ClipboardHistory> {
        let history = ClipboardHistory::new(Arc::new(RwLock::new(Settings::default())));
        for (i, text) in clips.iter().enumerate() {
            history.record((*text).to_string(), i as u64);
        }
        history
    }

    #[test]
    fn the_bare_prefix_browses_everything_newest_first() {
        let provider = ClipboardProvider::new(history_with(&["oldest", "middle", "newest"]));
        let rows = provider.query(&Query::new(""));

        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["newest", "middle", "oldest", "Clear clipboard history"]);
        // Recency must beat everything, so the browse order is the score order.
        assert!(rows[0].score > rows[1].score && rows[1].score > rows[2].score);
        assert!(rows[3].score < rows[2].score, "clearing is never the default action");
    }

    #[test]
    fn a_query_filters_the_ring() {
        let provider = ClipboardProvider::new(history_with(&["cargo test --workspace", "Guten Morgen"]));
        let rows = provider.query(&Query::new("clippy"));
        assert!(rows.is_empty());

        let rows = provider.query(&Query::new("workspace"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "cargo test --workspace");
    }

    #[test]
    fn a_clip_pastes_by_default_and_can_be_copied_or_forgotten() {
        let provider = ClipboardProvider::new(history_with(&["hello"]));
        let row = provider.query(&Query::new("hello")).remove(0);

        assert!(matches!(row.primary_action(), Some(Action::PasteText { text }) if text == "hello"));
        assert_eq!(row.actions[1].label, "Copy to clipboard");
        assert!(row.actions[2].confirm, "removing a clip asks first");
    }

    #[test]
    fn an_empty_history_explains_itself_instead_of_showing_nothing() {
        let provider = ClipboardProvider::new(history_with(&[]));
        let rows = provider.query(&Query::new(""));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Clipboard history is empty");
        assert!(rows[0].actions.is_empty(), "there is nothing to do with it");
    }

    #[test]
    fn previews_are_one_line_and_bounded() {
        assert_eq!(preview("  hello\n\n   world  "), "hello world");
        let long = preview(&"x".repeat(500));
        assert_eq!(long.chars().count(), PREVIEW_CHARS + 1, "truncated, plus the ellipsis");
        assert!(long.ends_with('…'));
    }

    #[test]
    fn clips_describe_their_age_and_size() {
        assert_eq!(ago(100, 105), "just now");
        assert_eq!(ago(100, 400), "5 min ago");
        assert_eq!(ago(0, 7200), "2 h ago");
        assert_eq!(ago(0, 172_800), "2 d ago");
    }
}
