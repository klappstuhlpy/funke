//! Fuzzy scoring on top of nucleo (the matcher behind Helix): parse the pattern once per
//! keystroke, score once per candidate.

use std::cell::RefCell;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

thread_local! {
    // Matcher is a large-ish reusable scratch buffer that needs &mut; one per thread
    // keeps `FuzzyMatcher::score` shareable across providers without locking.
    static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new(Config::DEFAULT));
}

/// A parsed query pattern, reusable across many haystacks.
pub struct FuzzyMatcher {
    pattern: Pattern,
}

impl FuzzyMatcher {
    /// Returns `None` for empty/whitespace-only needles so providers can bail early.
    pub fn new(needle: &str) -> Option<Self> {
        let needle = needle.trim();
        if needle.is_empty() {
            return None;
        }
        Some(Self {
            pattern: Pattern::parse(needle, CaseMatching::Ignore, Normalization::Smart),
        })
    }

    /// Higher is better; `None` means no match.
    pub fn score(&self, haystack: &str) -> Option<i64> {
        let mut buf = Vec::new();
        MATCHER
            .with(|matcher| {
                self.pattern
                    .score(Utf32Str::new(haystack, &mut buf), &mut matcher.borrow_mut())
            })
            .map(i64::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needles_produce_no_matcher() {
        assert!(FuzzyMatcher::new("").is_none());
        assert!(FuzzyMatcher::new("   ").is_none());
    }

    #[test]
    fn matches_are_scored_and_misses_are_none() {
        let matcher = FuzzyMatcher::new("quit").unwrap();
        assert!(matcher.score("Quit Funke").is_some());
        assert!(matcher.score("Settings").is_none());
    }

    #[test]
    fn contiguous_prefix_beats_scattered_match() {
        let matcher = FuzzyMatcher::new("fire").unwrap();
        let prefix = matcher.score("Firefox").unwrap();
        let scattered = matcher.score("Find Replace").unwrap();
        assert!(prefix > scattered, "prefix {prefix} should beat scattered {scattered}");
    }
}
