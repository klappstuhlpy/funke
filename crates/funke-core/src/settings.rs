//! App-wide user settings, persisted as one small JSON file (same contract as
//! [`FrecencyStore`](crate::FrecencyStore): a missing or corrupt file loads as the
//! defaults — losing preferences must never break the launcher). The struct is plain
//! data; applying a change (re-registering the hotkey, re-theming the overlay) is the
//! app crate's job.

use std::path::Path;
use std::{fs, io};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// UI language: `auto` (follow Windows), `en`, or `de`. Resolved to a
    /// [`crate::Locale`] by the app at startup and whenever this changes.
    pub language: String,
    /// Global summon hotkey in `tauri-plugin-global-shortcut` syntax, e.g. `Ctrl+Space`.
    pub hotkey: String,
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
    /// Start Funke with Windows.
    pub autostart: bool,
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

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            hotkey: "Ctrl+Space".into(),
            accent: "#d97757".into(),
            overlay_width: 680.0,
            web_engine: "duckduckgo".into(),
            disabled_providers: Vec::new(),
            index_roots: Vec::new(),
            autostart: false,
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
    fn disabled_providers_are_reported() {
        let settings = Settings {
            disabled_providers: vec!["web".into()],
            ..Default::default()
        };
        assert!(!settings.provider_enabled("web"));
        assert!(settings.provider_enabled("apps"));
    }
}
