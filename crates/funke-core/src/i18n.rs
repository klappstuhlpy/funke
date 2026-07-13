//! The string catalogue: everything the user reads that Funke itself wrote.
//!
//! The strings themselves live in **`crates/funke-core/locales/<tag>.json`**, one flat file per
//! language, and are compiled in with `include_str!` — nothing is read from disk at runtime, so
//! a shipped Funke cannot be broken by a missing or edited locale file. Adding a language is
//! four small things, and `docs/TRANSLATING.md` walks through them.
//!
//! This module owns the *rules*, and two of them are load-bearing enough to be tested:
//!
//! 1. **A `ResultItem`'s id is never localized.** Ids key the frecency store and the recents
//!    file, which outlive a language change — build one out of a title and switching to
//!    German silently orphans everything the user has ever launched. Ids come from a stable
//!    key (`system:lock`), titles come from here.
//! 2. **The English title keeps matching.** A German UI still answers to `settings`, because
//!    the fuzzy matcher scores the localized title *and* the English one and keeps the better
//!    (see `alias_score` at the call sites). Muscle memory is not a language.
//!
//! Formatting is `{name}` placeholders and [`tf`] — no `format!` on translated strings, or
//! the argument order becomes part of the translation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    De,
}

impl Locale {
    /// Every language Funke speaks, in the order their catalogues are indexed. English first:
    /// it is the source of truth every other file is checked against.
    const ALL: &'static [Locale] = &[Locale::En, Locale::De];

    /// The tag as settings, the frontend, and the locale file's name all spell it.
    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::De => "de",
        }
    }

    /// This language's catalogue, embedded at compile time.
    fn source(self) -> &'static str {
        match self {
            Locale::En => include_str!("../locales/en.json"),
            Locale::De => include_str!("../locales/de.json"),
        }
    }

    /// Parse a settings value or a Windows UI language (`de`, `de-DE`, `de_AT`) — anything
    /// unknown is English, which is also what `auto` resolves to when the app can't tell.
    pub fn parse(tag: &str) -> Locale {
        let language = tag
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        Locale::ALL
            .iter()
            .copied()
            .find(|locale| locale.tag() == language)
            .unwrap_or(Locale::En)
    }
}

/// The locale is read on every keystroke by every provider and written approximately never
/// (startup, and when the user changes it), so it is an atom rather than a lock.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    CURRENT.store(locale as u8, Ordering::Relaxed);
}

pub fn locale() -> Locale {
    let current = CURRENT.load(Ordering::Relaxed) as usize;
    Locale::ALL.get(current).copied().unwrap_or(Locale::En)
}

/// Translate. An unknown key falls back to English and then to the key itself — a missing
/// string shows up as `action.oops` in the UI, which is ugly on purpose: silence would hide it.
pub fn t(key: &str) -> &'static str {
    lookup(locale(), key)
        .or_else(|| lookup(Locale::En, key))
        .unwrap_or_else(|| leak(key.to_string()))
}

/// Translate and fill `{name}` placeholders.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = t(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn lookup(locale: Locale, key: &str) -> Option<&'static str> {
    catalogs()[locale as usize].get(key).copied()
}

/// The parsed catalogues, one per [`Locale::ALL`], built once on first use.
///
/// Their strings are leaked on purpose. `ProviderMeta` needs `&'static str`, the catalogue is
/// alive for as long as the process is, and the alternative — handing out borrows of a lazily
/// initialized map — buys nothing for a few kilobytes that are never freed anyway.
fn catalogs() -> &'static [HashMap<&'static str, &'static str>] {
    static CATALOGS: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();
    CATALOGS.get_or_init(|| Locale::ALL.iter().map(|locale| parse(locale.source())).collect())
}

/// A locale file is compiled into the binary, so it cannot be malformed in the field without
/// having been malformed in CI first — where the tests below parse both of them. Panicking is
/// the honest response to a file that shipped broken; degrading to an empty catalogue would
/// only mean every string in the app silently becomes its own key.
fn parse(source: &'static str) -> HashMap<&'static str, &'static str> {
    let entries: HashMap<String, String> = serde_json::from_str(source).expect("locale file is not valid JSON");
    entries
        .into_iter()
        .map(|(key, value)| (leak(key), leak(value)))
        .collect()
}

fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// Score a candidate against both the localized string and its English original, keeping the
/// better of the two: a German UI must still answer to `settings`, and an English one to a
/// string a translator has since reworded. Muscle memory is not a language.
pub fn alias_score(matcher: &crate::FuzzyMatcher, key: &str) -> Option<i64> {
    let localized = matcher.score(t(key));
    let Some(english) = lookup(Locale::En, key) else {
        return localized;
    };
    let english = matcher.score(english);
    localized.max(english)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file's keys, in the order they are written — so a duplicate key, which `serde_json`
    /// would silently collapse into one, is still visible to the test below.
    fn keys_in_file(source: &str) -> Vec<String> {
        source
            .lines()
            .filter_map(|line| {
                let rest = line.trim_start().strip_prefix('"')?;
                let (key, after) = rest.split_once('"')?;
                after.trim_start().starts_with(':').then(|| key.to_string())
            })
            .collect()
    }

    /// The catalogues must not drift. A key present in one language and missing from the
    /// other is a string that silently shows up in the wrong language.
    #[test]
    fn every_language_has_exactly_englishs_keys_and_no_duplicates() {
        let english = &catalogs()[Locale::En as usize];
        for locale in Locale::ALL {
            let catalog = &catalogs()[*locale as usize];
            let tag = locale.tag();
            for key in english.keys() {
                assert!(catalog.contains_key(key), "{tag} is missing `{key}`");
            }
            for key in catalog.keys() {
                assert!(english.contains_key(key), "{tag} has `{key}`, which English does not");
            }

            // `serde_json` keeps the last of two identical keys without a word, so the raw file
            // is what has to be checked — a duplicate is a translation nobody can find.
            let written = keys_in_file(locale.source());
            assert_eq!(
                written.len(),
                catalog.len(),
                "locales/{tag}.json has a duplicate key — the file has {} keys, the catalogue {}",
                written.len(),
                catalog.len()
            );
        }
    }

    /// Every `{placeholder}` in a string, in any order.
    fn placeholders(text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = text
            .match_indices('{')
            .filter_map(|(start, _)| {
                let end = text[start..].find('}')? + start;
                Some(&text[start..=end])
            })
            .collect();
        found.sort_unstable();
        found
    }

    /// A translated string that drops a placeholder produces a sentence with a hole in it,
    /// and one that invents a placeholder prints a literal `{app}` at the user.
    #[test]
    fn placeholders_survive_translation() {
        let english = &catalogs()[Locale::En as usize];
        for locale in Locale::ALL {
            for (key, translated) in &catalogs()[*locale as usize] {
                assert_eq!(
                    placeholders(english[key]),
                    placeholders(translated),
                    "{}: `{key}`: a dropped placeholder leaves a hole in the sentence, an invented \
                     one prints `{{…}}` at the user",
                    locale.tag()
                );
            }
        }
    }

    /// `set_locale` stores the discriminant and `catalogs()` indexes by it, so the two orders
    /// have to be the same one. A new language appended to `ALL` but declared in the middle of
    /// the enum would hand every string to the wrong file.
    #[test]
    fn the_enum_and_the_catalogue_list_agree_on_the_order() {
        for (index, locale) in Locale::ALL.iter().enumerate() {
            assert_eq!(*locale as usize, index, "{} is out of order", locale.tag());
        }
    }

    #[test]
    fn a_windows_language_tag_becomes_a_locale() {
        assert_eq!(Locale::parse("de"), Locale::De);
        assert_eq!(Locale::parse("de-DE"), Locale::De);
        assert_eq!(Locale::parse("de_AT"), Locale::De);
        assert_eq!(Locale::parse("DE-de"), Locale::De);
        assert_eq!(Locale::parse("en-US"), Locale::En);
        assert_eq!(Locale::parse("fr-FR"), Locale::En, "unknown languages read English");
        assert_eq!(Locale::parse(""), Locale::En);
    }

    #[test]
    fn translating_switches_with_the_locale_and_fills_placeholders() {
        set_locale(Locale::De);
        assert_eq!(t("action.open"), "Öffnen");
        assert_eq!(
            tf("vault.unlock_for", &[("app", "Discord")]),
            "Tresor entsperren und bei Discord anmelden"
        );

        set_locale(Locale::En);
        assert_eq!(t("action.open"), "Open");
        assert_eq!(tf("web.search_for", &[("query", "rust")]), "Search the web for “rust”");

        // A key nobody translated is shown, not swallowed.
        assert_eq!(t("action.does_not_exist"), "action.does_not_exist");
    }
}
