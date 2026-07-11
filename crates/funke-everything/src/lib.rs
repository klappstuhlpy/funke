//! Everything (voidtools) as a search backend: an always-current filename index, for free,
//! **if the user already runs Everything** — and nothing at all if they don't.
//!
//! Everything keeps a live index of every NTFS volume by reading the USN journal, which is
//! precisely the work `funke-files` cannot do yet (DESIGN.md §4, Phase B). So when it is running we ask
//! it instead of walking the disk ourselves: no index to build at startup, none to hold in
//! memory, and no minute-long staleness after a file appears. When it isn't, nothing changes
//! — [`is_running`] comes back false and the built-in index carries the feature as before.
//! It is detection, not a dependency: Everything is never installed, bundled, or required.
//!
//! The IPC is spoken directly ([`ipc`]) rather than through `Everything64.dll`, so there is
//! no third-party binary to ship and no license but ours in the tree.
//!
//! One caveat is worth knowing, because it changes what a query *means*: Everything matches
//! substrings (spaces are AND), not the fuzzy subsequences the built-in index allows. `rprt`
//! finds `report.txt` in the built-in index and nothing in Everything. Results are still
//! ranked by our own fuzzy scorer afterwards — Everything decides *which* files are
//! candidates, Funke decides what order you see them in.

mod ipc;

use std::sync::mpsc::{sync_channel, Receiver, Sender, SyncSender};
use std::thread;

pub use ipc::{is_running, Hit};

/// A query, and where to put the answer.
struct Request {
    search: String,
    max_results: u32,
    reply: SyncSender<Vec<Hit>>,
}

/// A client for a running Everything.
///
/// The Win32 side needs a window with a message pump, and pumping messages on the thread
/// that asked would mean pumping *Funke's* messages from inside a keystroke — reentrancy in
/// the middle of a search. So the window lives on a worker thread of its own, and callers
/// simply block on a channel until the answer (or the timeout) comes back.
pub struct Everything {
    requests: Sender<Request>,
}

impl Everything {
    pub fn spawn() -> Self {
        let (requests, incoming) = std::sync::mpsc::channel();
        thread::spawn(move || worker(incoming));
        Self { requests }
    }

    /// Ask Everything for up to `max_results` matches. Empty if Everything isn't running,
    /// the query found nothing, or it didn't answer in time — a caller treats all three the
    /// same way, by falling back to whatever it would have shown anyway.
    pub fn search(&self, search: &str, max_results: u32) -> Vec<Hit> {
        let search = search.trim();
        if search.is_empty() {
            return Vec::new();
        }
        // Rendezvous channel: the worker handles one query at a time, and a caller that has
        // already given up leaves nothing queued behind it.
        let (reply, answer) = sync_channel(0);
        let request = Request {
            search: search.to_string(),
            max_results,
            reply,
        };
        if self.requests.send(request).is_err() {
            return Vec::new(); // The worker is gone; Everything simply isn't available.
        }
        answer.recv().unwrap_or_default()
    }
}

fn worker(incoming: Receiver<Request>) {
    let Some(receiver) = ipc::Receiver::create() else {
        eprintln!("everything: no reply window; whole-disk search unavailable");
        return; // Senders now fail fast, which reads as "Everything unavailable".
    };
    while let Ok(request) = incoming.recv() {
        let hits = receiver.query(&request.search, request.max_results);
        // A caller that timed out and dropped its end is not an error worth reporting.
        let _ = request.reply.send(hits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real protocol, against the real Everything — the only test that can prove the
    /// hand-written wire format is right. Skipped (not failed) when Everything isn't
    /// running, which is the normal case on CI: the point is to be optional.
    #[test]
    fn a_running_everything_answers_a_real_query() {
        if !is_running() {
            eprintln!("skipped: Everything is not running");
            return;
        }
        let everything = Everything::spawn();

        // Windows always has these, on any machine this can run on.
        let hits = everything.search("explorer.exe", 25);
        assert!(!hits.is_empty(), "Everything found no explorer.exe");
        assert!(
            hits.iter().any(|hit| hit.path.to_lowercase().contains(r"\windows\")),
            "no hit under \\Windows\\: {hits:?}"
        );
        for hit in &hits {
            assert!(hit.path.ends_with(&hit.name), "{:?} does not end with its name", hit);
            assert!(hit.path.contains(':'), "{:?} is not an absolute path", hit);
        }

        // A folder comes back flagged as one, not guessed at from its name.
        let hits = everything.search("folder:windows", 10);
        assert!(hits.iter().all(|hit| hit.is_dir), "folder: search returned a file");
    }

    #[test]
    fn an_empty_search_never_reaches_the_ipc() {
        let everything = Everything::spawn();
        assert!(everything.search("   ", 10).is_empty());
    }
}
