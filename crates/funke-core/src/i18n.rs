//! The string catalogue: everything the user reads that Funke itself wrote.
//!
//! Two rules keep localization from quietly breaking the launcher, and both are load-bearing
//! enough to be tested:
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

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Locale {
    En,
    De,
}

impl Locale {
    /// The tag as settings and the frontend spell it.
    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::De => "de",
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
        match language.as_str() {
            "de" => Locale::De,
            _ => Locale::En,
        }
    }
}

/// The locale is read on every keystroke by every provider and written approximately never
/// (startup, and when the user changes it), so it is an atom rather than a lock.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_locale(locale: Locale) {
    CURRENT.store(locale as u8, Ordering::Relaxed);
}

pub fn locale() -> Locale {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Locale::De,
        _ => Locale::En,
    }
}

/// Translate. An unknown key falls back to English and then to the key itself — a missing
/// string shows up as `action.oops` in the UI, which is ugly on purpose: silence would hide it.
pub fn t(key: &str) -> &'static str {
    let catalog = match locale() {
        Locale::En => EN,
        Locale::De => DE,
    };
    lookup(catalog, key)
        .or_else(|| lookup(EN, key))
        .unwrap_or_else(|| leak_key(key))
}

/// Translate and fill `{name}` placeholders.
pub fn tf(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = t(key).to_string();
    for (name, value) in args {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

fn lookup(catalog: &[(&str, &'static str)], key: &str) -> Option<&'static str> {
    catalog
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| *value)
}

/// Only ever reached by a key that is in neither catalog — i.e. a bug, once, at development
/// time. Leaking it buys the `&'static str` that `ProviderMeta` needs.
fn leak_key(key: &str) -> &'static str {
    Box::leak(key.to_string().into_boxed_str())
}

/// English is the source of truth: every other catalog is checked against these keys.
const EN: &[(&str, &str)] = &[
    // Section headers — the provider names as the overlay groups them.
    ("provider.apps", "Applications"),
    ("provider.files", "Files"),
    ("provider.windows", "Windows"),
    ("provider.commands", "Commands"),
    ("provider.calculator", "Calculator"),
    ("provider.web", "Web"),
    ("provider.vault", "Vault"),
    ("provider.clipboard", "Clipboard"),
    ("provider.snippets", "Snippets"),
    // Actions.
    ("action.open", "Open"),
    ("action.run", "Run"),
    ("action.search", "Search"),
    ("action.reveal", "Reveal in Explorer"),
    ("action.copy_path", "Copy path"),
    ("action.copy_result", "Copy result"),
    ("action.switch_to", "Switch to"),
    ("action.end_process", "End process"),
    ("action.paste_into_last_window", "Paste into last window"),
    ("action.copy_to_clipboard", "Copy to clipboard"),
    ("action.remove_from_history", "Remove from history"),
    ("action.autotype", "Autotype into last window"),
    ("action.autotype_open", "Open website & autofill"),
    ("action.autotype_anyway", "Type it anyway"),
    ("action.copy_password", "Copy password"),
    ("action.copy_username", "Copy username"),
    ("action.copy_totp", "Copy TOTP"),
    ("action.unlock_hello", "Unlock with Windows Hello"),
    ("action.unlock_master", "Unlock with master password"),
    // Applications.
    ("apps.subtitle", "Application"),
    // Calculator.
    ("calc.subtitle", "{expression} — Enter copies the result"),
    // System commands.
    ("system.lock.title", "Lock"),
    ("system.lock.subtitle", "Lock this PC"),
    ("system.sleep.title", "Sleep"),
    ("system.sleep.subtitle", "Put the PC to sleep"),
    ("system.shutdown.title", "Shut down"),
    ("system.shutdown.subtitle", "Shut down this PC"),
    ("system.restart.title", "Restart"),
    ("system.restart.subtitle", "Restart this PC"),
    ("system.recycle.title", "Empty Recycle Bin"),
    ("system.recycle.subtitle", "Delete the recycle bin contents"),
    // Tray menu.
    ("tray.show", "Show ({hotkey})"),
    ("tray.settings", "Settings"),
    ("tray.quit", "Quit"),
    // Launcher control.
    ("control.settings.title", "Open Settings"),
    ("control.settings.subtitle", "Appearance, hotkey, commands"),
    ("control.quit.title", "Quit Funke"),
    ("control.quit.subtitle", "Exit the launcher"),
    // Web search.
    ("web.search_for", "Search the web for “{query}”"),
    // Clipboard.
    ("clipboard.clear.title", "Clear clipboard history"),
    (
        "clipboard.clear.subtitle",
        "Forgets every clip — press Enter again to confirm",
    ),
    ("clipboard.empty.title", "Clipboard history is empty"),
    (
        "clipboard.empty.subtitle",
        "Copy something — history is kept in memory only, so it starts empty each launch",
    ),
    // Vault.
    ("vault.starting", "Starting the vault backend…"),
    (
        "vault.starting.subtitle",
        "bw serve is coming up — try again in a second",
    ),
    ("vault.cli_missing", "Bitwarden CLI not found"),
    (
        "vault.cli_missing.subtitle",
        "Install bw.exe and put it on PATH — Enter opens the setup guide",
    ),
    ("vault.cli_unverified", "Bitwarden CLI not signature-verified"),
    (
        "vault.cli_unverified.subtitle",
        "The bw on this machine isn't signed by Bitwarden, and you asked Funke to refuse those — Enter opens the setup guide",
    ),
    ("vault.cli.unsigned", "the CLI carries no valid signature"),
    ("vault.cli.other_signer", "the CLI is signed, but not by Bitwarden"),
    ("vault.cli.shim", "the CLI is an unsigned script wrapper"),
    ("vault.not_logged_in", "Vault not logged in"),
    (
        "vault.not_logged_in.subtitle",
        "Run `bw login` in a terminal once — Enter opens the guide",
    ),
    ("vault.unlock", "Unlock vault"),
    ("vault.unlock_for", "Unlock vault to autofill {app}"),
    ("vault.unlock.subtitle", "Bitwarden — {how}"),
    (
        "vault.how.hello",
        "Enter uses Windows Hello, ⇧Enter the master password",
    ),
    // Reads on from "Bitwarden — ", so it is a clause, not a label.
    ("vault.how.master", "prompts for your master password"),
    // A refused autotype (see funke-shell's `form` module). The row names the credential;
    // these say why nothing was typed, and its Enter offers to type it anyway.
    ("vault.blocked", "Autotype blocked"),
    ("vault.blocked.window", "the focused window"),
    (
        "vault.blocked.no_form",
        "No login form in {app} — nothing was typed, in case that's a chat box",
    ),
    (
        "vault.blocked.no_field",
        "Couldn't put the caret in {app}'s login form — nothing was typed",
    ),
    (
        "vault.blocked.no_site",
        "No login form appeared on {app} — nothing was typed",
    ),
    ("vault.blocked.no_url", "This entry has no website to open"),
    // What the app itself answers the settings window with. The section fallback is only
    // reached by a provider the registry cannot name — a bug, but a visible one.
    ("results.fallback", "Results"),
    ("hotkey.rejected", "Couldn't bind “{hotkey}”: {error}"),
    ("update.none", "You're on the latest version."),
    ("update.installed", "Updated to {version} — restart Funke to finish."),
    (
        "update.unconfigured",
        "Auto-updates aren't set up yet (no update endpoint configured).",
    ),
];

/// German. Written, not translated: du-form, short, and using the words a German user
/// actually says — Snippet, Hotkey, App, TOTP stay as they are, because "Textbaustein" and
/// "Zusatztaste" would be correct and nobody would type them. Bitwarden's own German says
/// "Tresor", so an entry lives in one here too.
const DE: &[(&str, &str)] = &[
    ("provider.apps", "Apps"),
    ("provider.files", "Dateien"),
    ("provider.windows", "Fenster"),
    ("provider.commands", "Befehle"),
    ("provider.calculator", "Rechner"),
    ("provider.web", "Web"),
    ("provider.vault", "Tresor"),
    ("provider.clipboard", "Zwischenablage"),
    ("provider.snippets", "Snippets"),

    ("action.open", "Öffnen"),
    ("action.run", "Ausführen"),
    ("action.search", "Suchen"),
    ("action.reveal", "Im Explorer öffnen"),
    ("action.copy_path", "Pfad kopieren"),
    ("action.copy_result", "Ergebnis kopieren"),
    ("action.switch_to", "Zu Fenster wechseln"),
    ("action.end_process", "Prozess beenden"),
    ("action.paste_into_last_window", "Im vorherigen Fenster einfügen"),
    ("action.copy_to_clipboard", "In Zwischenablage kopieren"),
    ("action.remove_from_history", "Aus Verlauf entfernen"),
    ("action.autotype", "Automatisch ausfüllen"),
    ("action.autotype_open", "Website öffnen & ausfüllen"),
    ("action.autotype_anyway", "Trotzdem tippen"),
    ("action.copy_password", "Passwort kopieren"),
    ("action.copy_username", "Benutzernamen kopieren"),
    ("action.copy_totp", "TOTP-Code kopieren"),
    ("action.unlock_hello", "Mit Windows Hello entsperren"),
    ("action.unlock_master", "Mit Master-Passwort entsperren"),

    ("apps.subtitle", "App"),
    ("calc.subtitle", "{expression} — Enter kopiert das Ergebnis"),

    ("system.lock.title", "PC sperren"),
    ("system.lock.subtitle", "Windows sofort sperren"),

    ("system.sleep.title", "Energiesparmodus"),
    ("system.sleep.subtitle", "PC in den Energiesparmodus versetzen"),

    ("system.shutdown.title", "Herunterfahren"),
    ("system.shutdown.subtitle", "PC ausschalten"),

    ("system.restart.title", "Neu starten"),
    ("system.restart.subtitle", "PC neu starten"),

    ("system.recycle.title", "Papierkorb leeren"),
    ("system.recycle.subtitle", "Alle Dateien im Papierkorb löschen"),

    ("tray.show", "Öffnen ({hotkey})"),
    ("tray.settings", "Einstellungen"),
    ("tray.quit", "Beenden"),

    ("control.settings.title", "Einstellungen öffnen"),
    ("control.settings.subtitle", "Allgemein, Darstellung, Hotkey und mehr"),

    ("control.quit.title", "Funke beenden"),
    ("control.quit.subtitle", "Launcher schließen"),

    ("web.search_for", "Im Web nach „{query}“ suchen"),

    ("clipboard.clear.title", "Zwischenablage leeren"),
    (
        "clipboard.clear.subtitle",
        "Löscht den gesamten Verlauf — zum Bestätigen Enter erneut drücken",
    ),

    ("clipboard.empty.title", "Noch nichts kopiert"),
    (
        "clipboard.empty.subtitle",
        "Kopier etwas, dann findest du es hier wieder. Der Verlauf liegt nur im Arbeitsspeicher und ist nach jedem Neustart leer.",
    ),

    ("vault.starting", "Tresor wird gestartet…"),
    (
        "vault.starting.subtitle",
        "Bitwarden wird gerade gestartet. Versuch es gleich noch einmal.",
    ),

    ("vault.cli_missing", "Bitwarden CLI nicht gefunden"),
    (
        "vault.cli_missing.subtitle",
        "Installiere bw.exe und füge sie zum PATH hinzu. Enter öffnet die Anleitung.",
    ),

    ("vault.cli_unverified", "Signatur der Bitwarden-CLI nicht bestätigt"),
    (
        "vault.cli_unverified.subtitle",
        "Die bw auf diesem Rechner ist nicht von Bitwarden signiert, und du hast Funke gebeten, solche abzulehnen. Enter öffnet die Anleitung.",
    ),
    ("vault.cli.unsigned", "CLI ohne gültige Signatur"),
    ("vault.cli.other_signer", "CLI signiert, aber nicht von Bitwarden"),
    ("vault.cli.shim", "CLI ist ein unsignierter Skript-Wrapper"),
    ("vault.not_logged_in", "Nicht bei Bitwarden angemeldet"),
    (
        "vault.not_logged_in.subtitle",
        "Führe einmalig `bw login` im Terminal aus. Enter öffnet die Anleitung.",
    ),

    ("vault.unlock", "Tresor entsperren"),
    ("vault.unlock_for", "Tresor entsperren und bei {app} anmelden"),
    ("vault.unlock.subtitle", "Bitwarden — {how}"),

    (
        "vault.how.hello",
        "Enter nutzt Windows Hello, ⇧Enter das Master-Passwort",
    ),
    ("vault.how.master", "fragt nach dem Master-Passwort"),

    ("vault.blocked", "Autotype blockiert"),
    ("vault.blocked.window", "dem aktiven Fenster"),
    (
        "vault.blocked.no_form",
        "Kein Login-Formular in {app} — nichts getippt, es könnte ein Chatfenster sein",
    ),
    (
        "vault.blocked.no_field",
        "Cursor ließ sich nicht ins Login-Formular von {app} setzen — nichts getippt",
    ),
    (
        "vault.blocked.no_site",
        "Auf {app} ist kein Login-Formular erschienen — nichts getippt",
    ),
    ("vault.blocked.no_url", "Dieser Eintrag hat keine Website zum Öffnen"),

    ("results.fallback", "Treffer"),
    ("hotkey.rejected", "„{hotkey}“ ließ sich nicht belegen: {error}"),
    ("update.none", "Du hast die neueste Version."),
    ("update.installed", "Auf {version} aktualisiert — starte Funke neu, um fertig zu werden."),
    (
        "update.unconfigured",
        "Automatische Updates sind noch nicht eingerichtet (keine Update-Adresse konfiguriert).",
    ),
];

/// Score a candidate against both the localized string and its English original, keeping the
/// better of the two: a German UI must still answer to `settings`, and an English one to a
/// string a translator has since reworded. Muscle memory is not a language.
pub fn alias_score(matcher: &crate::FuzzyMatcher, key: &str) -> Option<i64> {
    let localized = matcher.score(t(key));
    let Some(english) = lookup(EN, key) else {
        return localized;
    };
    let english = matcher.score(english);
    localized.max(english)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalogues must not drift. A key present in one language and missing from the
    /// other is a string that silently shows up in the wrong language.
    #[test]
    fn every_english_key_has_a_german_one_and_nothing_is_orphaned_or_duplicated() {
        for (key, _) in EN {
            assert!(lookup(DE, key).is_some(), "German is missing `{key}`");
        }
        for (key, _) in DE {
            assert!(lookup(EN, key).is_some(), "German has `{key}`, which English does not");
        }
        for (index, (key, _)) in EN.iter().enumerate() {
            assert!(
                !EN[index + 1..].iter().any(|(other, _)| other == key),
                "`{key}` appears twice"
            );
        }
    }

    /// A translated string that drops a placeholder produces a sentence with a hole in it,
    /// and one that invents a placeholder prints a literal `{app}` at the user.
    /// Every `{placeholder}` in the English string, in any order.
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

    #[test]
    fn placeholders_survive_translation() {
        for (key, english) in EN {
            let german = lookup(DE, key).unwrap();
            assert_eq!(
                placeholders(english),
                placeholders(german),
                "`{key}`: a dropped placeholder leaves a hole in the sentence, an invented one \
                 prints `{{…}}` at the user"
            );
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
