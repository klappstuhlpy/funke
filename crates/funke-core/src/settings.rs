//! App-wide user settings, persisted as one small JSON file (same contract as
//! [`FrecencyStore`](crate::FrecencyStore): a missing or corrupt file loads as the
//! defaults — losing preferences must never break the launcher). The struct is plain
//! data; applying a change (re-registering the hotkey, re-theming the overlay) is the
//! app crate's job.

use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

use crate::Action;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// UI language: `auto` (follow Windows), `en`, or `de`. Resolved to a
    /// [`crate::Locale`] by the app at startup and whenever this changes.
    pub language: String,
    /// Global summon hotkey in `tauri-plugin-global-shortcut` syntax, e.g. `Ctrl+Space`.
    pub hotkey: String,
    /// Extra hotkeys that summon the overlay already scoped to one source.
    pub scope_hotkeys: Vec<ScopeHotkey>,
    /// Accent color as `#rrggbb`; drives the UI's `--accent` token family.
    pub accent: String,
    /// Overlay width in logical pixels.
    pub overlay_width: f64,
    /// Web search engine id (the app maps ids to names and URL templates).
    pub web_engine: String,
    /// Provider ids the user switched off in settings.
    pub disabled_providers: Vec<String>,
    /// Folders the file search indexes; empty means the user's home directory.
    pub index_roots: Vec<String>,
    /// Index hidden directories (dot-dirs and AppData). Off by default: enabling
    /// exposes caches, config files, and browser profiles in search results.
    pub index_hidden: bool,
    /// Start Funke with Windows.
    pub autostart: bool,
    /// Look for a new release in the background at startup and raise a Windows notification
    /// the first time a given version is seen. Never installs anything — that stays a
    /// button the user presses. This is the only network request Funke makes without being
    /// asked, which is why it is a setting at all (see SECURITY.md).
    pub update_check: bool,
    /// Unlock the vault with Windows Hello instead of the master password on repeat
    /// unlocks (persists a DPAPI-protected `bw` session key — see SECURITY.md).
    pub vault_hello: bool,
    /// Show website favicons on vault entries (fetched from the Bitwarden/Vaultwarden
    /// icon service, which learns the entry's domain — see SECURITY.md).
    pub vault_icons: bool,
    /// Minutes of vault inactivity before it auto-locks; `0` disables idle auto-lock.
    pub vault_idle_lock_minutes: u64,
    /// Whether autotype presses Enter after the password to submit the form. Applies to
    /// the built-in sequence only — an explicit template (here or on the entry) is typed
    /// exactly as written.
    pub vault_autotype_enter: bool,
    /// Default autotype template, e.g. `{USERNAME}{TAB}{PASSWORD}{ENTER}`. Empty = the
    /// built-in sequence. An entry's own `autotype` custom field overrides this.
    pub vault_autotype_sequence: String,
    /// Only autotype into a window that shows a login form (a password field UI
    /// Automation can see). What stops a password — and the sequence's Enter — from being
    /// typed into a chat box, a search bar, or the desktop. A blocked attempt is offered
    /// back to the user as "type anyway", so this costs a confirmation, never the action.
    pub vault_autotype_guard: bool,
    /// Lock the vault automatically when the user walks away: session lock (Win+L),
    /// sleep/hibernate, or an RDP disconnect. (The field name predates the wider
    /// trigger list — it is persisted API, so it keeps its name.)
    pub vault_lock_on_screen_lock: bool,
    /// Hide the overlay from screen capture (screenshots, recordings, shares) while it
    /// shows vault content — the masked master-password prompt, vault rows, context
    /// suggestions. Plain results stay capturable, so demos and screenshots still work.
    pub vault_capture_shield: bool,
    /// Refuse to run a `bw` CLI that isn't signature-verified as Bitwarden's, instead of
    /// warning and using it anyway. Off by default: an npm-installed CLI is an unsigned
    /// `.cmd` wrapper around a Node script, which is a perfectly legitimate install, and a
    /// launcher that bricks it would only teach people to distrust the check.
    pub vault_require_signed_cli: bool,
    /// Offer the credential for the app that was focused when the overlay was summoned
    /// (empty query), matched by window title, process, and — in browsers — the address
    /// bar's URL. Off means vault entries only ever appear behind the `v` keyword.
    pub vault_context_suggest: bool,
    /// Saved snippets (`s` prefix), in the order the settings pane shows them.
    pub snippets: Vec<Snippet>,
    /// Saved quicklinks, in the order the settings pane shows them.
    pub quicklinks: Vec<Quicklink>,
    /// Pinned favourites shown as icon tiles on the empty-input overview.
    pub pinned: Vec<PinnedItem>,
    /// Whether the favourites grid is collapsed (chevron toggle). Persists across restarts.
    pub pins_collapsed: bool,
}

/// A second hotkey that opens the overlay already scoped to one source.
///
/// `Ctrl+Shift+V` → the clipboard's browse view, without passing through a global search first.
/// It is the same thing typing `c ` does — the keyword and its committing space are simply
/// filled in for you — so it needs no new search path, only a new way in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScopeHotkey {
    /// `tauri-plugin-global-shortcut` syntax, like [`Settings::hotkey`]. **Empty means unbound**:
    /// a row in the settings list that has been added but not yet given a chord. It registers
    /// nothing and conflicts with nothing.
    pub hotkey: String,
    /// The provider keyword it opens on — `c`, `v`, `ff`, …
    pub prefix: String,
}

/// One saved quicklink: a URL you open often, optionally with a slot for an argument.
///
/// `https://youtube.com/results?search_query={query}` with the abbreviation `yt` turns
/// `yt lofi beats` into a search, and `youtube` alone into the bare URL. Everything after the
/// abbreviation is the argument, percent-encoded into every `{query}` in the template.
///
/// Like [`Snippet`], it lives here only because [`Settings`] is where it is persisted — a
/// quicklink is a preference, not a store of its own. Core does not interpret it; `funke-utils`
/// does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quicklink {
    /// Stable across edits and restarts, so frecency can learn the link you reach for.
    pub id: String,
    pub name: String,
    /// A short trigger ("yt", "gh"). Optional; may be empty, in which case the name is the
    /// only way to find it.
    #[serde(default)]
    pub abbreviation: String,
    /// The URL. `{query}` — if present — is where the argument goes.
    pub url: String,
}

/// One saved snippet: text you paste often, found by name or abbreviation.
///
/// The content may carry placeholders (`{DATE}`, `{CLIPBOARD}`, `{CURSOR}`, …) that are
/// resolved when it is pasted — see the `funke-snippets` crate. This lives in core only
/// because [`Settings`] is where it is persisted; core does not interpret it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snippet {
    /// Stable across edits and restarts, so frecency can learn a snippet you reach for.
    pub id: String,
    pub name: String,
    /// A short trigger to find it by ("sig", "addr"). Optional; may be empty.
    #[serde(default)]
    pub abbreviation: String,
    pub content: String,
}

/// One pinned favourite: an icon tile the overlay shows below results. Stores the primary
/// action captured at pin time, so clicking the tile launches without re-querying. Lives in
/// [`Settings`] because that is where it is persisted; core does not interpret the action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinnedItem {
    /// Matches the source `ResultItem::id` — keyed so a re-pin toggles rather than duplicates.
    pub id: String,
    pub title: String,
    /// `data:image/png;base64,…` or an inline SVG data URL, captured at pin time.
    pub icon: Option<String>,
    /// Matches `ResultItem::provider` (the display provider field, not any `provider_id`).
    pub provider: String,
    /// The row's primary action, stored at pin time and replayed when the tile is clicked.
    pub action: Action,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            hotkey: "Ctrl+Space".into(),
            scope_hotkeys: Vec::new(),
            accent: "#d97757".into(),
            overlay_width: 680.0,
            web_engine: "duckduckgo".into(),
            disabled_providers: Vec::new(),
            index_roots: Vec::new(),
            index_hidden: false,
            autostart: false,
            update_check: true,
            vault_hello: false,
            vault_icons: true,
            vault_idle_lock_minutes: 10,
            vault_autotype_enter: true,
            vault_autotype_sequence: String::new(),
            vault_autotype_guard: true,
            vault_lock_on_screen_lock: true,
            vault_capture_shield: true,
            vault_require_signed_cli: false,
            vault_context_suggest: true,
            snippets: Vec::new(),
            quicklinks: Vec::new(),
            pinned: Vec::new(),
            pins_collapsed: false,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self).expect("settings serialize"))
    }

    pub fn provider_enabled(&self, id: &str) -> bool {
        !self.disabled_providers.iter().any(|disabled| disabled == id)
    }

    /// The first scope hotkey bound to a chord something else in this file already owns — the
    /// summon hotkey, or an earlier scope hotkey.
    ///
    /// Windows gives a chord to one registrant. The second one to ask does not fail loudly; it
    /// simply never fires, and a setting that quietly does nothing is worse than one that
    /// refuses. So the collision is caught here, while the user is still looking at it.
    pub fn conflicting_scope_hotkey(&self) -> Option<&ScopeHotkey> {
        let mut taken = vec![chord(&self.hotkey)];
        self.scope_hotkeys.iter().find(|scope| {
            if scope.hotkey.trim().is_empty() {
                return false; // Unbound: still being filled in, owns nothing.
            }
            let bound = chord(&scope.hotkey);
            if taken.contains(&bound) {
                return true;
            }
            taken.push(bound);
            false
        })
    }
}

/// A chord, in the one spelling that compares. `Ctrl+Shift+V`, `shift+ctrl+v` and
/// `Ctrl + Shift + V` are the same three keys to Windows, so they must be the same string here
/// — a settings file people can hand-edit will eventually contain all three.
fn chord(hotkey: &str) -> String {
    let mut parts: Vec<String> = hotkey
        .split('+')
        .map(|part| part.trim().to_ascii_lowercase())
        .filter(|part| !part.is_empty())
        .collect();
    parts.sort();
    parts.join("+")
}

impl Quicklink {
    /// Is this a URL Funke is willing to open?
    ///
    /// Only `http:` and `https:`. A quicklink ends in `Action::OpenUrl`, which hands the string
    /// to the shell — and the shell will happily honour `file:`, a registered protocol handler,
    /// or anything else with a colon in it. That would quietly turn a text field in the
    /// settings window into a way to launch programs, which is not what "quicklink" means to
    /// the person filling it in. The check is on save, so a bad URL is refused while it is
    /// still being typed rather than at the moment someone presses Enter on it.
    pub fn has_web_scheme(&self) -> bool {
        let url = self.url.trim_start().to_ascii_lowercase();
        url.starts_with("http://") || url.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corrupt_or_missing_files_load_as_defaults() {
        let dir = std::env::temp_dir().join("funke-settings-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("corrupt.json");
        fs::write(&path, "not json {").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert_eq!(Settings::load(&dir.join("missing.json")), Settings::default());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join("funke-settings-test");
        let path = dir.join("roundtrip.json");
        let settings = Settings {
            hotkey: "Alt+Space".into(),
            disabled_providers: vec!["calc".into()],
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn unknown_fields_and_omissions_fall_back_per_field() {
        // Forward compat: a file from a newer/older build must still load.
        let loaded: Settings = serde_json::from_str(r#"{ "hotkey": "Win+K", "brand_new_field": 1 }"#).unwrap();
        assert_eq!(loaded.hotkey, "Win+K");
        assert_eq!(loaded.accent, Settings::default().accent);
    }

    #[test]
    fn quicklinks_round_trip_and_old_files_load_without_them() {
        let dir = std::env::temp_dir().join("funke-settings-test");
        let path = dir.join("quicklinks.json");
        let settings = Settings {
            quicklinks: vec![Quicklink {
                id: "1".into(),
                name: "YouTube search".into(),
                abbreviation: "yt".into(),
                url: "https://youtube.com/results?search_query={query}".into(),
            }],
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        fs::remove_file(&path).ok();

        // A settings file written before quicklinks existed still loads.
        let loaded: Settings = serde_json::from_str(r#"{ "hotkey": "Win+K" }"#).unwrap();
        assert!(loaded.quicklinks.is_empty());
    }

    /// A quicklink is a browser destination. `Action::OpenUrl` hands its string to the shell,
    /// so anything else with a colon in it is a way to run programs from a settings field.
    #[test]
    fn only_http_urls_are_accepted() {
        let link = |url: &str| Quicklink {
            id: "1".into(),
            name: "x".into(),
            abbreviation: String::new(),
            url: url.into(),
        };
        assert!(link("https://example.com").has_web_scheme());
        assert!(link("http://example.com").has_web_scheme());
        assert!(
            link("HTTPS://EXAMPLE.COM").has_web_scheme(),
            "the scheme is case-insensitive"
        );
        assert!(
            link("  https://example.com").has_web_scheme(),
            "leading space is a typo, not a scheme"
        );

        assert!(!link("file:///C:/Windows/System32/cmd.exe").has_web_scheme());
        assert!(!link("javascript:alert(1)").has_web_scheme());
        assert!(!link("steam://run/570").has_web_scheme());
        assert!(!link("example.com").has_web_scheme(), "no scheme is not a scheme");
        assert!(!link("").has_web_scheme());
    }

    #[test]
    fn a_chord_bound_twice_is_caught_however_it_is_spelled() {
        let scope = |hotkey: &str, prefix: &str| ScopeHotkey {
            hotkey: hotkey.into(),
            prefix: prefix.into(),
        };
        let with = |scopes: Vec<ScopeHotkey>| Settings {
            hotkey: "Ctrl+Space".into(),
            scope_hotkeys: scopes,
            ..Default::default()
        };

        assert!(with(vec![scope("Ctrl+Shift+V", "c"), scope("Ctrl+Shift+F", "f")])
            .conflicting_scope_hotkey()
            .is_none());

        // Against the summon hotkey.
        let settings = with(vec![scope("Ctrl+Space", "c")]);
        assert_eq!(settings.conflicting_scope_hotkey().unwrap().prefix, "c");

        // Against an earlier scope hotkey — and the same chord in a different spelling is the
        // same chord, because Windows reads keys, not strings.
        let settings = with(vec![scope("Ctrl+Shift+V", "c"), scope("shift + ctrl + v", "s")]);
        assert_eq!(
            settings.conflicting_scope_hotkey().unwrap().prefix,
            "s",
            "the later one is the one that would silently never fire"
        );

        // An unbound row owns nothing, however many of them there are.
        assert!(with(vec![scope("", "c"), scope("", "v")])
            .conflicting_scope_hotkey()
            .is_none());
    }

    #[test]
    fn scope_hotkeys_round_trip_and_old_files_load_without_them() {
        let dir = std::env::temp_dir().join("funke-settings-test");
        let path = dir.join("scopes.json");
        let settings = Settings {
            scope_hotkeys: vec![ScopeHotkey {
                hotkey: "Ctrl+Shift+V".into(),
                prefix: "c".into(),
            }],
            ..Default::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        fs::remove_file(&path).ok();

        let loaded: Settings = serde_json::from_str(r#"{ "hotkey": "Win+K" }"#).unwrap();
        assert!(loaded.scope_hotkeys.is_empty());
    }

    #[test]
    fn disabled_providers_are_reported() {
        let settings = Settings {
            disabled_providers: vec!["web".into()],
            ..Default::default()
        };
        assert!(!settings.provider_enabled("web"));
        assert!(settings.provider_enabled("apps"));
    }
}
