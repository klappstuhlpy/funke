//! Bounded-wait search fan-out.
//!
//! The launcher's original rule was that every provider must answer from memory, because
//! one blocking `query()` held up the whole keystroke. That rule holds only for as long as
//! every provider can obey it — and some cannot: `bw serve` has to boot on the first `v`,
//! and a content search asks an index that lives in another process.
//!
//! So the deadline moves out of the providers and into the registry. The query is fanned
//! out on a worker thread per provider; whatever has answered when the deadline passes is
//! merged and returned, and the stragglers are handed to a callback as they land. Nothing
//! is interrupted — a provider that misses the deadline is *abandoned*, not cancelled, and
//! its rows are dropped on the floor if the user has typed on in the meantime. That is what
//! the generation counter is for: cancellation the user can feel, without providers having
//! to cooperate in it.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::{ProviderMeta, Query, Registry, ResultItem};

/// How long a keystroke waits for the fan-out before painting what it has.
///
/// Long enough that every in-memory provider lands inside it (they answer in single-digit
/// milliseconds), short enough that the wait is below what a person reads as a delay. A
/// provider that misses it is not slow by accident — it is talking to something else.
pub const DEFAULT_DEADLINE: Duration = Duration::from_millis(120);

impl Registry {
    /// Fan the query out on worker threads and merge everything that arrives within
    /// `deadline`, best score first.
    ///
    /// Keyword scoping works exactly as in [`search_enabled`](Registry::search_enabled) —
    /// including for a single scoped provider, which is the case that matters most: `v` on
    /// a cold vault is precisely the query that used to freeze the overlay.
    ///
    /// Rows from a provider that answers after the deadline go to `on_late`, ranked, tagged
    /// with the provider's id — *unless* `current_generation` has moved past `generation`
    /// by the time they land, which means the user has typed on and these rows answer a
    /// question nobody is asking anymore.
    pub fn search_streaming(
        &self,
        query: &Query,
        enabled: impl Fn(&ProviderMeta) -> bool,
        deadline: Duration,
        generation: u64,
        current_generation: Arc<AtomicU64>,
        on_late: impl Fn(&str, Vec<ResultItem>) + Send + 'static,
    ) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }
        let (providers, query) = self.dispatch(query, &enabled);
        if providers.is_empty() {
            return Vec::new();
        }

        let pending = providers.len();
        let (tx, rx) = mpsc::channel::<(&'static str, Vec<ResultItem>)>();
        for provider in providers {
            let tx = tx.clone();
            let query = query.clone();
            std::thread::spawn(move || {
                let id = provider.metadata().id;
                let items = provider.query(&query);
                // A closed channel means nobody is listening for this generation anymore.
                // That is the abandoned path working as designed, not a failure.
                let _ = tx.send((id, items));
            });
        }
        // The workers hold every remaining sender, so the channel closes exactly when the
        // last of them finishes — which is how the collector below knows to stop.
        drop(tx);

        let started = Instant::now();
        let mut merged = Vec::new();
        let mut arrived = 0;
        let mut all_done = false;
        while arrived < pending {
            let Some(left) = deadline.checked_sub(started.elapsed()) else {
                break;
            };
            match rx.recv_timeout(left) {
                Ok((_, items)) => {
                    merged.extend(items);
                    arrived += 1;
                }
                Err(RecvTimeoutError::Timeout) => break,
                // Every sender is gone: a provider panicked in its worker thread. The rest
                // of the fan-out still answered, and one broken source must not take the
                // result list down with it.
                Err(RecvTimeoutError::Disconnected) => {
                    all_done = true;
                    break;
                }
            }
        }

        if !all_done && arrived < pending {
            std::thread::spawn(move || {
                while let Ok((id, items)) = rx.recv() {
                    if current_generation.load(Ordering::SeqCst) != generation {
                        break;
                    }
                    if items.is_empty() {
                        continue;
                    }
                    on_late(id, Registry::rank(items));
                }
            });
        }

        Registry::rank(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, NamedAction, SearchProvider};
    use std::sync::mpsc::channel;

    /// A provider that answers after `delay`, so a test can put a source on either side of
    /// the deadline on purpose.
    struct SleepyProvider {
        id: &'static str,
        prefix: Option<&'static str>,
        score: i64,
        delay: Duration,
    }

    impl SleepyProvider {
        fn new(id: &'static str, score: i64, delay: Duration) -> Self {
            Self {
                id,
                prefix: None,
                score,
                delay,
            }
        }
    }

    impl SearchProvider for SleepyProvider {
        fn metadata(&self) -> ProviderMeta {
            ProviderMeta {
                id: self.id,
                name: self.id,
                prefix: self.prefix,
                prefix_only: false,
            }
        }

        fn query(&self, query: &Query) -> Vec<ResultItem> {
            std::thread::sleep(self.delay);
            vec![ResultItem {
                id: format!("{}:1", self.id),
                provider: self.id.to_string(),
                title: query.text.clone(),
                subtitle: Some(if query.scoped { "scoped" } else { "global" }.into()),
                icon: None,
                score: self.score,
                actions: vec![NamedAction::new("Run", Action::AppControl { command: "noop".into() })],
            }]
        }
    }

    const FAST: Duration = Duration::from_millis(0);
    const SLOW: Duration = Duration::from_millis(300);
    const DEADLINE: Duration = Duration::from_millis(80);

    fn registry(providers: Vec<SleepyProvider>) -> Registry {
        let mut registry = Registry::new();
        for provider in providers {
            registry.register(Box::new(provider));
        }
        registry
    }

    /// The point of the whole exercise: one slow source must not hold the keystroke.
    #[test]
    fn the_deadline_bounds_the_reply_and_the_slow_provider_arrives_later() {
        let registry = registry(vec![
            SleepyProvider::new("fast", 90, FAST),
            SleepyProvider::new("slow", 50, SLOW),
        ]);
        let generation = Arc::new(AtomicU64::new(1));
        let (tx, rx) = channel();

        let started = Instant::now();
        let items = registry.search_streaming(
            &Query::new("hello"),
            |_| true,
            DEADLINE,
            1,
            Arc::clone(&generation),
            move |id, items| tx.send((id.to_string(), items)).unwrap(),
        );

        assert!(
            started.elapsed() < SLOW,
            "the reply waited for the slow provider: {:?}",
            started.elapsed()
        );
        assert_eq!(items.len(), 1, "only the fast provider made the deadline");
        assert_eq!(items[0].provider, "fast");

        let (id, late) = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("the slow rows arrive late");
        assert_eq!(id, "slow");
        assert_eq!(late.len(), 1);
        assert_eq!(late[0].provider, "slow");
    }

    /// Cancellation, as the user experiences it: they typed another character, so the rows
    /// the abandoned provider is still computing answer a question that no longer exists.
    #[test]
    fn late_rows_from_a_superseded_query_are_dropped() {
        let registry = registry(vec![SleepyProvider::new("slow", 50, SLOW)]);
        let generation = Arc::new(AtomicU64::new(1));
        let (tx, rx) = channel();

        let items = registry.search_streaming(
            &Query::new("hello"),
            |_| true,
            DEADLINE,
            1,
            Arc::clone(&generation),
            move |id, items| tx.send((id.to_string(), items)).unwrap(),
        );
        assert!(items.is_empty(), "nothing beat the deadline");

        // The next keystroke starts its own generation before the straggler lands.
        generation.store(2, Ordering::SeqCst);

        assert!(
            rx.recv_timeout(SLOW * 2).is_err(),
            "rows for a query the user has typed past must never reach the overlay"
        );
    }

    /// A provider that answers from memory — every one of them, today — is not made to wait
    /// for the deadline it beat.
    #[test]
    fn a_fully_fast_fan_out_returns_at_once_and_never_calls_back() {
        let registry = registry(vec![
            SleepyProvider::new("a", 10, FAST),
            SleepyProvider::new("b", 90, FAST),
            SleepyProvider::new("c", 50, FAST),
        ]);
        let (tx, rx) = channel();

        let started = Instant::now();
        let items = registry.search_streaming(
            &Query::new("hello"),
            |_| true,
            DEADLINE,
            1,
            Arc::new(AtomicU64::new(1)),
            move |id, items| tx.send((id.to_string(), items)).unwrap(),
        );

        assert!(started.elapsed() < DEADLINE, "the fan-out waited out its own deadline");
        assert_eq!(items.len(), 3);
        assert_eq!(
            items.iter().map(|i| i.provider.as_str()).collect::<Vec<_>>(),
            ["b", "c", "a"],
            "merged best score first, exactly as the blocking path merges"
        );
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "nothing was late");
    }

    /// The dispatch rules are the registry's, not the path's: scoping, the browse-view
    /// space, prefix_only, and the settings filter must answer identically either way.
    #[test]
    fn streaming_dispatch_matches_the_blocking_path() {
        let mut registry = Registry::new();
        registry.register(Box::new(SleepyProvider {
            prefix: Some("f"),
            ..SleepyProvider::new("files", 10, FAST)
        }));
        registry.register(Box::new(SleepyProvider::new("apps", 90, FAST)));

        let both = |text: &str| {
            let blocking = registry.search_enabled(&Query::new(text), |meta| meta.id != "off");
            let streaming = registry.search_streaming(
                &Query::new(text),
                |meta| meta.id != "off",
                DEADLINE,
                1,
                Arc::new(AtomicU64::new(1)),
                |_, _| {},
            );
            let shape = |items: Vec<ResultItem>| {
                items
                    .into_iter()
                    .map(|i| (i.provider, i.title, i.subtitle.unwrap_or_default()))
                    .collect::<Vec<_>>()
            };
            let streaming = shape(streaming);
            assert_eq!(shape(blocking), streaming, "paths disagree on `{text}`");
            streaming
        };

        // A keyword scopes and is stripped; the query arrives marked scoped.
        let scoped = both("f report q3");
        assert_eq!(scoped, [("files".into(), "report q3".into(), "scoped".into())]);
        // A bare keyword is ordinary text for everyone.
        assert_eq!(both("f").len(), 2);
        // The committing space reaches the provider with an empty query: its browse view.
        assert_eq!(both("f "), [("files".into(), String::new(), "scoped".into())]);
        // Nothing typed at all.
        assert!(both("   ").is_empty());
    }

    /// A provider that panics loses its own rows and nothing else — the fan-out is not a
    /// place where one bad source can take the list down.
    #[test]
    fn a_panicking_provider_does_not_take_the_fan_out_with_it() {
        struct Panicky;
        impl SearchProvider for Panicky {
            fn metadata(&self) -> ProviderMeta {
                ProviderMeta {
                    id: "panicky",
                    name: "panicky",
                    prefix: None,
                    prefix_only: false,
                }
            }
            fn query(&self, _: &Query) -> Vec<ResultItem> {
                panic!("this provider is having a bad day");
            }
        }

        let mut registry = registry(vec![SleepyProvider::new("fast", 90, FAST)]);
        registry.register(Box::new(Panicky));

        let items = registry.search_streaming(
            &Query::new("hello"),
            |_| true,
            DEADLINE,
            1,
            Arc::new(AtomicU64::new(1)),
            |_, _| {},
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].provider, "fast");
    }
}
