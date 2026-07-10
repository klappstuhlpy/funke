//! System commands: a handful of static power/session actions, fuzzy-matched like any
//! other result. Destructive entries carry `confirm`, so the UI demands a second Enter.

use funke_core::{glyph_data_url, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

pub struct SystemProvider;

struct SystemEntry {
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
        title: "Lock",
        subtitle: "Lock this PC",
        glyph: "<rect x='5' y='10.5' width='14' height='9.5' rx='2'/><path d='M8 10.5V7.5a4 4 0 0 1 8 0v3'/>",
        program: "rundll32",
        args: &["user32.dll,LockWorkStation"],
        confirm: false,
    },
    SystemEntry {
        title: "Sleep",
        subtitle: "Put the PC to sleep",
        glyph: "<path d='M20.2 13.6A8.2 8.2 0 0 1 10.4 3.8a8.2 8.2 0 1 0 9.8 9.8z'/>",
        program: "rundll32",
        args: &["powrprof.dll,SetSuspendState", "0", "1", "0"],
        confirm: false,
    },
    SystemEntry {
        title: "Shut down",
        subtitle: "Shut down this PC",
        glyph: "<path d='M12 3.5v8'/><path d='M7.2 6.4a7.5 7.5 0 1 0 9.6 0'/>",
        program: "shutdown",
        args: &["/s", "/t", "0"],
        confirm: true,
    },
    SystemEntry {
        title: "Restart",
        subtitle: "Restart this PC",
        glyph: "<path d='M20.5 5.5v5h-5'/><path d='M18.9 14.4a7.2 7.2 0 1 1-1.7-7.5l3.3 3.6'/>",
        program: "shutdown",
        args: &["/r", "/t", "0"],
        confirm: true,
    },
    SystemEntry {
        title: "Empty Recycle Bin",
        subtitle: "Delete the recycle bin contents",
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
            name: "Commands",
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
                matcher.score(entry.title).map(|score| ResultItem {
                    id: format!("system:{}", entry.title),
                    provider: "system".into(),
                    title: entry.title.into(),
                    subtitle: Some(entry.subtitle.into()),
                    icon: Some(glyph_data_url(entry.glyph)),
                    score,
                    actions: vec![NamedAction {
                        label: entry.title.into(),
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
