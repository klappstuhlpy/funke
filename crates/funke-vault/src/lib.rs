//! Bitwarden/Vaultwarden vault provider (M4).
//!
//! All vault crypto stays in the official CLI: we spawn `bw serve` on a random
//! localhost port and talk REST to it (see docs/PLAN.md §4 — never reimplement the
//! client protocol by hand). The `serve` process holds the unlocked state; this crate
//! caches only non-secret fields (names, usernames, URI hosts, a has-TOTP flag, the
//! organization label) for fuzzy search and fetches secrets by id at action time, so
//! passwords never ride inside `ResultItem`s, the recents store, or the webview.
//!
//! Privacy/security posture:
//! - `prefix_only`: entries appear for `v <query>` searches, never global ones.
//! - Auto-lock after [`IDLE_LOCK`] without vault use (`POST /lock` + cache wipe; with
//!   a persisted Hello session the server is killed instead — a `bw lock` would
//!   invalidate the stored session key).
//! - `bw serve` binds 127.0.0.1 and dies with the launcher ([`Vault::shutdown`]).
//! - Windows Hello unlock (opt-in, [`hello`]): a DPAPI-protected session key lets
//!   repeat unlocks skip the master password behind a Hello consent prompt.
//! - Website icons (opt-in-out): favicons come from the configured server's icon
//!   service, cached in memory only. See SECURITY.md for both tradeoffs.

mod hello;
mod provider;
mod serve;

pub use provider::VaultProvider;

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use funke_core::Settings;
use zeroize::Zeroize;

/// Lock the vault after this much time without a vault query or action.
const IDLE_LOCK: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VaultStatus {
    /// `ensure_started` hasn't been called yet.
    Idle,
    /// Background startup (CLI check + `bw serve` boot) in progress.
    Starting,
    /// No `bw` CLI on PATH.
    NoCli,
    /// CLI present but `bw login` has never been run.
    Unauthenticated,
    Locked,
    Unlocked,
}

/// One cached vault entry — deliberately no secrets (see crate docs).
#[derive(Debug, Clone)]
pub struct VaultEntry {
    pub id: String,
    pub name: String,
    pub username: Option<String>,
    pub host: Option<String>,
    /// The item has a TOTP seed (the seed itself never leaves the CLI).
    pub has_totp: bool,
    /// Organization name when the item lives in a shared vault; `None` = personal.
    pub organization: Option<String>,
}

pub struct Credentials {
    pub username: Option<String>,
    pub password: Option<String>,
}

impl Drop for Credentials {
    fn drop(&mut self) {
        if let Some(username) = self.username.as_mut() {
            username.zeroize();
        }
        if let Some(password) = self.password.as_mut() {
            password.zeroize();
        }
    }
}

pub struct Vault {
    status: Mutex<VaultStatus>,
    /// Set once `bw serve` responds; the whole REST surface hangs off this port.
    port: Mutex<Option<u16>>,
    child: Mutex<Option<Child>>,
    entries: RwLock<Vec<VaultEntry>>,
    /// Per-host favicon data URLs; a cached `None` means "tried, no icon" (no retry).
    icons: RwLock<HashMap<String, Option<String>>>,
    icons_pending: Mutex<HashSet<String>>,
    /// Called (off the query thread) when a background favicon fetch populates the
    /// cache, so the host can nudge the overlay to re-render. The app installs an
    /// event-emitting closure; the vault stays UI-agnostic.
    icons_listener: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// From `/status` — decides which icon service serves the favicons.
    server_url: Mutex<Option<String>>,
    settings: Arc<RwLock<Settings>>,
    last_used: Mutex<Instant>,
    watchdog_running: AtomicBool,
}

impl Default for Vault {
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

impl Vault {
    pub fn new(settings: Arc<RwLock<Settings>>) -> Self {
        Self {
            status: Mutex::new(VaultStatus::Idle),
            port: Mutex::new(None),
            child: Mutex::new(None),
            entries: RwLock::new(Vec::new()),
            icons: RwLock::new(HashMap::new()),
            icons_pending: Mutex::new(HashSet::new()),
            icons_listener: Mutex::new(None),
            server_url: Mutex::new(None),
            settings,
            last_used: Mutex::new(Instant::now()),
            watchdog_running: AtomicBool::new(false),
        }
    }

    pub fn status(&self) -> VaultStatus {
        *self.status.lock().unwrap()
    }

    #[cfg(test)]
    pub(crate) fn force_status(&self, status: VaultStatus) {
        *self.status.lock().unwrap() = status;
    }

    #[cfg(test)]
    pub(crate) fn force_entries(&self, entries: Vec<VaultEntry>) {
        *self.entries.write().unwrap() = entries;
    }

    pub fn entries(&self) -> Vec<VaultEntry> {
        self.entries.read().unwrap().clone()
    }

    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    /// Boot `bw serve` on a background thread (finding the CLI and waiting for the
    /// server must never block a keystroke or startup). Called at app launch when the
    /// provider is enabled, and again by every vault query as the lazy fallback.
    /// Idempotent.
    pub fn ensure_started(self: &Arc<Self>) {
        let mut status = self.status.lock().unwrap();
        if *status != VaultStatus::Idle {
            return;
        }
        *status = VaultStatus::Starting;
        drop(status);

        let vault = Arc::clone(self);
        std::thread::spawn(move || {
            let outcome = serve::start();
            let mut status = vault.status.lock().unwrap();
            match outcome {
                Err(serve::StartError::NoCli) => *status = VaultStatus::NoCli,
                Err(serve::StartError::Failed(e)) => {
                    eprintln!("bw serve failed to start: {e}");
                    *status = VaultStatus::NoCli;
                }
                Ok((child, port, info)) => {
                    *vault.child.lock().unwrap() = Some(child);
                    *vault.port.lock().unwrap() = Some(port);
                    *vault.server_url.lock().unwrap() = info.server_url;
                    *status = match info.status.as_str() {
                        "unauthenticated" => VaultStatus::Unauthenticated,
                        "unlocked" => VaultStatus::Unlocked, // pre-unlocked serve is unusual but possible
                        _ => VaultStatus::Locked,
                    };
                }
            }
        });
    }

    /// Unlock via `POST /unlock`, then cache the searchable (non-secret) item fields.
    /// With the Hello setting on, additionally mint and persist a session key so the
    /// next unlock is a Hello prompt instead of the master password.
    /// The caller zeroizes the password.
    pub fn unlock(self: &Arc<Self>, password: &str) -> Result<(), String> {
        let port = match self.port() {
            Some(port) => port,
            // A Hello-mode lock kills `bw serve`; bring it back before unlocking.
            None if self.status() == VaultStatus::Locked => self.respawn_serve(None)?.0,
            None => return Err("The vault backend isn't running yet — try again in a moment".into()),
        };
        serve::unlock(port, password)?;
        if self.settings.read().unwrap().vault_hello {
            // Runs the KDF a second time (bw unlock --raw); a failure only costs the
            // Hello shortcut, never the unlock itself.
            match serve::unlock_raw(password) {
                Ok(mut session) => {
                    if let Err(e) = hello::save_session(&session) {
                        eprintln!("failed to store the Windows Hello session: {e}");
                    }
                    session.zeroize();
                }
                Err(e) => eprintln!("could not mint a Windows Hello session key: {e}"),
            }
        }
        self.finish_unlock(port);
        Ok(())
    }

    /// Unlock without the master password: Windows Hello consent prompt (parented to
    /// `hwnd`), then redeem the DPAPI-protected session key by respawning `bw serve`
    /// pre-unlocked with it.
    pub fn hello_unlock(self: &Arc<Self>, hwnd: isize) -> Result<(), String> {
        if !self.hello_ready() {
            return Err("Windows Hello unlock isn't set up — unlock with your master password once".into());
        }
        hello::verify(hwnd, "Unlock your Bitwarden vault")?;
        let mut session = hello::load_session()?;
        let respawned = self.respawn_serve(Some(&session));
        session.zeroize();
        let (port, status) = respawned?;
        if status != "unlocked" {
            // Invalidated elsewhere (bw lock/logout, a newer unlock) — back to the
            // password prompt; the next master-password unlock mints a fresh key.
            hello::forget_session();
            *self.status.lock().unwrap() = VaultStatus::Locked;
            return Err("The saved session has expired — unlock with your master password to refresh it".into());
        }
        self.finish_unlock(port);
        Ok(())
    }

    /// A persisted Hello session exists and the setting is on — the unlock row may
    /// offer Windows Hello.
    pub fn hello_ready(&self) -> bool {
        self.settings.read().unwrap().vault_hello && hello::has_session()
    }

    /// Drop the persisted Hello session (the settings toggle was switched off).
    pub fn forget_hello_session(&self) {
        hello::forget_session();
    }

    /// `POST /lock` and forget everything cached. With a persisted Hello session the
    /// server is killed instead: `bw lock` would invalidate the stored session key,
    /// and killing wipes the server's memory just as thoroughly.
    pub fn lock(&self) {
        if hello::has_session() {
            if let Some(mut child) = self.child.lock().unwrap().take() {
                let _ = child.kill();
            }
            *self.port.lock().unwrap() = None;
        } else if let Some(port) = self.port() {
            let _ = serve::lock(port);
        }
        self.entries.write().unwrap().clear();
        self.icons.write().unwrap().clear();
        let mut status = self.status.lock().unwrap();
        if *status == VaultStatus::Unlocked {
            *status = VaultStatus::Locked;
        }
    }

    /// Fetch one item's credentials at action time (never cached).
    pub fn credentials(&self, id: &str) -> Result<Credentials, String> {
        if self.status() != VaultStatus::Unlocked {
            return Err("The vault is locked".into());
        }
        let port = self.port().ok_or("The vault backend isn't running")?;
        self.touch();
        serve::item_credentials(port, id)
    }

    /// Current TOTP code for an item, computed by the CLI at action time.
    pub fn totp(&self, id: &str) -> Result<String, String> {
        if self.status() != VaultStatus::Unlocked {
            return Err("The vault is locked".into());
        }
        let port = self.port().ok_or("The vault backend isn't running")?;
        self.touch();
        serve::item_totp(port, id)
    }

    /// Install the re-render nudge fired after background favicon fetches land.
    pub fn set_icons_listener(&self, listener: impl Fn() + Send + Sync + 'static) {
        *self.icons_listener.lock().unwrap() = Some(Box::new(listener));
    }

    /// Cached favicon for a host, if a fetch already succeeded.
    pub fn icon_for(&self, host: &str) -> Option<String> {
        self.icons.read().unwrap().get(host).cloned().flatten()
    }

    /// Queue favicon fetches for hosts not tried yet; rows pick the icons up on a
    /// later render. No-op when the icons setting is off.
    pub fn request_icons(self: &Arc<Self>, hosts: Vec<String>) {
        if !self.settings.read().unwrap().vault_icons {
            return;
        }
        let todo: Vec<String> = {
            let known = self.icons.read().unwrap();
            let mut pending = self.icons_pending.lock().unwrap();
            hosts
                .into_iter()
                .filter(|host| !known.contains_key(host) && pending.insert(host.clone()))
                .collect()
        };
        if todo.is_empty() {
            return;
        }
        let base = self.icon_base();
        let vault = Arc::clone(self);
        std::thread::spawn(move || {
            let mut any = false;
            for host in todo {
                let icon = serve::fetch_icon(&base, &host);
                any |= icon.is_some();
                vault.icons.write().unwrap().insert(host.clone(), icon);
                vault.icons_pending.lock().unwrap().remove(&host);
            }
            // Nudge the overlay to re-render so freshly-fetched icons appear without
            // waiting for the next keystroke (or a close/reopen).
            if any {
                if let Some(listener) = vault.icons_listener.lock().unwrap().as_ref() {
                    listener();
                }
            }
        });
    }

    /// The official cloud uses Bitwarden's icon CDN; self-hosted servers (Vaultwarden)
    /// serve `/icons` themselves.
    fn icon_base(&self) -> String {
        match self.server_url.lock().unwrap().as_deref() {
            None | Some("") => "https://icons.bitwarden.net".into(),
            Some(url) if url.contains("bitwarden.com") => "https://icons.bitwarden.net".into(),
            Some(url) if url.contains("bitwarden.eu") => "https://icons.bitwarden.eu".into(),
            Some(url) => format!("{}/icons", url.trim_end_matches('/')),
        }
    }

    /// Kill the `bw serve` child. Locks first so the server never outlives us unlocked
    /// (belt and braces — the process dies anyway).
    pub fn shutdown(&self) {
        self.lock();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
        }
    }

    fn port(&self) -> Option<u16> {
        *self.port.lock().unwrap()
    }

    /// Replace the `bw serve` child (killing any current one) — with a session it
    /// comes up pre-unlocked. Returns the new port and reported status string.
    fn respawn_serve(&self, session: Option<&str>) -> Result<(u16, String), String> {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
        }
        *self.port.lock().unwrap() = None;
        let outcome = match session {
            Some(session) => serve::start_with_session(session),
            None => serve::start(),
        };
        match outcome {
            Ok((child, port, info)) => {
                *self.child.lock().unwrap() = Some(child);
                *self.port.lock().unwrap() = Some(port);
                *self.server_url.lock().unwrap() = info.server_url;
                Ok((port, info.status))
            }
            Err(serve::StartError::NoCli) => Err("Bitwarden CLI not found".into()),
            Err(serve::StartError::Failed(e)) => Err(format!("bw serve failed to start: {e}")),
        }
    }

    /// Shared unlock tail: flip the status, cache the entries, arm the watchdog, and
    /// pull server-side changes in the background.
    fn finish_unlock(self: &Arc<Self>, port: u16) {
        *self.status.lock().unwrap() = VaultStatus::Unlocked;
        self.touch();
        self.refresh_entries(port);
        self.spawn_watchdog();

        let vault = Arc::clone(self);
        std::thread::spawn(move || {
            if serve::sync(port).is_ok() {
                vault.refresh_entries(port);
            }
        });
    }

    fn refresh_entries(&self, port: u16) {
        match serve::list_entries(port) {
            Ok(entries) => *self.entries.write().unwrap() = entries,
            Err(e) => eprintln!("vault item list failed: {e}"),
        }
    }

    /// Idle auto-lock: one background thread for the vault's lifetime.
    fn spawn_watchdog(self: &Arc<Self>) {
        if self.watchdog_running.swap(true, Ordering::SeqCst) {
            return;
        }
        let vault = Arc::clone(self);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            let idle = vault.last_used.lock().unwrap().elapsed();
            if vault.status() == VaultStatus::Unlocked && idle >= IDLE_LOCK {
                vault.lock();
            }
        });
    }
}
