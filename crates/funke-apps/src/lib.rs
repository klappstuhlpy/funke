//! Installed-application provider.
//!
//! Two sources, indexed on a background thread at startup:
//! - `Get-StartApps` (PowerShell): every Start Menu entry — classic desktop apps *and*
//!   UWP/Store apps — as name + AUMID, launched through `shell:AppsFolder\<AUMID>`.
//! - Executables on `PATH`, launched directly.
//!
//! Queries return nothing until the first index completes; that's by design (the
//! launcher must never block on indexing).

use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::{env, fs, thread};

use funke_core::{Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};
use serde::Deserialize;

#[derive(Debug, Clone)]
struct AppEntry {
    name: String,
    /// `shell:AppsFolder\<AUMID>` for Start apps, an absolute path for PATH executables.
    target: String,
    subtitle: String,
    icon: Option<String>,
}

pub struct AppsProvider {
    entries: Arc<RwLock<Vec<AppEntry>>>,
}

impl AppsProvider {
    /// Indexing shells out to PowerShell (a second or two), so it runs off-thread.
    /// The index is published twice: names first (searchable immediately), then again
    /// with icons once the slower shell extraction finishes.
    pub fn spawn() -> Self {
        let entries = Arc::new(RwLock::new(Vec::new()));
        let handle = Arc::clone(&entries);
        thread::spawn(move || {
            let indexed = build_index();
            *handle.write().unwrap() = indexed.clone();

            let mut with_icons = indexed;
            for entry in &mut with_icons {
                entry.icon = funke_shell::icon_data_url(&entry.target);
            }
            *handle.write().unwrap() = with_icons;
        });
        Self { entries }
    }
}

impl SearchProvider for AppsProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "apps",
            name: "Applications",
            prefix: None,
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let Some(matcher) = FuzzyMatcher::new(&query.text) else {
            return Vec::new();
        };
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .filter_map(|entry| {
                matcher.score(&entry.name).map(|score| ResultItem {
                    id: format!("apps:{}", entry.target),
                    provider: "apps".into(),
                    title: entry.name.clone(),
                    subtitle: Some(entry.subtitle.clone()),
                    icon: entry.icon.clone(),
                    score,
                    actions: vec![NamedAction::new(
                        "Open",
                        Action::LaunchApp {
                            target: entry.target.clone(),
                        },
                    )],
                })
            })
            .collect()
    }
}

fn build_index() -> Vec<AppEntry> {
    let mut entries = start_apps();
    // Start apps win name collisions; PATH mostly adds CLI tools not on the Start Menu.
    let mut seen: HashSet<String> = entries.iter().map(|e| e.name.to_lowercase()).collect();
    for exe in path_executables() {
        if seen.insert(exe.name.to_lowercase()) {
            entries.push(exe);
        }
    }
    entries
}

#[derive(Deserialize)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StartAppsJson {
    Many(Vec<StartApp>),
    One(StartApp),
}

/// Parse `Get-StartApps | ConvertTo-Json` output (a bare object, not an array, when
/// exactly one app exists).
fn parse_start_apps(json: &str) -> Vec<AppEntry> {
    let apps = match serde_json::from_str::<StartAppsJson>(json) {
        Ok(StartAppsJson::Many(apps)) => apps,
        Ok(StartAppsJson::One(app)) => vec![app],
        Err(_) => Vec::new(),
    };
    apps.into_iter()
        .map(|app| AppEntry {
            name: app.name,
            target: format!("shell:AppsFolder\\{}", app.app_id),
            subtitle: "Application".into(),
            icon: None,
        })
        .collect()
}

fn start_apps() -> Vec<AppEntry> {
    let mut command = Command::new("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        "Get-StartApps | ConvertTo-Json -Compress",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    match command.output() {
        Ok(out) if out.status.success() => parse_start_apps(&String::from_utf8_lossy(&out.stdout)),
        _ => Vec::new(),
    }
}

fn path_executables() -> Vec<AppEntry> {
    let Some(path_var) = env::var_os("PATH") else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for dir in env::split_paths(&path_var) {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for file in read.flatten() {
            let path = file.path();
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("exe")) {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    entries.push(AppEntry {
                        name: stem.to_string(),
                        target: path.to_string_lossy().into_owned(),
                        subtitle: path.to_string_lossy().into_owned(),
                        icon: None,
                    });
                }
            }
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_array_shape() {
        let json = r#"[{"Name":"Firefox","AppID":"Mozilla.Firefox"},{"Name":"Notepad","AppID":"App\\notepad"}]"#;
        let apps = parse_start_apps(json);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "Firefox");
        assert_eq!(apps[0].target, "shell:AppsFolder\\Mozilla.Firefox");
        assert_eq!(apps[1].target, "shell:AppsFolder\\App\\notepad");
    }

    #[test]
    fn parses_the_single_object_shape() {
        let json = r#"{"Name":"Solo","AppID":"solo.app"}"#;
        let apps = parse_start_apps(json);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].name, "Solo");
    }

    #[test]
    fn garbage_yields_an_empty_index() {
        assert!(parse_start_apps("").is_empty());
        assert!(parse_start_apps("not json").is_empty());
    }
}
