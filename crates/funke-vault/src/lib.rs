//! Bitwarden/Vaultwarden vault provider (M4).
//!
//! All vault crypto stays in the official CLI: we spawn `bw serve` on a random
//! localhost port and talk REST to it (see docs/DESIGN.md §5 — never reimplement the
//! client protocol by hand). The `serve` process holds the unlocked state; this crate
//! caches only non-secret fields (names, usernames, URI hosts, a has-TOTP flag, the
//! organization label) for fuzzy search and fetches secrets by id at action time, so
//! passwords never ride inside `ResultItem`s, the recents store, or the webview.
//!
//! Privacy/security posture:
//! - `prefix_only`: entries appear for `v <query>` searches, never global ones.
//! - Auto-lock after `Settings::vault_idle_lock_minutes` without vault use, and (opt-in)
//!   the moment the user walks away — session lock, sleep/hibernate, RDP disconnect
//!   ([`session_events`], with [`lockscreen`]'s poll as fallback). Locking is
//!   `POST /lock` + cache wipe; with a persisted Hello session the server is killed
//!   instead — a `bw lock` would invalidate the stored session key.
//! - `bw serve` binds 127.0.0.1 and dies with the launcher — gracefully via
//!   [`Vault::shutdown`], and on a crash via the kill-on-close job object every serve
//!   child is assigned to ([`job`]): the kernel, not a destructor, bounds its lifetime.
//! - Windows Hello unlock (opt-in, [`hello`]): a DPAPI-protected session key lets
//!   repeat unlocks skip the master password behind a Hello consent prompt.
//! - Website icons (opt-in-out): favicons come from the configured server's icon
//!   service, cached in memory only. See SECURITY.md for both tradeoffs.

mod context;
mod hello;
mod job;
mod lockscreen;
mod provider;
mod secret_buf;
mod sequence;
mod serve;
mod session_events;

pub use context::{FocusContext, MIN_SUGGEST_SCORE};
pub use provider::{blocked_row, suggestions, VaultProvider};
pub use sequence::{password_onward, Step};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use funke_core::Settings;
use zeroize::Zeroize;

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
    /// The item's website, ready for a browser (`https://github.com/login`) — `None` when
    /// it has no web URI at all (an app-only entry). What "open website & autofill" opens.
    pub uri: Option<String>,
    /// The item has a TOTP seed (the seed itself never leaves the CLI).
    pub has_totp: bool,
    /// Organization name when the item lives in a shared vault; `None` = personal.
    pub organization: Option<String>,
    /// This entry's own autotype sequence, from its `autotype` custom field — a template
    /// (`{USERNAME}{TAB}{PASSWORD}{ENTER}`), never a secret. See [`sequence`].
    pub autotype: Option<String>,
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
    /// The serve child and its kill-on-close job — dropping the slot is itself a kill.
    child: Mutex<Option<serve::ServeProcess>>,
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

    /// One cached entry by id — its name, site and URI, never its secrets. `None` once the
    /// vault locks (the cache is wiped) or if the item is gone from the server.
    pub fn entry(&self, id: &str) -> Option<VaultEntry> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
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
        // Page-locked while it waits for `bw serve` to boot; zeroized+freed on drop.
        let session = hello::load_session()?;
        let respawned = self.respawn_serve(Some(session.as_str()?));
        drop(session);
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

    /// Whether the overlay's empty state may offer the credential for the focused app.
    pub fn context_suggest_enabled(&self) -> bool {
        self.settings.read().unwrap().vault_context_suggest
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

    /// The autotype sequence for an item, most specific template first: the entry's own
    /// `autotype` custom field, else the user's default from settings, else the built-in
    /// username ⇥ password (with the trailing Enter the `vault_autotype_enter` setting
    /// asks for). Steps name the fields they need — the caller resolves the secrets.
    pub fn autotype_steps(&self, id: &str) -> Vec<Step> {
        let per_entry = self
            .entries
            .read()
            .unwrap()
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.autotype.clone());

        let (default_sequence, press_enter) = {
            let settings = self.settings.read().unwrap();
            (
                settings.vault_autotype_sequence.trim().to_string(),
                settings.vault_autotype_enter,
            )
        };

        match per_entry.or_else(|| (!default_sequence.is_empty()).then_some(default_sequence)) {
            // An explicit template is typed exactly as written — the Enter toggle governs
            // the built-in sequence only, so a user-authored one can't be second-guessed.
            Some(template) => sequence::parse(&template),
            None => {
                let mut steps = sequence::parse(sequence::DEFAULT);
                if press_enter {
                    steps.push(Step::Enter);
                }
                steps
            }
        }
    }

    /// Context boost per entry id: how strongly each entry belongs to the window that was
    /// focused when the overlay was summoned (see [`context`]). Empty while locked —
    /// there is no cache to match against.
    pub fn context_scores(&self, focus: &FocusContext) -> HashMap<String, i64> {
        self.entries
            .read()
            .unwrap()
            .iter()
            .filter_map(|entry| context::score(entry, focus).map(|score| (entry.id.clone(), score)))
            .collect()
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

    /// Idle + screen-lock auto-lock: one background thread for the vault's lifetime.
    /// Both triggers read live settings each tick, so changing them takes effect without
    /// a restart.
    fn spawn_watchdog(self: &Arc<Self>) {
        if self.watchdog_running.swap(true, Ordering::SeqCst) {
            return;
        }
        // The message-driven twin of the poll below: session lock, RDP disconnect, and
        // suspend/resume arrive as events the moment they happen, where the poll would
        // notice the lock case alone, up to 30 s late. Same setting governs both — all
        // four are the user walking away.
        let events_vault = Arc::clone(self);
        session_events::watch(move || {
            if events_vault.settings.read().unwrap().vault_lock_on_screen_lock
                && events_vault.status() == VaultStatus::Unlocked
            {
                events_vault.lock();
            }
        });
        let vault = Arc::clone(self);
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            if vault.status() != VaultStatus::Unlocked {
                continue;
            }
            let (idle_minutes, lock_on_screen) = {
                let settings = vault.settings.read().unwrap();
                (settings.vault_idle_lock_minutes, settings.vault_lock_on_screen_lock)
            };
            if lock_on_screen && lockscreen::workstation_locked() {
                vault.lock();
                continue;
            }
            if idle_minutes > 0 && vault.last_used.lock().unwrap().elapsed() >= Duration::from_secs(idle_minutes * 60) {
                vault.lock();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vault_with(settings: Settings, entry: VaultEntry) -> Arc<Vault> {
        let vault = Arc::new(Vault::new(Arc::new(RwLock::new(settings))));
        vault.force_entries(vec![entry]);
        vault
    }

    fn login(autotype: Option<&str>) -> VaultEntry {
        VaultEntry {
            id: "uuid-1".into(),
            name: "GitHub".into(),
            username: Some("ben".into()),
            host: Some("github.com".into()),
            uri: Some("https://github.com/login".into()),
            has_totp: true,
            organization: None,
            autotype: autotype.map(str::to_string),
        }
    }

    #[test]
    fn the_built_in_sequence_honours_the_trailing_enter_toggle() {
        let vault = vault_with(Settings::default(), login(None));
        assert_eq!(
            vault.autotype_steps("uuid-1"),
            vec![Step::Username, Step::Tab, Step::Password, Step::Enter]
        );

        let no_enter = Settings {
            vault_autotype_enter: false,
            ..Default::default()
        };
        let vault = vault_with(no_enter, login(None));
        assert_eq!(
            vault.autotype_steps("uuid-1"),
            vec![Step::Username, Step::Tab, Step::Password]
        );
    }

    #[test]
    fn an_entrys_own_sequence_beats_the_default_which_beats_the_built_in_one() {
        let settings = Settings {
            vault_autotype_sequence: "{PASSWORD}{ENTER}".into(),
            // Explicit templates are typed as written — this toggle must not touch them.
            vault_autotype_enter: false,
            ..Default::default()
        };
        let vault = vault_with(settings.clone(), login(None));
        assert_eq!(vault.autotype_steps("uuid-1"), vec![Step::Password, Step::Enter]);

        let vault = vault_with(settings, login(Some("{USERNAME}{TAB}{PASSWORD}{TAB}{TOTP}{ENTER}")));
        assert_eq!(
            vault.autotype_steps("uuid-1"),
            vec![
                Step::Username,
                Step::Tab,
                Step::Password,
                Step::Tab,
                Step::Totp,
                Step::Enter
            ]
        );
    }

    #[test]
    fn an_unknown_id_still_yields_the_default_sequence() {
        // The item vanished between the query and the keypress: type the usual thing
        // rather than nothing (the credentials fetch is what will fail, loudly).
        let vault = vault_with(Settings::default(), login(None));
        assert_eq!(vault.autotype_steps("gone").first(), Some(&Step::Username));
    }
}
