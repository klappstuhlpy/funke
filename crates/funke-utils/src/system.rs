//! System commands: a handful of static power/session actions, fuzzy-matched like any
//! other result. Destructive entries carry `confirm`, so the UI demands a second Enter.

use funke_core::{
    alias_score, glyph_data_url, t, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider,
};

pub struct SystemProvider;

struct SystemEntry {
    /// Stable across languages: it is what the item's id — and so the user's frecency
    /// history — is keyed on. The visible strings are looked up from it.
    key: &'static str,
    /// Catalogue keys, resolved per query so a language change needs no restart.
    title: &'static str,
    subtitle: &'static str,
    /// SVG paths for the row icon (24×24 viewbox, see [`glyph_data_url`]).
    glyph: &'static str,
    program: &'static str,
    args: &'static [&'static str],
    /// Destructive commands make the UI ask before running.
    confirm: bool,
}

const ENTRIES: &[SystemEntry] = &[
    SystemEntry {
        key: "lock",
        title: "system.lock.title",
        subtitle: "system.lock.subtitle",
        glyph: "<rect x='5' y='10.5' width='14' height='9.5' rx='2'/><path d='M8 10.5V7.5a4 4 0 0 1 8 0v3'/>",
        program: "rundll32",
        args: &["user32.dll,LockWorkStation"],
        confirm: false,
    },
    SystemEntry {
        key: "sleep",
        title: "system.sleep.title",
        subtitle: "system.sleep.subtitle",
        glyph: "<path d='M20.2 13.6A8.2 8.2 0 0 1 10.4 3.8a8.2 8.2 0 1 0 9.8 9.8z'/>",
        program: "rundll32",
        args: &["powrprof.dll,SetSuspendState", "0", "1", "0"],
        confirm: false,
    },
    SystemEntry {
        key: "shutdown",
        title: "system.shutdown.title",
        subtitle: "system.shutdown.subtitle",
        glyph: "<path d='M12 3.5v8'/><path d='M7.2 6.4a7.5 7.5 0 1 0 9.6 0'/>",
        program: "shutdown",
        args: &["/s", "/t", "0"],
        confirm: true,
    },
    SystemEntry {
        key: "restart",
        title: "system.restart.title",
        subtitle: "system.restart.subtitle",
        glyph: "<path d='M20.5 5.5v5h-5'/><path d='M18.9 14.4a7.2 7.2 0 1 1-1.7-7.5l3.3 3.6'/>",
        program: "shutdown",
        args: &["/r", "/t", "0"],
        confirm: true,
    },
    SystemEntry {
        key: "recycle",
        title: "system.recycle.title",
        subtitle: "system.recycle.subtitle",
        glyph: "<path d='M4.5 6.5h15'/><path d='M8.5 6.5V5A1.5 1.5 0 0 1 10 3.5h4A1.5 1.5 0 0 1 15.5 5v1.5'/>\
                <path d='M6.5 6.5l.9 12.6a1.5 1.5 0 0 0 1.5 1.4h6.2a1.5 1.5 0 0 0 1.5-1.4l.9-12.6'/>",
        program: "powershell",
        args: &[
            "-NoProfile",
            "-Command",
            "Clear-RecycleBin -Force -ErrorAction SilentlyContinue",
        ],
        confirm: true,
    },
];

impl SearchProvider for SystemProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "system",
            // Shares the section label with the launcher's ControlProvider on purpose.
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
            .filter_map(|entry| {
                // Matched against the German *and* the English title: `lock` must keep
                // working for someone whose UI says "Sperren".
                alias_score(&matcher, entry.title).map(|score| ResultItem {
                    // Never the title — a language change would orphan its frecency.
                    id: format!("system:{}", entry.key),
                    provider: "system".into(),
                    title: t(entry.title).into(),
                    subtitle: Some(t(entry.subtitle).into()),
                    icon: Some(glyph_data_url(entry.glyph)),
                    score,
                    actions: vec![NamedAction {
                        label: t(entry.title).into(),
                        action: Action::RunCommand {
                            program: entry.program.into(),
                            args: entry.args.iter().map(|arg| (*arg).to_string()).collect(),
                        },
                        confirm: entry.confirm,
                    }],
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_by_title() {
        let items = SystemProvider.query(&Query::new("lock"));
        assert!(items.iter().any(|item| item.title == "Lock"));

        let items = SystemProvider.query(&Query::new("recycle"));
        assert!(items.iter().any(|item| item.title == "Empty Recycle Bin"));

        assert!(SystemProvider.query(&Query::new("zzz")).is_empty());
    }
}
