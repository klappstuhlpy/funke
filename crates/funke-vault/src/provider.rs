//! The `SearchProvider` face of the vault: status rows while getting ready, fuzzy
//! search over the (non-secret) entry cache once unlocked, plus the context suggestions
//! the overlay's empty state shows for the app you came from ([`suggestions`]).

use std::sync::Arc;

use funke_core::{glyph_data_url, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

use crate::context::{self, FocusContext};
use crate::{Vault, VaultStatus};

/// A key.
const VAULT_GLYPH: &str = "<circle cx='8' cy='15' r='3.8'/><path d='M11 12.2L20.2 3.8'/><path d='M16.2 7.5l3.2 3.2'/>";
/// A padlock (locked / status rows).
const LOCK_GLYPH: &str = "<rect x='5' y='10.5' width='14' height='9.5' rx='2'/><path d='M8 10.5V7.5a4 4 0 0 1 8 0v3'/>";

const CLI_HELP_URL: &str = "https://bitwarden.com/help/cli/";
/// Status rows top the (single-provider, prefix-scoped) list trivially.
const STATUS_SCORE: i64 = 100;

pub struct VaultProvider {
    vault: Arc<Vault>,
}

impl VaultProvider {
    pub fn new(vault: Arc<Vault>) -> Self {
        Self { vault }
    }
}

impl SearchProvider for VaultProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "vault",
            name: "Vault",
            prefix: Some("v"),
            // Privacy: account names never surface in unscoped searches.
            prefix_only: true,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }
        self.vault.ensure_started();
        match self.vault.status() {
            VaultStatus::Idle | VaultStatus::Starting => vec![status_row(
                "vault:starting",
                "Starting the vault backend…",
                "bw serve is coming up — try again in a second",
                LOCK_GLYPH,
                Action::OpenUrl {
                    url: CLI_HELP_URL.into(),
                },
            )],
            VaultStatus::NoCli => vec![status_row(
                "vault:no-cli",
                "Bitwarden CLI not found",
                "Install bw.exe and put it on PATH — Enter opens the setup guide",
                LOCK_GLYPH,
                Action::OpenUrl {
                    url: CLI_HELP_URL.into(),
                },
            )],
            VaultStatus::Unauthenticated => vec![status_row(
                "vault:login",
                "Vault not logged in",
                "Run `bw login` in a terminal once — Enter opens the guide",
                LOCK_GLYPH,
                Action::OpenUrl {
                    url: CLI_HELP_URL.into(),
                },
            )],
            VaultStatus::Locked => vec![unlock_row(self.vault.hello_ready(), None)],
            VaultStatus::Unlocked => {
                self.vault.touch();
                let Some(matcher) = FuzzyMatcher::new(&query.text) else {
                    return Vec::new();
                };
                // Usernames only participate when the query looks like one ("ben@") —
                // otherwise "gmx" would drag in every @gmx.de account instead of the
                // GMX entries themselves.
                let by_username = query.text.contains('@');
                let mut rows = Vec::new();
                let mut wanted_hosts = Vec::new();
                for entry in self.vault.entries() {
                    let score = matcher
                        .score(&entry.name)
                        .into_iter()
                        .chain(entry.host.as_deref().and_then(|h| matcher.score(h)))
                        .chain(if by_username {
                            entry.username.as_deref().and_then(|u| matcher.score(u))
                        } else {
                            None
                        })
                        .max();
                    let Some(score) = score else { continue };
                    let icon = entry.host.as_deref().and_then(|host| self.vault.icon_for(host));
                    if icon.is_none() {
                        if let Some(host) = &entry.host {
                            wanted_hosts.push(host.clone());
                        }
                    }
                    rows.push(entry_row(entry, score, icon));
                }
                // Favicons arrive in the background; rows wear them on a later render.
                self.vault.request_icons(wanted_hosts);
                rows
            }
        }
    }
}

/// Credentials for the app the user came from, for the overlay's empty state — the
/// launcher asks for these on every summon (see [`crate::context`]).
///
/// Unlocked: the entries the focused window suggests, best match first. Locked: a single
/// "unlock to autofill …" row, because a locked vault has no cache to match against — we
/// genuinely cannot know whether a Discord credential exists until it is open. Anything
/// else (no CLI, not logged in, still starting, the setting off, a shell window in front)
/// yields nothing: the overview must not nag.
pub fn suggestions(vault: &Arc<Vault>, focus: &FocusContext, limit: usize) -> Vec<ResultItem> {
    if !vault.context_suggest_enabled() || !focus.is_plausible() {
        return Vec::new();
    }
    match vault.status() {
        VaultStatus::Unlocked => {
            let scores = vault.context_scores(focus);
            let mut scored: Vec<(i64, crate::VaultEntry)> = vault
                .entries()
                .into_iter()
                .filter_map(|entry| {
                    scores
                        .get(&entry.id)
                        .filter(|score| **score >= context::MIN_SUGGEST_SCORE)
                        .map(|score| (*score, entry))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
            scored.truncate(limit);

            let wanted_hosts = scored
                .iter()
                .filter(|(_, entry)| entry.host.as_deref().is_some_and(|host| vault.icon_for(host).is_none()))
                .filter_map(|(_, entry)| entry.host.clone())
                .collect();
            vault.request_icons(wanted_hosts);

            scored
                .into_iter()
                .map(|(score, entry)| {
                    let icon = entry.host.as_deref().and_then(|host| vault.icon_for(host));
                    entry_row(entry, score, icon)
                })
                .collect()
        }
        VaultStatus::Locked => vec![unlock_row(vault.hello_ready(), focus.label().as_deref())],
        _ => Vec::new(),
    }
}

/// The locked-vault row: Enter uses Windows Hello when a session is ready, the master
/// password otherwise (then still reachable via Shift+Enter). `context` names the app the
/// credential would be for, when the row is offered as a suggestion rather than searched.
fn unlock_row(hello: bool, context: Option<&str>) -> ResultItem {
    let mut actions = Vec::new();
    if hello {
        actions.push(NamedAction::new("Unlock with Windows Hello", Action::VaultHelloUnlock));
    }
    actions.push(NamedAction::new(
        "Unlock with master password",
        Action::PromptVaultUnlock,
    ));
    let how = if hello {
        "Enter uses Windows Hello, ⇧Enter the master password"
    } else {
        "prompts for your master password"
    };
    ResultItem {
        id: "vault:unlock".into(),
        provider: "vault".into(),
        title: match context {
            Some(context) => format!("Unlock vault to autofill {context}"),
            None => "Unlock vault".into(),
        },
        subtitle: Some(format!("Bitwarden — {how}")),
        icon: Some(glyph_data_url(LOCK_GLYPH)),
        score: STATUS_SCORE,
        actions,
    }
}

fn status_row(id: &str, title: &str, subtitle: &str, glyph: &str, action: Action) -> ResultItem {
    ResultItem {
        id: id.into(),
        provider: "vault".into(),
        title: title.into(),
        subtitle: Some(subtitle.into()),
        icon: Some(glyph_data_url(glyph)),
        score: STATUS_SCORE,
        actions: vec![NamedAction::new("Open", action)],
    }
}

fn entry_row(entry: crate::VaultEntry, score: i64, icon: Option<String>) -> ResultItem {
    let mut subtitle = match (&entry.username, &entry.host) {
        (Some(user), Some(host)) => Some(format!("{user} — {host}")),
        (Some(user), None) => Some(user.clone()),
        (None, Some(host)) => Some(host.clone()),
        (None, None) => None,
    };
    if let Some(org) = &entry.organization {
        subtitle = Some(match subtitle {
            Some(text) => format!("{text} · {org}"),
            None => org.clone(),
        });
    }
    let mut actions = vec![
        NamedAction::new(
            "Autotype into last window",
            Action::VaultAutotype { id: entry.id.clone() },
        ),
        NamedAction::new(
            "Copy password",
            Action::VaultCopy {
                id: entry.id.clone(),
                field: "password".into(),
            },
        ),
        NamedAction::new(
            "Copy username",
            Action::VaultCopy {
                id: entry.id.clone(),
                field: "username".into(),
            },
        ),
    ];
    if entry.has_totp {
        actions.push(NamedAction::new(
            "Copy TOTP",
            Action::VaultCopy {
                id: entry.id.clone(),
                field: "totp".into(),
            },
        ));
    }
    ResultItem {
        id: format!("vault:{}", entry.id),
        provider: "vault".into(),
        title: entry.name,
        subtitle,
        icon: icon.or_else(|| Some(glyph_data_url(VAULT_GLYPH))),
        score,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VaultEntry;

    fn entry(id: &str, name: &str, username: Option<&str>, host: Option<&str>) -> VaultEntry {
        VaultEntry {
            id: id.into(),
            name: name.into(),
            username: username.map(str::to_string),
            host: host.map(str::to_string),
            has_totp: false,
            organization: None,
            autotype: None,
        }
    }

    /// A vault whose icon fetches stay off, so provider tests never touch the network.
    fn offline_vault() -> Arc<Vault> {
        let settings = funke_core::Settings {
            vault_icons: false,
            ..Default::default()
        };
        Arc::new(Vault::new(Arc::new(std::sync::RwLock::new(settings))))
    }

    #[test]
    fn entry_rows_reference_secrets_by_id_only() {
        let mut entry = entry("uuid-1", "GitHub", Some("ben"), Some("github.com"));
        entry.has_totp = true;
        entry.organization = Some("Acme".into());
        let row = entry_row(entry, 10, None);
        assert_eq!(row.subtitle.as_deref(), Some("ben — github.com · Acme"));
        assert_eq!(row.actions.len(), 4, "TOTP items grow a Copy TOTP action");
        assert_eq!(row.actions[3].label, "Copy TOTP");
        let serialized = serde_json::to_string(&row).unwrap();
        assert!(!serialized.contains("password\":\""), "no secret material in the item");
        assert!(serialized.contains("uuid-1"));
    }

    #[test]
    fn items_without_totp_or_organization_stay_plain() {
        let row = entry_row(entry("uuid-2", "Router", Some("admin"), None), 10, None);
        assert_eq!(row.subtitle.as_deref(), Some("admin"));
        assert_eq!(row.actions.len(), 3);
    }

    #[test]
    fn locked_vault_yields_only_the_unlock_row() {
        let vault = offline_vault();
        // Force the state machine past startup without a bw CLI.
        vault.force_status(VaultStatus::Locked);
        let provider = VaultProvider::new(Arc::clone(&vault));
        let rows = provider.query(&Query::new("github"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "vault:unlock");
        // vault_hello defaults to off, so the password prompt is the (only) action.
        assert!(matches!(rows[0].primary_action(), Some(Action::PromptVaultUnlock)));
    }

    /// The overview asks on every summon: focused Discord → the Discord credential,
    /// ready to autotype straight back into it.
    #[test]
    fn the_focused_app_suggests_its_credential() {
        let vault = offline_vault();
        vault.force_status(VaultStatus::Unlocked);
        vault.force_entries(vec![
            entry("uuid-1", "Discord", Some("ben@example.com"), Some("discord.com")),
            entry("uuid-2", "GitHub", Some("ben"), Some("github.com")),
        ]);
        let focus = FocusContext {
            title: Some("#general | Funke - Discord".into()),
            process: Some("discord".into()),
            url: None,
            browser: false,
        };

        let rows = suggestions(&vault, &focus, 3);
        assert_eq!(rows.len(), 1, "only the entry the window is about");
        assert_eq!(rows[0].title, "Discord");
        assert!(matches!(
            rows[0].primary_action(),
            Some(Action::VaultAutotype { id }) if id == "uuid-1"
        ));
    }

    #[test]
    fn a_locked_vault_offers_to_unlock_for_the_app_in_front() {
        let vault = offline_vault();
        vault.force_status(VaultStatus::Locked);
        let focus = FocusContext {
            title: Some("Discord".into()),
            process: Some("discord".into()),
            url: None,
            browser: false,
        };

        // Locked means no entry cache, so we can't know *whether* a Discord credential
        // exists — offering the unlock is the most the overview can honestly do.
        let rows = suggestions(&vault, &focus, 3);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Unlock vault to autofill Discord");
        assert!(matches!(rows[0].primary_action(), Some(Action::PromptVaultUnlock)));

        // The desktop or the Start menu in front is not a credential context — no nag.
        let shell = FocusContext {
            title: Some("Program Manager".into()),
            process: Some("explorer".into()),
            ..Default::default()
        };
        assert!(suggestions(&vault, &shell, 3).is_empty());
    }

    #[test]
    fn suggestions_stay_silent_while_the_backend_is_unusable_or_switched_off() {
        let vault = offline_vault();
        let focus = FocusContext {
            title: Some("Discord".into()),
            process: Some("discord".into()),
            ..Default::default()
        };
        // No CLI / still starting / not logged in: the overview shows nothing at all.
        for status in [VaultStatus::Idle, VaultStatus::Starting, VaultStatus::NoCli] {
            vault.force_status(status);
            assert!(suggestions(&vault, &focus, 3).is_empty(), "{status:?} must not nag");
        }

        // …and the setting switches the whole idea off.
        let settings = funke_core::Settings {
            vault_icons: false,
            vault_context_suggest: false,
            ..Default::default()
        };
        let vault = Arc::new(Vault::new(Arc::new(std::sync::RwLock::new(settings))));
        vault.force_status(VaultStatus::Locked);
        assert!(suggestions(&vault, &focus, 3).is_empty());
    }

    #[test]
    fn usernames_only_match_queries_containing_an_at_sign() {
        let vault = offline_vault();
        vault.force_status(VaultStatus::Unlocked);
        vault.force_entries(vec![
            entry("uuid-1", "GMX", Some("ben@gmx.de"), Some("gmx.net")),
            entry("uuid-2", "Forum", Some("someone@gmx.de"), Some("example.org")),
        ]);
        let provider = VaultProvider::new(Arc::clone(&vault));

        // "gmx" means the site, not every account with a @gmx.de address.
        let rows = provider.query(&Query::new("gmx"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "GMX");

        // With an @ the user means the username — both entries match again.
        let rows = provider.query(&Query::new("@gmx"));
        assert_eq!(rows.len(), 2);
    }
}
