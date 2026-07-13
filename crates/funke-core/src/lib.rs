//! funke-core — the UI-free heart of the launcher.
//!
//! Everything in this crate must stay free of Tauri, webview, and Win32 imports so it
//! remains unit-testable and reusable. The app crate wires providers into a [`Registry`]
//! and exposes [`Registry::search`] over IPC.

mod frecency;
mod fuzzy;
mod glyph;
pub mod i18n;
mod orchestrator;
mod recents;
mod roots;
mod settings;

pub use frecency::FrecencyStore;
pub use fuzzy::FuzzyMatcher;
pub use glyph::glyph_data_url;
pub use i18n::{alias_score, t, tf, Locale};
pub use orchestrator::DEFAULT_DEADLINE;
pub use recents::RecentsStore;
pub use roots::{denied_dir_name, is_junk_path, resolve_index_roots};
pub use settings::{Quicklink, ScopeHotkey, Settings, Snippet};

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A single keystroke-driven search request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    pub text: String,
    /// The user reached this provider through its keyword (`s report`), rather than the
    /// query being fanned out to everyone.
    ///
    /// It lets a provider be more forthcoming when it was *asked for* than when it merely
    /// overheard the query — snippets search their bodies only when scoped, so a global
    /// search can't surface your address because you typed a street name. Providers that
    /// answer the same either way ignore it.
    #[serde(default)]
    pub scoped: bool,
}

impl Query {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            scoped: false,
        }
    }

    /// The same query, arriving through a provider's keyword.
    pub fn scoped(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            scoped: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// What happens when the user confirms a result. Serialized to the UI and sent back
/// verbatim on Enter, so the frontend never needs to understand action semantics.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Open a file or folder with the shell default handler.
    OpenPath { path: String },
    /// Reveal a file or folder in Explorer (select it in its parent).
    RevealPath { path: String },
    /// Launch an application (path, AUMID, or PATH executable).
    LaunchApp { target: String },
    /// Open a URL in the default browser.
    OpenUrl { url: String },
    /// Run a program with arguments, without a console window (system commands).
    RunCommand { program: String, args: Vec<String> },
    /// Copy text to the clipboard.
    CopyText { text: String },
    /// Type text straight into the window the overlay was summoned from (clipboard
    /// history, snippets) — the same focus seam vault autotype uses.
    PasteText { text: String },
    /// Drop one entry from the clipboard history (ids are per-process, see funke-clipboard).
    ClipboardForget { id: u64 },
    /// Expand a snippet's placeholders and paste it into the window the overlay was
    /// summoned from. Expansion happens at action time, not query time — `{CLIPBOARD}`
    /// and `{DATE}` mean "now", and `{CURSOR}` needs the keystrokes the paste sends.
    SnippetPaste { id: String },
    /// Expand a snippet and put it on the clipboard instead of typing it.
    SnippetCopy { id: String },
    /// Bring an existing top-level window to the foreground (window switcher).
    FocusWindow { hwnd: isize },
    /// Force-terminate a process (the window switcher's destructive action).
    KillProcess { pid: u32 },
    /// Switch the overlay into the masked master-password prompt (vault locked).
    PromptVaultUnlock,
    /// Unlock the vault via a Windows Hello consent prompt (a DPAPI-protected session
    /// key from an earlier master-password unlock is redeemed — see SECURITY.md).
    VaultHelloUnlock,
    /// Copy one field of a vault item. The secret is fetched at action time by id —
    /// it never rides inside a `ResultItem`.
    VaultCopy { id: String, field: String },
    /// Autotype a vault item's credentials into the previously focused window.
    ///
    /// `force` skips the login-form guard (`Settings::vault_autotype_guard`): it is what
    /// the "type anyway" row a blocked autotype offers carries, and the only way a secret
    /// is ever typed into a window that shows no password field. Never set on a row the
    /// providers produce — the user has to ask for it, per attempt.
    VaultAutotype {
        id: String,
        #[serde(default)]
        force: bool,
    },
    /// Open a vault item's website in the default browser and autofill it once the login
    /// form is up — for the credential you have no window open for yet.
    VaultOpenAutotype { id: String },
    /// Hand an action back to the out-of-process plugin that produced the item;
    /// the plugin executes it (the host only routes).
    PluginInvoke {
        plugin: String,
        item: String,
        action_index: usize,
    },
    /// Internal launcher commands (quit, reload, ...).
    AppControl { command: String },
}

/// One user-invocable action on a result, with the label the actions menu shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedAction {
    pub label: String,
    pub action: Action,
    /// Destructive: the UI demands an explicit second Enter before running it.
    #[serde(default)]
    pub confirm: bool,
}

impl NamedAction {
    pub fn new(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            action,
            confirm: false,
        }
    }

    /// An action the UI must confirm before running (shutdown, kill, ...).
    pub fn confirmed(label: impl Into<String>, action: Action) -> Self {
        Self {
            label: label.into(),
            action,
            confirm: true,
        }
    }
}

/// One row in the result list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultItem {
    pub id: String,
    pub provider: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    /// Small square icon as a data URL — `data:image/png;base64,…` from the shell or an
    /// inline SVG glyph ([`glyph_data_url`]); the UI falls back to a monogram when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Higher is better. Providers score their own items; the registry merges globally.
    pub score: i64,
    /// Never empty. `actions[0]` runs on Enter, `actions[1]` (when present) on
    /// Shift+Enter; the actions menu (Tab) lists them all. The UI treats entries as
    /// opaque — it only renders labels and sends the chosen index back.
    pub actions: Vec<NamedAction>,
}

impl ResultItem {
    /// The default (Enter) action, if the provider supplied any.
    pub fn primary_action(&self) -> Option<&Action> {
        self.actions.first().map(|named| &named.action)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderMeta {
    pub id: &'static str,
    /// Display name, used as the section label in the result list. Providers sharing a
    /// name (e.g. launcher control + system commands as "Commands") merge into one section.
    pub name: &'static str,
    /// Keyword that restricts a query to this provider (e.g. `f` for files).
    pub prefix: Option<&'static str>,
    /// Only answer prefix-scoped queries, never global ones (the vault: account names
    /// must not surface while typing an ordinary search).
    pub prefix_only: bool,
}

/// A search source.
///
/// Deliberately synchronous: [`Registry::search_streaming`] supplies the concurrency by
/// calling `query` on a worker thread per provider, so a slow source costs its own rows
/// and never the keystroke. A provider that answers from memory should still do so —
/// arriving inside the deadline is the difference between rows that are simply *there*
/// and rows that appear a moment later.
pub trait SearchProvider: Send + Sync {
    fn metadata(&self) -> ProviderMeta;
    fn query(&self, query: &Query) -> Vec<ResultItem>;
}

/// Owns all enabled providers and merges their results.
///
/// Providers are held behind `Arc` so the orchestrator's worker threads can each hold one
/// while the registry keeps owning the set — a provider abandoned past the deadline stays
/// alive until it finishes, whatever the registry does next.
#[derive(Default)]
pub struct Registry {
    providers: Vec<Arc<dyn SearchProvider>>,
}

impl Registry {
    pub const MAX_RESULTS: usize = 50;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Box<dyn SearchProvider>) {
        self.providers.push(Arc::from(provider));
    }

    /// Drop a provider by id. The counterpart to registering a plugin live: an uninstalled
    /// plugin must stop answering queries immediately, not at the next launch. Returns
    /// whether anything was removed.
    pub fn unregister(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|provider| provider.metadata().id != id);
        self.providers.len() != before
    }

    /// Fan the query out to every provider and merge, best score first.
    ///
    /// A leading provider keyword scopes the query: `f report` searches only the
    /// provider whose [`ProviderMeta::prefix`] is `f`, with the keyword stripped.
    pub fn search(&self, query: &Query) -> Vec<ResultItem> {
        self.search_enabled(query, |_| true)
    }

    /// [`search`](Self::search), restricted to providers the filter accepts — how the
    /// app applies the settings toggles. A keyword for a rejected provider is treated
    /// as ordinary query text.
    ///
    /// The blocking path: every provider is called in turn, so the reply waits for the
    /// slowest of them. The app searches through [`search_streaming`](Self::search_streaming)
    /// instead; this stays as the degenerate case, and as what the tests pin the dispatch
    /// rules against.
    pub fn search_enabled(&self, query: &Query, enabled: impl Fn(&ProviderMeta) -> bool) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }
        let (providers, query) = self.dispatch(query, &enabled);
        Self::rank(providers.iter().flat_map(|p| p.query(&query)).collect())
    }

    /// Who answers this query, and with what text — the keyword-scoping rule, in one place,
    /// so the blocking and streaming paths can never disagree about it.
    ///
    /// The space is what commits to the scope, so it must survive to be found: a keyword
    /// alone (`c`) is still ordinary query text, but `c ` hands the provider an *empty*
    /// query — which is how a browse view (the clipboard's history list) is reached.
    /// Trimming both ends first would eat that trailing space.
    fn dispatch(
        &self,
        query: &Query,
        enabled: &dyn Fn(&ProviderMeta) -> bool,
    ) -> (Vec<Arc<dyn SearchProvider>>, Query) {
        if let Some((keyword, rest)) = query.text.trim_start().split_once(char::is_whitespace) {
            let scoped = self.providers.iter().find(|p| {
                let meta = p.metadata();
                meta.prefix.is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword)) && enabled(&meta)
            });
            if let Some(provider) = scoped {
                return (vec![Arc::clone(provider)], Query::scoped(rest.trim()));
            }
        }
        let providers = self
            .providers
            .iter()
            .filter(|p| {
                let meta = p.metadata();
                !meta.prefix_only && enabled(&meta)
            })
            .map(Arc::clone)
            .collect();
        (providers, query.clone())
    }

    /// Metadata of every registered provider, in registration order (for the settings UI).
    pub fn providers(&self) -> Vec<ProviderMeta> {
        self.providers.iter().map(|p| p.metadata()).collect()
    }

    fn rank(mut items: Vec<ResultItem>) -> Vec<ResultItem> {
        items.sort_by_key(|item| std::cmp::Reverse(item.score));
        items.truncate(Self::MAX_RESULTS);
        items
    }

    /// Display name of a provider id — the UI's section label for its results.
    pub fn provider_name(&self, id: &str) -> Option<&'static str> {
        self.providers
            .iter()
            .map(|p| p.metadata())
            .find(|meta| meta.id == id)
            .map(|meta| meta.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FixedProvider {
        id: &'static str,
        prefix: Option<&'static str>,
        score: i64,
        prefix_only: bool,
    }

    impl SearchProvider for FixedProvider {
        fn metadata(&self) -> ProviderMeta {
            ProviderMeta {
                id: self.id,
                name: self.id,
                prefix: self.prefix,
                prefix_only: self.prefix_only,
            }
        }

        fn query(&self, query: &Query) -> Vec<ResultItem> {
            vec![ResultItem {
                id: format!("{}:1", self.id),
                provider: self.id.to_string(),
                title: query.text.clone(),
                subtitle: Some(if query.scoped { "scoped" } else { "global" }.into()),
                icon: None,
                score: self.score,
                actions: vec![NamedAction::new("Run", Action::AppControl { command: "noop".into() })],
            }]
        }
    }

    #[test]
    fn unregister_drops_a_provider_from_the_fan_out() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "plugin:doomed",
            prefix: None,
            score: 10,
            ..Default::default()
        }));
        assert_eq!(registry.search(&Query::new("hi")).len(), 1);

        assert!(registry.unregister("plugin:doomed"), "the provider was registered");
        assert!(
            registry.search(&Query::new("hi")).is_empty(),
            "an uninstalled plugin stops answering at once, not at the next launch"
        );
        assert!(!registry.unregister("plugin:doomed"), "removing it twice is a no-op");
    }

    #[test]
    fn registry_merges_best_score_first_and_skips_empty_queries() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "low",
            prefix: None,
            score: 10,
            ..Default::default()
        }));
        registry.register(Box::new(FixedProvider {
            id: "high",
            prefix: None,
            score: 90,
            ..Default::default()
        }));

        let results = registry.search(&Query::new("hello"));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].provider, "high");
        assert_eq!(results[1].provider, "low");

        assert!(registry.search(&Query::new("   ")).is_empty());
    }

    #[test]
    fn keyword_prefix_scopes_to_one_provider_and_strips_the_keyword() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "files",
            prefix: Some("f"),
            score: 10,
            ..Default::default()
        }));
        registry.register(Box::new(FixedProvider {
            id: "other",
            prefix: None,
            score: 90,
            ..Default::default()
        }));

        let results = registry.search(&Query::new("f report q3"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "files");
        assert_eq!(results[0].title, "report q3", "keyword must be stripped from the query");

        // A bare keyword with nothing after it stays a normal global query.
        let results = registry.search(&Query::new("f"));
        assert_eq!(results.len(), 2);
    }

    /// The space commits to the scope: `c ` is a provider's browse view (its whole list,
    /// unfiltered), not a global search for "c". Providers that have nothing to browse
    /// see an empty query and return nothing, which is what they did before.
    #[test]
    fn a_keyword_and_a_space_scopes_with_an_empty_query() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "clipboard",
            prefix: Some("c"),
            score: 10,
            prefix_only: true,
        }));
        registry.register(Box::new(FixedProvider {
            id: "other",
            prefix: None,
            score: 90,
            ..Default::default()
        }));

        let results = registry.search(&Query::new("c "));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "clipboard");
        assert_eq!(results[0].title, "", "the provider is asked for everything it has");
    }

    /// A provider must be able to tell "you asked me" from "I overheard the query" —
    /// snippets search their own bodies only in the first case.
    #[test]
    fn providers_learn_whether_they_were_reached_by_keyword() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "snippets",
            prefix: Some("s"),
            score: 10,
            ..Default::default()
        }));

        // FixedProvider reports the flag back through the subtitle.
        let scoped = registry.search(&Query::new("s address"));
        assert_eq!(scoped[0].subtitle.as_deref(), Some("scoped"));

        let global = registry.search(&Query::new("address"));
        assert_eq!(global[0].subtitle.as_deref(), Some("global"));
    }

    #[test]
    fn disabled_providers_are_skipped_even_via_keyword() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "files",
            prefix: Some("f"),
            score: 10,
            ..Default::default()
        }));
        registry.register(Box::new(FixedProvider {
            id: "other",
            prefix: None,
            score: 90,
            ..Default::default()
        }));

        let enabled = |meta: &ProviderMeta| meta.id != "files";
        let results = registry.search_enabled(&Query::new("hello"), enabled);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "other");

        // With files off, `f report` is just text for the remaining providers.
        let results = registry.search_enabled(&Query::new("f report"), enabled);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "other");
        assert_eq!(results[0].title, "f report");
    }

    #[test]
    fn prefix_only_providers_answer_scoped_queries_exclusively() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "vault",
            prefix: Some("v"),
            score: 50,
            prefix_only: true,
        }));
        registry.register(Box::new(FixedProvider {
            id: "other",
            prefix: None,
            score: 10,
            ..Default::default()
        }));

        // Global queries must never reach the prefix-only provider…
        let results = registry.search(&Query::new("github"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "other");

        // …but its own keyword still works.
        let results = registry.search(&Query::new("v github"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider, "vault");
    }

    #[test]
    fn provider_names_resolve_by_id() {
        let mut registry = Registry::new();
        registry.register(Box::new(FixedProvider {
            id: "files",
            prefix: Some("f"),
            score: 10,
            ..Default::default()
        }));
        assert_eq!(registry.provider_name("files"), Some("files"));
        assert_eq!(registry.provider_name("nope"), None);
    }
}
