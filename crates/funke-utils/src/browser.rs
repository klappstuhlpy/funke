//! The default browser's icon, resolved once for the whole process.
//!
//! Two providers put it on their rows — web search and quicklinks — because both of them end
//! in the browser, and a row should wear the face of the thing Enter opens. Resolving it means
//! a registry lookup and a COM call, which is why it happens on a background thread and why it
//! happens exactly once: rows render icon-less for the instant before it lands, and never wait
//! for it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

static ICON: OnceLock<Option<String>> = OnceLock::new();

/// Start resolving, if nobody has. Idempotent — both providers call it from their
/// constructors and only the first does any work.
pub(crate) fn resolve() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        let _ = ICON.set(funke_shell::default_browser_icon());
    });
}

/// The icon, or `None` while it is still being looked up (or if the lookup found nothing).
pub(crate) fn icon() -> Option<String> {
    ICON.get().cloned().flatten()
}
