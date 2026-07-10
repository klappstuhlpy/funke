//! The `SearchProvider` face of the vault: status rows while getting ready, fuzzy
//! search over the (non-secret) entry cache once unlocked.

use std::sync::Arc;

use funke_core::{glyph_data_url, Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

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
            VaultStatus::Locked => vec![unlock_row(self.vault.hello_ready())],
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

/// The locked-vault row: Enter uses Windows Hello when a session is ready, the master
/// password otherwise (then still reachable via Shift+Enter).
fn unlock_row(hello: bool) -> ResultItem {
    let mut actions = Vec::new();
    if hello {
        actions.push(NamedAction::new("Unlock with Windows Hello", Action::VaultHelloUnlock));
    }
    actions.push(NamedAction::new(
        "Unlock with master password",
        Action::PromptVaultUnlock,
    ));
    ResultItem {
        id: "vault:unlock".into(),
        provider: "vault".into(),
        title: "Unlock vault".into(),
        subtitle: Some(if hello {
            "Bitwarden — Enter uses Windows Hello, ⇧Enter the master password".into()
        } else {
            "Bitwarden — prompts for your master password".into()
        }),
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
