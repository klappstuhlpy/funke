//! Saved snippets (`s` prefix): text you paste often — an email signature, an address, a
//! code block — found by name or abbreviation and typed into the window you came from.
//!
//! Snippets are stored in `Settings` (so they persist, back up and sync with the rest of
//! your preferences, and need no store of their own) and edited in Settings → Snippets.
//! The content may carry placeholders resolved *at paste time* — see [`expand`].
//!
//! Pasting reuses the seam vault autotype opened: take the foreground HWND captured
//! before the overlay appeared, refocus it, and put the text in. Like the clipboard, the
//! text goes in via Ctrl+V rather than synthesized keystrokes — a snippet is very often
//! multi-line, and typing a newline into a chat window sends the half-finished message.

mod expand;
mod provider;

pub use expand::{expand, expand_at, Context, Expansion};
pub use provider::{SnippetsProvider, PROVIDER_ID};

use funke_core::{Settings, Snippet};

/// Find a snippet by id in the settings — how the app resolves `SnippetPaste { id }` when
/// the action comes back from the frontend.
pub fn find<'a>(settings: &'a Settings, id: &str) -> Option<&'a Snippet> {
    settings.snippets.iter().find(|snippet| snippet.id == id)
}
