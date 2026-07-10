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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Space".into(),
            accent: "#d97757".into(),
            overlay_width: 680.0,
            web_engine: "duckduckgo".into(),
            disabled_providers: Vec::new(),
            index_roots: Vec::new(),
            autostart: false,
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
