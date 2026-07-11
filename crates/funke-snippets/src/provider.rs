//! The `SearchProvider` face of snippets.
//!
//! `s ` browses everything you saved, `s sig` searches it. Unlike the vault and the
//! clipboard, snippets **do** appear in a global search — but only by *name* and
//! *abbreviation*, never by content: a snippet is something you deliberately created and
//! named, so finding it by that name without the prefix is the point. Its body, which may
//! be an address or a paragraph of boilerplate, is only matched once you have scoped to
//! `s` and asked.

use std::sync::{Arc, RwLock};

use funke_core::{glyph_data_url, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};
use funke_core::{Settings, Snippet};

pub const PROVIDER_ID: &str = "snippets";

/// A page with a folded corner.
const SNIPPET_GLYPH: &str =
    "<path d='M6 3.5h7l5 5v12a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-16a1 1 0 0 1 1-1z'/><path d='M13 3.5v5h5'/>";

/// The browse view is the order you arranged them in, so scores descend from here.
const BROWSE_TOP_SCORE: i64 = 10_000;
/// A name match is what you meant; a content match is a lucky find. Keep them apart.
const CONTENT_PENALTY: i64 = 40;
const PREVIEW_CHARS: usize = 90;

pub struct SnippetsProvider {
    settings: Arc<RwLock<Settings>>,
}

impl SnippetsProvider {
    pub fn new(settings: Arc<RwLock<Settings>>) -> Self {
        Self { settings }
    }
}

impl SearchProvider for SnippetsProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: PROVIDER_ID,
            name: "Snippets",
            prefix: Some("s"),
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let snippets = self.settings.read().unwrap().snippets.clone();
        if snippets.is_empty() {
            return Vec::new();
        }

        // `s ` — the browse view: everything you saved, in the order you arranged it.
        if query.is_empty() {
            return snippets
                .into_iter()
                .enumerate()
                .map(|(rank, snippet)| row(snippet, BROWSE_TOP_SCORE - rank as i64))
                .collect();
        }

        let Some(matcher) = FuzzyMatcher::new(&query.text) else {
            return Vec::new();
        };
        // The prefix is what unlocks searching the *body*. Without it we are in an
        // ordinary global search, where matching the contents of every snippet would
        // surface your address because you typed a street name.
        let by_content = query.scoped;
        snippets
            .into_iter()
            .filter_map(|snippet| {
                let named = matcher
                    .score(&snippet.name)
                    .into_iter()
                    .chain(matcher.score(&snippet.abbreviation))
                    .max();
                let score = match named {
                    Some(score) => Some(score),
                    None if by_content => matcher.score(&snippet.content).map(|s| s - CONTENT_PENALTY),
                    None => None,
                }?;
                Some(row(snippet, score))
            })
            .collect()
    }
}

fn row(snippet: Snippet, score: i64) -> ResultItem {
    let id = snippet.id.clone();
    let subtitle = match snippet.abbreviation.trim() {
        "" => preview(&snippet.content),
        abbreviation => format!("{abbreviation} · {}", preview(&snippet.content)),
    };
    ResultItem {
        id: format!("snippet:{id}"),
        provider: PROVIDER_ID.into(),
        title: snippet.name,
        subtitle: Some(subtitle),
        icon: Some(glyph_data_url(SNIPPET_GLYPH)),
        score,
        actions: vec![
            NamedAction::new("Paste into last window", Action::SnippetPaste { id: id.clone() }),
            NamedAction::new("Copy to clipboard", Action::SnippetCopy { id }),
        ],
    }
}

/// One line of the body, whitespace collapsed — enough to recognize which snippet this is.
fn preview(content: &str) -> String {
    let mut preview = String::with_capacity(PREVIEW_CHARS);
    let mut spaced = false;
    for c in content.trim().chars() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(id: &str, name: &str, abbreviation: &str, content: &str) -> Snippet {
        Snippet {
            id: id.into(),
            name: name.into(),
            abbreviation: abbreviation.into(),
            content: content.into(),
        }
    }

    fn provider(snippets: Vec<Snippet>) -> SnippetsProvider {
        SnippetsProvider::new(Arc::new(RwLock::new(Settings {
            snippets,
            ..Default::default()
        })))
    }

    fn saved() -> Vec<Snippet> {
        vec![
            snippet("1", "Email signature", "sig", "Viele Grüße\nBenedikt"),
            snippet("2", "Home address", "addr", "Musterstraße 1, 10115 Berlin"),
        ]
    }

    #[test]
    fn the_bare_prefix_browses_everything_in_order() {
        let rows = provider(saved()).query(&Query::new(""));
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["Email signature", "Home address"]);
        assert!(rows[0].score > rows[1].score, "the browse order is the score order");
    }

    #[test]
    fn snippets_are_found_by_name_and_by_abbreviation() {
        let rows = provider(saved()).query(&Query::new("sig"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Email signature");
        assert_eq!(
            rows[0].subtitle.as_deref(),
            Some("sig · Viele Grüße Benedikt"),
            "the row shows the trigger and a one-line preview of the body"
        );
    }

    /// The body is private-ish: an address should not surface because a global search
    /// happened to contain a street name. Only the `s` scope searches inside snippets.
    #[test]
    fn the_body_is_only_searched_behind_the_prefix() {
        let global = Query::new("Musterstraße");
        assert!(provider(saved()).query(&global).is_empty());

        let rows = provider(saved()).query(&Query::scoped("Musterstraße"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Home address");
    }

    #[test]
    fn a_snippet_pastes_by_default_and_can_be_copied() {
        let rows = provider(saved()).query(&Query::new("sig"));
        assert!(matches!(rows[0].primary_action(), Some(Action::SnippetPaste { id }) if id == "1"));
        assert!(matches!(&rows[0].actions[1].action, Action::SnippetCopy { id } if id == "1"));
    }

    #[test]
    fn no_snippets_means_no_rows_at_all() {
        assert!(provider(Vec::new()).query(&Query::new("anything")).is_empty());
    }
}
