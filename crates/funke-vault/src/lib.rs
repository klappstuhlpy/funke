//! Bitwarden/Vaultwarden vault provider (M4).
//!
//! All vault crypto stays in the official CLI: we spawn `bw serve` on a random
//! localhost port and talk REST to it (see docs/PLAN.md §4 — never reimplement the
//! client protocol by hand). The `serve` process holds the unlocked state; this crate
//! caches only non-secret fields (names, usernames, URI hosts) for fuzzy search and
//! fetches secrets by id at action time, so passwords never ride inside `ResultItem`s,
//! the recents store, or the webview.
//!
//! Privacy/security posture:
//! - `prefix_only`: entries appear for `v <query>` searches, never global ones.
//! - Auto-lock after [`IDLE_LOCK`] without vault use (`POST /lock` + cache wipe).
//! - `bw serve` binds 127.0.0.1 and dies with the launcher ([`Vault::shutdown`]).

mod provider;
mod serve;

pub use provider::VaultProvider;

use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

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
    last_used: Mutex<Instant>,
    watchdog_running: AtomicBool,
}

impl Default for Vault {
    fn default() -> Self {
        Self::new()
    }
}

impl Vault {
    pub fn new() -> Self {
        Self {
            status: Mutex::new(VaultStatus::Idle),
            port: Mutex::new(None),
            child: Mutex::new(None),
            entries: RwLock::new(Vec::new()),
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

    pub fn entries(&self) -> Vec<VaultEntry> {
        self.entries.read().unwrap().clone()
    }

    pub fn touch(&self) {
        *self.last_used.lock().unwrap() = Instant::now();
    }

    /// First vault query: boot `bw serve` on a background thread (finding the CLI and
    /// waiting for the server must never block a keystroke). Idempotent.
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
                Ok((child, port, server_status)) => {
                    *vault.child.lock().unwrap() = Some(child);
                    *vault.port.lock().unwrap() = Some(port);
                    *status = match server_status.as_str() {
                        "unauthenticated" => VaultStatus::Unauthenticated,
                        "unlocked" => VaultStatus::Unlocked, // pre-unlocked serve is unusual but possible
                        _ => VaultStatus::Locked,
                    };
                }
            }
        });
    }

    /// Unlock via `POST /unlock`, then cache the searchable (non-secret) item fields.
    /// The caller zeroizes the password.
    pub fn unlock(self: &Arc<Self>, password: &str) -> Result<(), String> {
        let port = self
            .port()
            .ok_or("The vault backend isn't running yet — try again in a moment")?;
        serve::unlock(port, password)?;
        *self.status.lock().unwrap() = VaultStatus::Unlocked;
        self.touch();
        self.refresh_entries(port);
        self.spawn_watchdog();

        // Pull server-side changes in the background, then refresh the cache again.
        let vault = Arc::clone(self);
        std::thread::spawn(move || {
            if serve::sync(port).is_ok() {
                vault.refresh_entries(port);
            }
        });
        Ok(())
    }

    /// `POST /lock` and forget everything cached.
    pub fn lock(&self) {
        if let Some(port) = self.port() {
            let _ = serve::lock(port);
        }
        self.entries.write().unwrap().clear();
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
