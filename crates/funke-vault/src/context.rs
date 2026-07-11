//! Focus context: what the user was looking at when the overlay was summoned, and which
//! vault entries that suggests.
//!
//! The launcher captures the foreground window before showing the overlay (invariant 5).
//! Beyond the HWND it can also tell us the window's title, the process behind it, and —
//! for browsers, via UI Automation — the URL in the address bar. Matching those against
//! the (non-secret) entry cache is what lets a focused Discord float the Discord
//! credential and a GitHub tab float the GitHub one.
//!
//! The matching is deliberately conservative: a wrong suggestion means offering to type
//! the wrong password into a window, so a hit needs a real signal (host equality, the
//! process *being* the site, the window title naming the entry) rather than a fuzzy
//! near-miss.

use crate::VaultEntry;

/// The strongest signal there is: the browser's address bar names the entry's site.
const SCORE_HOST_EXACT: i64 = 400;
/// Same registrable domain (`accounts.google.com` ↔ `google.com`).
const SCORE_HOST_DOMAIN: i64 = 320;
/// The focused process *is* the service (`Discord.exe` ↔ discord.com / "Discord").
const SCORE_PROCESS_EXACT: i64 = 300;
/// The process name is contained in the entry's name or site label.
const SCORE_PROCESS_PARTIAL: i64 = 220;
/// The window title names the entry ("Steam Login" ↔ "Steam"), on its own.
const SCORE_TITLE: i64 = 120;
/// …and as a confirmation on top of a host/process hit.
const SCORE_TITLE_BONUS: i64 = 40;

/// The weakest score that may surface an entry unasked. Title-only matches clear it, so
/// anything below this is noise.
pub const MIN_SUGGEST_SCORE: i64 = SCORE_TITLE;

/// Windows' own shell surfaces: focusing them means "no app", so they never carry a
/// credential context (and must not provoke an unlock prompt in the overview).
const SHELL_PROCESSES: &[&str] = &[
    "explorer",
    "searchhost",
    "searchapp",
    "shellexperiencehost",
    "startmenuexperiencehost",
    "textinputhost",
    "applicationframehost",
    "dwm",
    "lockapp",
    "funke",
];

/// What had focus before the overlay opened. Fields are optional because each one comes
/// from a call that can fail (a window with no title, a process we can't open, a browser
/// whose UI Automation tree doesn't hand over a URL).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FocusContext {
    /// The foreground window's title.
    pub title: Option<String>,
    /// The executable's stem, lowercased (`Discord.exe` → `discord`).
    pub process: Option<String>,
    /// The URL from the address bar — browsers only.
    pub url: Option<String>,
    /// The process is a known browser, so its *name* says nothing about the credential
    /// (the URL does). Set even when the URL couldn't be read.
    pub browser: bool,
}

impl FocusContext {
    /// The host from [`url`](Self::url), `www.` stripped.
    pub fn host(&self) -> Option<&str> {
        self.url.as_deref().and_then(crate::serve::host_of).map(strip_www)
    }

    /// Something a credential could plausibly belong to — a browsing session or a real
    /// app, as opposed to the desktop, the Start menu, or the launcher itself. Gates the
    /// "unlock to autofill …" row, which is all a *locked* vault can offer (the entry
    /// cache is wiped on lock, so there is nothing to match against).
    pub fn is_plausible(&self) -> bool {
        if self.title.as_deref().unwrap_or_default().trim().is_empty() {
            return false;
        }
        match self.process.as_deref() {
            Some(process) => !SHELL_PROCESSES.contains(&process),
            None => false,
        }
    }

    /// How to name this context in the UI ("github.com", "Discord").
    pub fn label(&self) -> Option<String> {
        if let Some(host) = self.host() {
            return Some(host.to_string());
        }
        self.process.as_deref().map(capitalize)
    }
}

/// How strongly `entry` belongs to the focused context, or `None` for no match at all.
pub fn score(entry: &VaultEntry, context: &FocusContext) -> Option<i64> {
    let mut best = 0;

    if let (Some(focused), Some(entry_host)) = (context.host(), entry.host.as_deref()) {
        best = best.max(host_score(strip_www(entry_host), focused));
    }
    // A browser's process name ("chrome") describes the browser, never the credential —
    // only its URL does. Matching it would offer the same entries in every tab.
    if !context.browser {
        if let Some(process) = context.process.as_deref() {
            best = best.max(process_score(entry, process));
        }
    }

    if let Some(title) = context.title.as_deref() {
        if title_names(entry, title) {
            best = if best > 0 {
                best + SCORE_TITLE_BONUS
            } else {
                SCORE_TITLE
            };
        }
    }

    (best > 0).then_some(best)
}

fn host_score(entry_host: &str, focused: &str) -> i64 {
    if entry_host.eq_ignore_ascii_case(focused) {
        return SCORE_HOST_EXACT;
    }
    if registrable(entry_host) == registrable(focused) {
        return SCORE_HOST_DOMAIN;
    }
    0
}

fn process_score(entry: &VaultEntry, process: &str) -> i64 {
    let name = normalize(&entry.name);
    let label = entry.host.as_deref().map(|host| site_label(strip_www(host)));

    if name == process || label.as_deref() == Some(process) {
        return SCORE_PROCESS_EXACT;
    }
    // Containment is only trustworthy for names long enough not to collide by accident
    // ("code" ⊂ "codewars" is a stretch; "disc" ⊂ anything is noise).
    if process.len() >= 4 {
        let partial = name.contains(process)
            || label
                .as_deref()
                .is_some_and(|label| label.len() >= 4 && (label.contains(process) || process.contains(label)));
        if partial {
            return SCORE_PROCESS_PARTIAL;
        }
    }
    0
}

/// The window title mentions the entry by name — "Steam Login" floats "Steam".
fn title_names(entry: &VaultEntry, title: &str) -> bool {
    let name = entry.name.trim().to_lowercase();
    name.chars().count() >= 3 && title.to_lowercase().contains(&name)
}

fn strip_www(host: &str) -> &str {
    host.strip_prefix("www.").unwrap_or(host)
}

/// `accounts.google.com` → `google.com`. Two labels, or three when the second-to-last is
/// a public suffix of the `co.uk` shape — enough to keep `bbc.co.uk` and `gov.co.uk`
/// apart without shipping the public-suffix list.
fn registrable(host: &str) -> String {
    const SECOND_LEVEL: &[&str] = &["co", "com", "org", "net", "ac", "gov", "edu"];
    let labels: Vec<&str> = host.split('.').filter(|label| !label.is_empty()).collect();
    let take = match labels.as_slice() {
        [.., second, _last] if labels.len() >= 3 && SECOND_LEVEL.contains(second) => 3,
        _ => 2,
    };
    labels[labels.len().saturating_sub(take)..].join(".").to_lowercase()
}

/// `discord.com` → `discord` — the bit that tends to equal the process name.
fn site_label(host: &str) -> String {
    registrable(host).split('.').next().unwrap_or_default().to_string()
}

/// Lowercase, letters and digits only: "GitHub (work)" → "githubwork" — so the process
/// name `github` still matches an entry the user decorated.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, host: Option<&str>) -> VaultEntry {
        VaultEntry {
            id: "id".into(),
            name: name.into(),
            username: Some("ben".into()),
            host: host.map(str::to_string),
            has_totp: false,
            organization: None,
            autotype: None,
        }
    }

    fn app(process: &str, title: &str) -> FocusContext {
        FocusContext {
            title: Some(title.into()),
            process: Some(process.into()),
            url: None,
            browser: false,
        }
    }

    fn browsing(url: &str) -> FocusContext {
        FocusContext {
            title: Some("Sign in".into()),
            process: Some("chrome".into()),
            url: Some(url.into()),
            browser: true,
        }
    }

    #[test]
    fn a_focused_desktop_app_matches_its_credential_by_process_name() {
        let discord = entry("Discord", Some("discord.com"));
        let github = entry("GitHub", Some("github.com"));
        let context = app("discord", "#general | Funke - Discord");

        assert_eq!(score(&discord, &context), Some(SCORE_PROCESS_EXACT + SCORE_TITLE_BONUS));
        assert_eq!(score(&github, &context), None, "unrelated entries stay hidden");
    }

    #[test]
    fn the_entry_name_alone_carries_a_process_match() {
        // No URI on the item at all — the name is all we have.
        assert_eq!(
            score(&entry("Steam", None), &app("steam", "Steam Login")),
            Some(SCORE_PROCESS_EXACT + SCORE_TITLE_BONUS)
        );
        // Decorated names still match ("Steam (family)" normalizes to "steamfamily").
        assert_eq!(
            score(&entry("Steam (family)", None), &app("steam", "Sign in")),
            Some(SCORE_PROCESS_PARTIAL)
        );
    }

    #[test]
    fn browsers_match_on_the_url_not_the_process_name() {
        let github = entry("GitHub", Some("github.com"));
        let chrome_entry = entry("Chrome Remote", Some("chrome.com"));
        let context = browsing("https://github.com/login");

        assert_eq!(score(&github, &context), Some(SCORE_HOST_EXACT));
        assert_eq!(
            score(&chrome_entry, &context),
            None,
            "the browser's own name must not drag in a 'Chrome' entry"
        );
    }

    #[test]
    fn subdomains_fold_into_the_registrable_domain() {
        let google = entry("Google", Some("google.com"));
        assert_eq!(
            score(&google, &browsing("https://accounts.google.com/signin")),
            Some(SCORE_HOST_DOMAIN)
        );
        // …but different sites under the same public suffix must not collide.
        assert_eq!(
            score(&entry("BBC", Some("bbc.co.uk")), &browsing("https://gov.co.uk")),
            None
        );
    }

    #[test]
    fn shell_surfaces_and_untitled_windows_carry_no_context() {
        assert!(!app("explorer", "Downloads").is_plausible());
        assert!(!app("discord", "  ").is_plausible());
        assert!(app("discord", "Funke - Discord").is_plausible());
        assert!(
            !FocusContext::default().is_plausible(),
            "nothing focused, nothing to offer"
        );
    }

    #[test]
    fn labels_name_the_site_in_a_browser_and_the_app_otherwise() {
        assert_eq!(
            browsing("https://www.github.com/x").label().as_deref(),
            Some("github.com")
        );
        assert_eq!(app("discord", "Discord").label().as_deref(), Some("Discord"));
    }
}
