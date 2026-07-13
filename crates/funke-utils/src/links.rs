//! Quicklinks: URLs you open often, and the arguments you open them with.
//!
//! A quicklink is a name, an optional abbreviation, and a URL — and if that URL contains
//! `{query}`, everything typed after the abbreviation goes into it. `yt lofi beats` opens a
//! YouTube search for "lofi beats"; `yt` alone opens YouTube. That is the whole feature, and
//! it earns its keep because the alternative is a bookmark you have to find, in a browser you
//! have to focus first.
//!
//! Like snippets, quicklinks are **preferences, not a store**: they live in `Settings` and are
//! edited in Settings → Quicklinks. Unlike snippets they have no prefix — a quicklink is
//! *supposed* to turn up in a global search, because its name is the thing you were going to
//! type anyway.
//!
//! **Placeholders in the URL are the user's, not ours.** An unknown token is left exactly as
//! written, the same rule snippets and vault sequences follow: a quicklink is the user's URL,
//! and a launcher that silently rewrites it is a launcher you cannot predict.

use std::sync::{Arc, RwLock};

use funke_core::{
    Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, Quicklink, ResultItem, SearchProvider, Settings,
};

use crate::browser;

pub const PROVIDER_ID: &str = "links";

/// The placeholder an argument fills. The only one there is — see the module note.
const QUERY_TOKEN: &str = "{query}";

/// Longest argument that goes into a URL. Every browser and server draws this line somewhere,
/// and the argument is whatever happened to be in the input — a pasted wall of text included.
const MAX_QUICKLINK_ARG: usize = 512;

/// An abbreviation with an argument behind it is not a guess: the user named this link and
/// then typed its name. Nothing fuzzy should outrank that.
const EXACT_SCORE: i64 = 5_000;
/// The browse view (`Settings` order), and a name match scored by the fuzzy matcher, both sit
/// below it.
const BROWSE_TOP_SCORE: i64 = 4_000;

pub struct QuicklinksProvider {
    settings: Arc<RwLock<Settings>>,
}

impl QuicklinksProvider {
    pub fn new(settings: Arc<RwLock<Settings>>) -> Self {
        browser::resolve();
        Self { settings }
    }
}

impl SearchProvider for QuicklinksProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: PROVIDER_ID,
            name: funke_core::t("provider.links"),
            prefix: Some("l"),
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let links = self.settings.read().unwrap().quicklinks.clone();
        if links.is_empty() {
            return Vec::new();
        }
        let text = query.text.trim();

        // `l ` — the browse view: every link you saved, in the order you arranged them.
        if text.is_empty() {
            return links
                .iter()
                .enumerate()
                .map(|(rank, link)| row(link, "", BROWSE_TOP_SCORE - rank as i64))
                .collect();
        }

        // An abbreviation followed by a space commits: the rest is the argument, verbatim,
        // spaces and all. Checked before the fuzzy pass so that a link called "Github" can
        // never outrank the link the user explicitly triggered with `gh …`.
        if let Some((link, argument)) = triggered(&links, text) {
            return vec![row(link, argument, EXACT_SCORE)];
        }

        let Some(matcher) = FuzzyMatcher::new(text) else {
            return Vec::new();
        };
        links
            .iter()
            .filter_map(|link| {
                // Name and abbreviation only. The URL is not matching surface: a search for
                // "search" would otherwise hit every link with a `?search_query=` in it.
                let score = matcher
                    .score(&link.name)
                    .into_iter()
                    .chain(matcher.score(&link.abbreviation))
                    .max()?;
                Some(row(link, "", score))
            })
            .collect()
    }
}

/// The `abbr <argument>` form: an exact abbreviation, a space, and the rest.
///
/// The space is what commits — `yt` alone is still an ordinary search (which will match the
/// YouTube link by name anyway, and open it without an argument). Deliberately not fuzzy: a
/// trigger that fires on something *like* what you typed is a trigger you cannot rely on.
fn triggered<'a>(links: &'a [Quicklink], text: &'a str) -> Option<(&'a Quicklink, &'a str)> {
    let (head, rest) = text.split_once(char::is_whitespace)?;
    let link = links
        .iter()
        .find(|link| !link.abbreviation.trim().is_empty() && link.abbreviation.trim().eq_ignore_ascii_case(head))?;
    Some((link, rest.trim()))
}

fn row(link: &Quicklink, argument: &str, score: i64) -> ResultItem {
    let url = resolve(&link.url, argument);
    ResultItem {
        // The id is the link's, not the resolved URL's: frecency should learn that you reach
        // for this quicklink, not that you once searched YouTube for one particular thing.
        id: format!("link:{}", link.id),
        provider: PROVIDER_ID.into(),
        title: link.name.clone(),
        // The URL Enter actually opens, argument and all — so what happens next is visible
        // before it happens.
        subtitle: Some(url.clone()),
        icon: browser::icon(),
        score,
        actions: vec![
            NamedAction::new(funke_core::t("action.open"), Action::OpenUrl { url: url.clone() }),
            NamedAction::new(funke_core::t("action.copy_link"), Action::CopyText { text: url }),
        ],
    }
}

/// Fill every `{query}` with the argument, percent-encoded.
///
/// An empty argument fills them with nothing, which leaves a bare `?search_query=` behind. That
/// is left standing on purpose: it is the user's URL, it works, and quietly deleting a query
/// parameter Funke does not recognize is exactly the kind of helpfulness that makes a template
/// stop doing what its author wrote.
fn resolve(template: &str, argument: &str) -> String {
    if !template.contains(QUERY_TOKEN) {
        return template.to_string();
    }
    let argument: String = argument.chars().take(MAX_QUICKLINK_ARG).collect();
    template.replace(QUERY_TOKEN, &urlencoding::encode(&argument))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(id: &str, name: &str, abbreviation: &str, url: &str) -> Quicklink {
        Quicklink {
            id: id.into(),
            name: name.into(),
            abbreviation: abbreviation.into(),
            url: url.into(),
        }
    }

    fn saved() -> Vec<Quicklink> {
        vec![
            link(
                "1",
                "YouTube search",
                "yt",
                "https://youtube.com/results?search_query={query}",
            ),
            link("2", "GitHub", "gh", "https://github.com/{query}"),
            link("3", "Internal wiki", "", "https://wiki.example.com"),
        ]
    }

    fn provider(quicklinks: Vec<Quicklink>) -> QuicklinksProvider {
        QuicklinksProvider::new(Arc::new(RwLock::new(Settings {
            quicklinks,
            ..Default::default()
        })))
    }

    fn opens(row: &ResultItem) -> &str {
        match row.primary_action() {
            Some(Action::OpenUrl { url }) => url,
            other => panic!("expected OpenUrl, got {other:?}"),
        }
    }

    #[test]
    fn an_abbreviation_and_an_argument_fill_the_template() {
        let rows = provider(saved()).query(&Query::new("yt lofi beats"));
        assert_eq!(rows.len(), 1, "the trigger is exact — nothing else competes with it");
        assert_eq!(rows[0].title, "YouTube search");
        assert_eq!(opens(&rows[0]), "https://youtube.com/results?search_query=lofi%20beats");
        assert_eq!(
            rows[0].subtitle.as_deref(),
            Some("https://youtube.com/results?search_query=lofi%20beats"),
            "the row shows the URL Enter will open, not the template"
        );
    }

    /// The whole rest of the line is the argument — a quicklink argument is a phrase, not a
    /// word, and splitting on the first space only would silently truncate it.
    #[test]
    fn the_argument_is_everything_after_the_abbreviation() {
        let rows = provider(saved()).query(&Query::new("gh rust-lang/rust issues open"));
        assert_eq!(opens(&rows[0]), "https://github.com/rust-lang%2Frust%20issues%20open");
    }

    #[test]
    fn the_trigger_is_case_insensitive_but_never_fuzzy() {
        let rows = provider(saved()).query(&Query::new("YT lofi"));
        assert_eq!(opens(&rows[0]), "https://youtube.com/results?search_query=lofi");

        // `y ` is not `yt`. It falls through to the fuzzy pass, where it is ordinary text —
        // and must not fire the trigger on something merely close to it.
        let rows = provider(saved()).query(&Query::new("y lofi"));
        assert!(rows.iter().all(|row| !opens(row).contains("lofi")));
    }

    /// A bare abbreviation is still just text: it finds the link by name and opens it with an
    /// empty argument. The space is what commits to the trigger.
    #[test]
    fn a_bare_abbreviation_opens_the_link_without_an_argument() {
        let rows = provider(saved()).query(&Query::new("yt"));
        let youtube = rows.iter().find(|row| row.title == "YouTube search").unwrap();
        assert_eq!(opens(youtube), "https://youtube.com/results?search_query=");
    }

    #[test]
    fn links_are_found_by_name_in_a_global_search() {
        let rows = provider(saved()).query(&Query::new("wiki"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Internal wiki");
        assert_eq!(opens(&rows[0]), "https://wiki.example.com");
    }

    /// The URL is not matching surface. Every second link has `search_query` in it.
    #[test]
    fn the_url_itself_is_never_matched() {
        assert!(provider(saved()).query(&Query::new("search_query")).is_empty());
    }

    #[test]
    fn a_template_may_use_the_argument_more_than_once() {
        let links = vec![link("1", "Compare", "cmp", "https://x.example/{query}?also={query}")];
        let rows = provider(links).query(&Query::new("cmp a b"));
        assert_eq!(opens(&rows[0]), "https://x.example/a%20b?also=a%20b");
    }

    /// A URL an argument cannot get into is still a perfectly good quicklink; the argument is
    /// simply dropped rather than appended somewhere it was never invited.
    #[test]
    fn an_argument_for_a_template_without_a_slot_goes_nowhere() {
        let rows = provider(saved()).query(&Query::new("wiki something"));
        assert!(rows.iter().all(|row| opens(row) == "https://wiki.example.com"));
    }

    /// Frecency keys off the id. If it keyed off the resolved URL, every argument would be a
    /// new thing to learn and the link itself would never rise.
    #[test]
    fn the_id_is_the_links_and_does_not_move_with_the_argument() {
        let rows = provider(saved()).query(&Query::new("yt one"));
        assert_eq!(rows[0].id, "link:1");
        let rows = provider(saved()).query(&Query::new("yt two"));
        assert_eq!(rows[0].id, "link:1");
    }

    #[test]
    fn the_bare_prefix_browses_everything_in_order() {
        let rows = provider(saved()).query(&Query::scoped(""));
        let titles: Vec<&str> = rows.iter().map(|row| row.title.as_str()).collect();
        assert_eq!(titles, ["YouTube search", "GitHub", "Internal wiki"]);
        assert!(rows[0].score > rows[1].score, "the browse order is the score order");
    }

    #[test]
    fn no_quicklinks_means_no_rows_at_all() {
        assert!(provider(Vec::new()).query(&Query::new("anything")).is_empty());
    }

    /// A URL has a length limit somewhere in every browser and server, and the argument is
    /// whatever the user happened to have in the input — including a pasted wall of text.
    #[test]
    fn an_absurd_argument_is_cut_rather_than_sent() {
        let long = "x".repeat(MAX_QUICKLINK_ARG * 2);
        let resolved = resolve("https://x.example/?q={query}", &long);
        assert_eq!(resolved.len(), "https://x.example/?q=".len() + MAX_QUICKLINK_ARG);
    }
}
