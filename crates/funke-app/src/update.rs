//! Update checks — finding out, being told once, and deciding for yourself.
//!
//! Three rules shape this:
//!
//! 1. **Checking is not installing.** The button used to do both: one click and the update
//!    was downloaded and staged, with no version shown and nothing to say no to. Now the
//!    check reports what it found and stops; installing is a second, separate press.
//! 2. **Told once, not nagged.** A background check at startup raises a Windows
//!    notification the *first* time it sees a given version — the version it told you about
//!    is written to a marker file, so the same release never knocks twice, however often
//!    Funke restarts. A newer one will.
//! 3. **Nothing happens behind the user's back.** The background check is a network request
//!    Funke makes without being asked, so it is a setting (`Settings::update_check`) and it
//!    is in SECURITY.md. With the updater unconfigured (as it is until the release keypair
//!    exists) it never even reaches the network.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

/// A release newer than the one running. Deliberately just *facts* — the decision is the
/// user's, so this crosses to the frontend as something to read, not something to obey.
#[derive(serde::Serialize, Clone)]
pub struct Available {
    pub version: String,
    /// The release notes, as the release published them. May be empty.
    pub notes: String,
}

/// Ask the update endpoint what's out there. `None` = already current.
pub async fn check(app: &AppHandle) -> Result<Option<Available>, String> {
    let updater = app.updater().map_err(|_| unconfigured())?;
    let found = updater.check().await.map_err(|e| e.to_string())?;
    Ok(found.map(|update| Available {
        version: update.version.clone(),
        notes: update.body.clone().unwrap_or_default(),
    }))
}

/// Download and install — the user pressed the second button.
///
/// Re-checks rather than holding the `Update` from the earlier press: it is one request,
/// and it means what gets installed is what is current *now*, not what was current when the
/// settings window happened to be opened. If the release vanished in between, say so
/// instead of installing a ghost.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|_| unconfigured())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| funke_core::t("update.none").to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())
}

/// The startup check: notify once per version, then never again for that version.
///
/// Failure is silence. A dormant updater, no network, a rate-limited endpoint — none of it
/// is the user's problem at sign-in, and none of it is worth a dialog.
pub fn notify_if_new(app: &AppHandle, seen: PathBuf) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Sign-in belongs to the app index and the vault. An update check can wait its turn.
        std::thread::sleep(Duration::from_secs(10));
        tauri::async_runtime::spawn(async move {
            match check(&app).await {
                Ok(Some(update)) => {
                    if told_about(&seen) == update.version {
                        return;
                    }
                    let current = app.package_info().version.to_string();
                    let shown = app
                        .notification()
                        .builder()
                        .title(funke_core::tf("update.notify.title", &[("version", &update.version)]))
                        .body(funke_core::tf("update.notify.body", &[("current", &current)]))
                        .show();
                    // Only remember it if it was actually said. A notification Windows
                    // swallowed (no Start-menu shortcut, focus assist) must not count as
                    // having told anyone — it would silence the one release they'd have got.
                    match shown {
                        Ok(()) => remember(&seen, &update.version),
                        Err(e) => eprintln!("update notification not shown: {e}"),
                    }
                }
                Ok(None) => {}
                Err(e) => eprintln!("update check skipped: {e}"),
            }
        });
    });
}

fn unconfigured() -> String {
    funke_core::t("update.unconfigured").to_string()
}

/// The version we last raised a notification for; empty if we never have.
fn told_about(seen: &Path) -> String {
    std::fs::read_to_string(seen)
        .map(|version| version.trim().to_string())
        .unwrap_or_default()
}

fn remember(seen: &Path, version: &str) {
    if let Some(parent) = seen.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(seen, version) {
        // Worst case the same notification appears once more on the next launch. Annoying,
        // not broken — so it is a warning, not a failure.
        eprintln!("could not remember the announced update version: {e}");
    }
}
