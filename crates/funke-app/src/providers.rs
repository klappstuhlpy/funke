//! Built-in providers that live in the app crate because they act on the launcher itself.
//! Feature providers (apps, files, ...) get their own crates under `crates/` from M1 on.

use funke_core::{
    alias_score, glyph_data_url, t, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider,
};

/// Launcher self-control: quit, and later reload/settings.
pub struct ControlProvider;

/// (command, title key, subtitle key, icon glyph — 24×24 SVG paths, see [`glyph_data_url`])
/// The command doubles as the id, so it stays stable when the language does not.
const ENTRIES: &[(&str, &str, &str, &str)] = &[
    (
        "settings",
        "control.settings.title",
        "control.settings.subtitle",
        "<circle cx='12' cy='12' r='3.2'/><path d='M12 2.8v2.4'/><path d='M12 18.8v2.4'/><path d='M2.8 12h2.4'/>\
         <path d='M18.8 12h2.4'/><path d='M5.5 5.5l1.7 1.7'/><path d='M16.8 16.8l1.7 1.7'/>\
         <path d='M18.5 5.5l-1.7 1.7'/><path d='M7.2 16.8l-1.7 1.7'/>",
    ),
    (
        "quit",
        "control.quit.title",
        "control.quit.subtitle",
        "<path d='M15 4.5H8A1.5 1.5 0 0 0 6.5 6v12A1.5 1.5 0 0 0 8 19.5h7'/><path d='M11 12h9.5'/><path d='M17 8.5l3.5 3.5-3.5 3.5'/>",
    ),
];

impl SearchProvider for ControlProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "control",
            // Shares the section label with funke-utils' SystemProvider on purpose.
            name: t("provider.commands"),
            prefix: None,
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let Some(matcher) = FuzzyMatcher::new(&query.text) else {
            return Vec::new();
        };
        ENTRIES
            .iter()
            .filter_map(|(command, title, subtitle, glyph)| {
                // `settings` finds it whatever the UI language is (see `alias_score`).
                alias_score(&matcher, title).map(|score| ResultItem {
                    id: format!("control:{command}"),
                    provider: "control".into(),
                    title: t(title).into(),
                    subtitle: Some(t(subtitle).into()),
                    icon: Some(glyph_data_url(glyph)),
                    score,
                    actions: vec![NamedAction::new(
                        t(title),
                        Action::AppControl {
                            command: (*command).into(),
                        },
                    )],
                })
            })
            .collect()
    }
}
