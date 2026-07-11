//! Clipboard history: an in-memory ring of what you copied, searchable behind the `c`
//! prefix.
//!
//! **Nothing here is ever written to disk.** A file of everything you have ever copied is
//! the worst artifact this app could leave behind — it catches passwords, tokens and 2FA
//! codes by nature — so the history lives in the process and dies with it. The cost is
//! honest and small: restart Funke, and the history is empty.
//!
//! Three filters stand between the clipboard and this ring, in order of how much they can
//! be trusted:
//!
//! 1. **The exclusion markers** (exact). Funke's own vault copies set them
//!    ([`win::write_secret`]) and so do other password managers, so those copies never
//!    reach us at all — [`win::read_text`] returns `None` for them.
//! 2. **The shape heuristic** ([`secret::looks_like_secret`], guesswork). Catches the
//!    accident nobody marked: an API key out of a dashboard, a token out of a terminal.
//! 3. **The cap.** [`MAX_CLIPS`] entries, oldest evicted, each zeroized on the way out.
//!
//! The provider is `prefix_only` for the same reason the vault is: what you copied is
//! nobody's business until you ask for it, least of all a bystander's while you type an
//! ordinary search.

mod provider;
mod secret;
mod win;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use funke_core::Settings;
use zeroize::Zeroize;

pub use provider::{ClipboardProvider, PROVIDER_ID};
pub use win::{read_text, write_secret, write_text};

/// How many clips are kept. Past this the oldest is evicted (and zeroized).
pub const MAX_CLIPS: usize = 100;
/// Clips longer than this are not recorded: at that size it is a document, not something
/// you meant to keep re-pasting, and it would sit in memory for the life of the process.
const MAX_CLIP_BYTES: usize = 64 * 1024;

/// One thing you copied.
#[derive(Debug, Clone)]
pub struct Clip {
    /// Stable for the life of the process — what an action refers back to.
    pub id: u64,
    pub text: String,
    /// Unix seconds, from the caller (so tests stay deterministic, as elsewhere).
    pub copied_at: u64,
}

impl Drop for Clip {
    fn drop(&mut self) {
        // Best effort: shrink the window in which an evicted clip's bytes still sit in
        // the heap. It cannot undo copies the allocator or the OS already made.
        self.text.zeroize();
    }
}

/// The ring, plus the listener that fills it.
pub struct ClipboardHistory {
    clips: Mutex<VecDeque<Clip>>,
    next_id: AtomicU64,
    settings: Arc<RwLock<Settings>>,
}

impl ClipboardHistory {
    pub fn new(settings: Arc<RwLock<Settings>>) -> Arc<Self> {
        Arc::new(Self {
            clips: Mutex::new(VecDeque::new()),
            next_id: AtomicU64::new(1),
            settings,
        })
    }

    /// Start recording. The listener runs on its own thread (see [`win::watch`]); this
    /// returns immediately.
    pub fn watch(self: &Arc<Self>) {
        let history = Arc::downgrade(self);
        win::watch(move || {
            if let Some(history) = history.upgrade() {
                history.capture();
            }
        });
    }

    /// A clipboard change arrived. Read it, judge it, keep it — or don't.
    fn capture(&self) {
        // The provider toggle governs the *recording*, not just the searching: switching
        // the clipboard off in Settings has to mean nothing is being kept, or the toggle
        // is a lie.
        if !self.settings.read().unwrap().provider_enabled(PROVIDER_ID) {
            return;
        }
        match win::read() {
            win::Read::Text(text) => {
                self.record(text, unix_now());
            }
            // Deliberately nothing to keep: somebody's secret, or an image.
            win::Read::Excluded | win::Read::NotText => {}
            // We lost the race for the clipboard lock. Every other clipboard manager woke
            // on this same notification and grabbed it too, so this is ordinary, not
            // exceptional — and dropping the clip would leave a hole in the history for
            // no reason. The content is still there; come back for it.
            win::Read::Busy => {
                if let win::Read::Text(text) = win::read() {
                    self.record(text, unix_now());
                } else {
                    eprintln!("clipboard: content was locked by another app — clip not recorded");
                }
            }
        }
    }

    /// Push a clip, applying the length, blank, secret and duplicate rules. Split out of
    /// [`capture`](Self::capture) so it can be tested without a clipboard.
    fn record(&self, text: String, now: u64) -> bool {
        if text.trim().is_empty() || text.len() > MAX_CLIP_BYTES {
            return false;
        }
        if secret::looks_like_secret(&text) {
            return false;
        }
        let mut clips = self.clips.lock().unwrap();
        // Re-copying something you already have moves it to the front rather than
        // filling the history with the same string.
        if let Some(index) = clips.iter().position(|clip| clip.text == text) {
            if let Some(mut clip) = clips.remove(index) {
                clip.copied_at = now;
                clips.push_front(clip);
            }
            return true;
        }
        clips.push_front(Clip {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            text,
            copied_at: now,
        });
        while clips.len() > MAX_CLIPS {
            clips.pop_back(); // Drop zeroizes.
        }
        true
    }

    /// Every clip, most recently copied first.
    pub fn clips(&self) -> Vec<Clip> {
        self.clips.lock().unwrap().iter().cloned().collect()
    }

    /// Drop one clip (the ✕ / "Remove from history" action).
    pub fn forget(&self, id: u64) -> bool {
        let mut clips = self.clips.lock().unwrap();
        let before = clips.len();
        clips.retain(|clip| clip.id != id);
        clips.len() != before
    }

    /// Drop everything — the panic button, and what the app calls on vault lock.
    pub fn clear(&self) {
        self.clips.lock().unwrap().clear();
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history() -> Arc<ClipboardHistory> {
        ClipboardHistory::new(Arc::new(RwLock::new(Settings::default())))
    }

    #[test]
    fn clips_stack_newest_first_and_re_copying_moves_to_the_front() {
        let history = history();
        assert!(history.record("first".into(), 100));
        assert!(history.record("second".into(), 200));
        assert_eq!(
            history.clips().iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
            ["second", "first"]
        );

        // Copying "first" again promotes it — it does not appear twice.
        assert!(history.record("first".into(), 300));
        let clips = history.clips();
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0].text, "first");
        assert_eq!(clips[0].copied_at, 300, "and it carries the newer timestamp");
    }

    #[test]
    fn secrets_blanks_and_giants_are_never_recorded() {
        let history = history();
        assert!(!history.record("ghp_16C7e42F292c6912E7710c838347Ae178B4a".into(), 1));
        assert!(!history.record("   \n ".into(), 1));
        assert!(!history.record("x".repeat(MAX_CLIP_BYTES + 1), 1));
        assert!(history.clips().is_empty());

        // …but the ordinary copy right after them still lands.
        assert!(history.record("cargo test --workspace".into(), 2));
        assert_eq!(history.clips().len(), 1);
    }

    #[test]
    fn the_ring_is_capped_and_forgets_the_oldest() {
        let history = history();
        for i in 0..MAX_CLIPS + 10 {
            assert!(history.record(format!("clip {i}"), i as u64));
        }
        let clips = history.clips();
        assert_eq!(clips.len(), MAX_CLIPS);
        assert_eq!(clips[0].text, format!("clip {}", MAX_CLIPS + 9), "newest kept");
        assert!(
            !clips.iter().any(|clip| clip.text == "clip 0"),
            "the oldest is gone, not merely hidden"
        );
    }

    #[test]
    fn recording_stops_when_the_provider_is_switched_off() {
        let settings = Arc::new(RwLock::new(Settings {
            disabled_providers: vec![PROVIDER_ID.into()],
            ..Default::default()
        }));
        let history = ClipboardHistory::new(Arc::clone(&settings));
        history.capture(); // The real entry point: it must bail before reading anything.
        assert!(history.clips().is_empty());
    }

    #[test]
    fn forget_and_clear_remove_clips() {
        let history = history();
        history.record("keep".into(), 1);
        history.record("drop".into(), 2);
        let doomed = history.clips().iter().find(|c| c.text == "drop").unwrap().id;

        assert!(history.forget(doomed));
        assert!(!history.forget(doomed), "forgetting it twice is a no-op");
        assert_eq!(history.clips().len(), 1);

        history.clear();
        assert!(history.clips().is_empty());
    }
}
