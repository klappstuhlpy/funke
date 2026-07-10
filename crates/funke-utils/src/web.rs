//! Web search fallback: one low-ranked "search the web" row for any query, or the only
//! row when scoped with the `g` prefix. The engine comes from settings ([`ENGINES`]);
//! the row wears the default browser's icon — that's where Enter lands.

use std::sync::{Arc, OnceLock, RwLock};

use funke_core::{Action, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider, Settings};

/// `(id, display name, query URL with a `{}` placeholder)`. The first entry is the
/// default and the fallback for unknown ids (e.g. from a hand-edited settings file).
pub const ENGINES: &[(&str, &str, &str)] = &[
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
    ("google", "Google", "https://www.google.com/search?q={}"),
    ("bing", "Bing", "https://www.bing.com/search?q={}"),
    ("startpage", "Startpage", "https://www.startpage.com/sp/search?query={}"),
];

pub struct WebSearchProvider {
    settings: Arc<RwLock<Settings>>,
    /// Default-browser icon, resolved once on a background thread (registry + COM stay
    /// off the query path). Rows render icon-less for the instant until it lands.
    browser_icon: Arc<OnceLock<Option<String>>>,
}

const MIN_QUERY_CHARS: usize = 3;
/// Deliberately near the bottom of unscoped result lists — it's a fallback.
const WEB_SCORE: i64 = 3;

fn engine(id: &str) -> (&'static str, &'static str, &'static str) {
    ENGINES
        .iter()
        .copied()
        .find(|(eid, ..)| *eid == id)
        .unwrap_or(ENGINES[0])
}

impl WebSearchProvider {
    pub fn spawn(settings: Arc<RwLock<Settings>>) -> Self {
        let browser_icon = Arc::new(OnceLock::new());
        let slot = Arc::clone(&browser_icon);
        std::thread::spawn(move || {
            let _ = slot.set(funke_shell::default_browser_icon());
        });
        Self { settings, browser_icon }
    }
}

impl SearchProvider for WebSearchProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "web",
            name: "Web",
            prefix: Some("g"),
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let text = query.text.trim();
        if text.chars().count() < MIN_QUERY_CHARS {
            return Vec::new();
        }
        let (_, name, template) = engine(&self.settings.read().unwrap().web_engine);
        vec![ResultItem {
            id: format!("web:{text}"),
            provider: "web".into(),
            title: format!("Search the web for “{text}”"),
            subtitle: Some(name.into()),
            icon: self.browser_icon.get().cloned().flatten(),
            score: WEB_SCORE,
            actions: vec![NamedAction::new(
                "Search",
                Action::OpenUrl {
                    url: url_for(template, text),
                },
            )],
        }]
    }
}

fn url_for(template: &str, text: &str) -> String {
    template.replace("{}", &urlencoding::encode(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider_with(web_engine: &str) -> WebSearchProvider {
        WebSearchProvider::spawn(Arc::new(RwLock::new(Settings {
            web_engine: web_engine.into(),
            ..Default::default()
        })))
    }

    #[test]
    fn queries_are_percent_encoded() {
        assert_eq!(
            url_for("https://duckduckgo.com/?q={}", "rust & tauri"),
            "https://duckduckgo.com/?q=rust%20%26%20tauri"
        );
    }

    #[test]
    fn short_queries_yield_nothing() {
        let provider = provider_with("duckduckgo");
        assert!(provider.query(&Query::new("ab")).is_empty());
        assert_eq!(provider.query(&Query::new("abc")).len(), 1);
    }

    #[test]
    fn engine_comes_from_settings_and_unknown_ids_fall_back() {
        let item = &provider_with("google").query(&Query::new("rust"))[0];
        assert_eq!(item.subtitle.as_deref(), Some("Google"));
        assert!(
            matches!(item.primary_action(), Some(Action::OpenUrl { url }) if url.starts_with("https://www.google.com/"))
        );

        let item = &provider_with("no-such-engine").query(&Query::new("rust"))[0];
        assert_eq!(item.subtitle.as_deref(), Some("DuckDuckGo"));
    }
}
