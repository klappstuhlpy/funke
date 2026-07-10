//! `bw serve` lifecycle and its localhost REST surface.
//!
//! The server is spawned windowless on a random free port (pre-unlocked when a
//! `BW_SESSION` is supplied) and answers, once up: `GET /status`,
//! `POST /unlock {password}`, `POST /lock`, `POST /sync`, `GET /list/object/items`,
//! `GET /list/object/organizations`, `GET /object/item/{id}`, `GET /object/totp/{id}`.
//! Responses arrive as `{ success, data, message }` envelopes; failures are 4xx with
//! the same envelope. Favicons come from the server's icon service, not `bw serve`.

use std::collections::HashMap;
use std::io::Read;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;

use crate::VaultEntry;

const START_TIMEOUT: Duration = Duration::from_secs(15);
/// Unlock runs the KDF (Argon2/PBKDF2) — give it room.
const UNLOCK_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Bitwarden item type 1 = login (the only kind we surface).
const TYPE_LOGIN: u8 = 1;

pub enum StartError {
    NoCli,
    Failed(String),
}

/// What `/status` reports: the vault state plus the configured server (which decides
/// where favicons come from).
pub struct StatusInfo {
    pub status: String,
    pub server_url: Option<String>,
}

/// Find the CLI, spawn `bw serve`, and wait until `/status` answers.
/// Returns the child, the port, and the reported status.
pub fn start() -> Result<(Child, u16, StatusInfo), StartError> {
    start_with(None)
}

/// [`start`], but with `BW_SESSION` in the child's environment — the server comes up
/// already unlocked (the Windows Hello path).
pub fn start_with_session(session: &str) -> Result<(Child, u16, StatusInfo), StartError> {
    start_with(Some(session))
}

fn start_with(session: Option<&str>) -> Result<(Child, u16, StatusInfo), StartError> {
    if !cli_available() {
        return Err(StartError::NoCli);
    }
    let port = free_port().map_err(|e| StartError::Failed(format!("no free port: {e}")))?;

    let mut command = Command::new("bw");
    command
        .args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(session) = session {
        command.env("BW_SESSION", session);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| StartError::Failed(e.to_string()))?;

    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Ok(status) = status(port) {
            return Ok((child, port, status));
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            return Err(StartError::Failed("bw serve did not come up in time".into()));
        }
        // The child dying early (port taken, corrupt config) must not hang the loop.
        if let Ok(Some(code)) = child.try_wait() {
            return Err(StartError::Failed(format!("bw serve exited early ({code})")));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Mint a session key (for Windows Hello re-unlocks): `bw unlock --raw` prints it to
/// stdout. The password travels via an env var, never the command line. Runs the KDF
/// a second time, so this takes as long as the unlock itself.
pub fn unlock_raw(password: &str) -> Result<String, String> {
    let mut command = Command::new("bw");
    command
        .args(["unlock", "--raw", "--passwordenv", "FUNKE_BW_PASSWORD"])
        .env("FUNKE_BW_PASSWORD", password)
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().map_err(|e| format!("bw unlock failed to run: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "bw unlock failed".into()
        } else {
            stderr
        });
    }
    let session = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if session.is_empty() {
        Err("bw unlock returned no session key".into())
    } else {
        Ok(session)
    }
}

/// `bw --version` succeeding is the cheapest "CLI exists and runs" probe.
fn cli_available() -> bool {
    let mut command = Command::new("bw");
    command.arg("--version").stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.status().map(|status| status.success()).unwrap_or(false)
}

/// Bind port 0, read what the OS handed out, release it. The tiny window before
/// `bw serve` rebinds it is an accepted race.
fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))?.local_addr()?.port())
}

fn url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}{path}")
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    #[serde(default)]
    message: Option<String>,
    data: Option<T>,
}

#[derive(Deserialize)]
struct StatusData {
    template: StatusTemplate,
}

#[derive(Deserialize)]
struct StatusTemplate {
    status: String,
    #[serde(rename = "serverUrl")]
    server_url: Option<String>,
}

pub fn status(port: u16) -> Result<StatusInfo, String> {
    let envelope: Envelope<StatusData> = ureq::get(&url(port, "/status"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    envelope
        .data
        .map(|data| StatusInfo {
            status: data.template.status,
            server_url: data.template.server_url,
        })
        .ok_or_else(|| "status response had no data".into())
}

pub fn unlock(port: u16, password: &str) -> Result<(), String> {
    let response = ureq::post(&url(port, "/unlock"))
        .timeout(UNLOCK_TIMEOUT)
        .send_json(serde_json::json!({ "password": password }));
    let envelope: Envelope<serde_json::Value> = match response {
        Ok(resp) => resp.into_json().map_err(|e| e.to_string())?,
        // bw answers failed unlocks with a 4xx that still carries the envelope.
        Err(ureq::Error::Status(_, resp)) => resp.into_json().map_err(|e| e.to_string())?,
        Err(e) => return Err(e.to_string()),
    };
    if envelope.success {
        Ok(())
    } else {
        Err(envelope.message.unwrap_or_else(|| "Invalid master password".into()))
    }
}

pub fn lock(port: u16) -> Result<(), String> {
    ureq::post(&url(port, "/lock"))
        .timeout(REQUEST_TIMEOUT)
        .send_json(serde_json::json!({}))
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn sync(port: u16) -> Result<(), String> {
    ureq::post(&url(port, "/sync"))
        .timeout(UNLOCK_TIMEOUT)
        .send_json(serde_json::json!({}))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Deserialize)]
struct ListData {
    data: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    id: String,
    name: String,
    #[serde(rename = "type")]
    kind: u8,
    #[serde(rename = "organizationId")]
    organization_id: Option<String>,
    login: Option<Login>,
}

#[derive(Deserialize)]
struct Login {
    username: Option<String>,
    password: Option<String>,
    totp: Option<String>,
    uris: Option<Vec<Uri>>,
}

#[derive(Deserialize)]
struct Uri {
    uri: Option<String>,
}

#[derive(Deserialize)]
struct Organization {
    id: String,
    name: String,
}

/// Organization id → display name, for labeling which vault an entry lives in.
/// Any failure degrades to "no labels", never to a failed entry list.
fn organizations(port: u16) -> HashMap<String, String> {
    #[derive(Deserialize)]
    struct OrgList {
        data: Vec<Organization>,
    }
    let envelope: Option<Envelope<OrgList>> = ureq::get(&url(port, "/list/object/organizations"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .ok()
        .and_then(|resp| resp.into_json().ok());
    envelope
        .and_then(|env| env.data)
        .map(|list| list.data.into_iter().map(|org| (org.id, org.name)).collect())
        .unwrap_or_default()
}

/// The searchable cache: logins only, secrets dropped on the floor here (the TOTP
/// seed collapses to a has-it flag).
pub fn list_entries(port: u16) -> Result<Vec<VaultEntry>, String> {
    let envelope: Envelope<ListData> = ureq::get(&url(port, "/list/object/items"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let items = envelope.data.ok_or("item list had no data")?.data;
    let orgs = organizations(port);
    Ok(items
        .into_iter()
        .filter(|item| item.kind == TYPE_LOGIN)
        .map(|item| to_entry(item, &orgs))
        .collect())
}

fn to_entry(item: Item, orgs: &HashMap<String, String>) -> VaultEntry {
    let login = item.login.as_ref();
    VaultEntry {
        id: item.id,
        name: item.name,
        username: login.and_then(|l| l.username.clone()).filter(|u| !u.is_empty()),
        host: login
            .and_then(|l| l.uris.as_ref())
            .and_then(|uris| uris.iter().find_map(|u| u.uri.as_deref().and_then(host_of)))
            .map(str::to_string),
        has_totp: login.and_then(|l| l.totp.as_deref()).is_some_and(|t| !t.is_empty()),
        organization: item.organization_id.and_then(|id| orgs.get(&id).cloned()),
    }
}

/// `https://github.com/login` → `github.com`, tolerant of scheme-less URIs.
fn host_of(uri: &str) -> Option<&str> {
    let rest = uri.split_once("://").map_or(uri, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?; // strip userinfo
    let host = host.split(':').next()?; // strip port
    (!host.is_empty()).then_some(host)
}

/// Current TOTP code for an item, computed by the CLI from the (never cached) seed.
pub fn item_totp(port: u16, id: &str) -> Result<String, String> {
    let response = ureq::get(&url(port, &format!("/object/totp/{id}")))
        .timeout(REQUEST_TIMEOUT)
        .call();
    let envelope: Envelope<serde_json::Value> = match response {
        Ok(resp) => resp.into_json().map_err(|e| e.to_string())?,
        // Items without a TOTP answer 4xx with the envelope carrying the reason.
        Err(ureq::Error::Status(_, resp)) => resp.into_json().map_err(|e| e.to_string())?,
        Err(e) => return Err(e.to_string()),
    };
    if !envelope.success {
        return Err(envelope.message.unwrap_or_else(|| "this item has no TOTP".into()));
    }
    // The code arrives as {"object":"string","data":"123456"}; tolerate a bare string.
    match envelope.data {
        Some(serde_json::Value::String(code)) => Ok(code),
        Some(serde_json::Value::Object(map)) => map
            .get("data")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| "totp response had no code".into()),
        _ => Err("totp response had no code".into()),
    }
}

/// One favicon from the server's icon service as a data URL, or `None` (no icon, too
/// big, network trouble — all equally "show the glyph instead").
pub fn fetch_icon(base: &str, host: &str) -> Option<String> {
    const ICON_MAX_BYTES: u64 = 256 * 1024;
    let response = ureq::get(&format!("{base}/{host}/icon.png"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(ICON_MAX_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty()).then(|| {
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        )
    })
}

pub fn item_credentials(port: u16, id: &str) -> Result<crate::Credentials, String> {
    let envelope: Envelope<Item> = ureq::get(&url(port, &format!("/object/item/{id}")))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let item = envelope.data.ok_or("item response had no data")?;
    let login = item.login.ok_or("item has no login fields")?;
    Ok(crate::Credentials {
        username: login.username.filter(|u| !u.is_empty()),
        password: login.password.filter(|p| !p.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_are_extracted_from_messy_uris() {
        assert_eq!(host_of("https://github.com/login"), Some("github.com"));
        assert_eq!(
            host_of("https://user@vault.example.org:8443/x?y#z"),
            Some("vault.example.org")
        );
        assert_eq!(host_of("example.com"), Some("example.com"));
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn list_response_parses_and_drops_secrets_and_non_logins() {
        let json = r#"{"success":true,"data":{"object":"list","data":[
            {"object":"item","id":"aaa","type":1,"name":"GitHub","organizationId":"org-1",
             "login":{"username":"ben","password":"hunter2","totp":"JBSWY3DP","uris":[{"uri":"https://github.com"}]}},
            {"object":"item","id":"ccc","type":1,"name":"Router",
             "login":{"username":"admin","password":"hunter2"}},
            {"object":"item","id":"bbb","type":2,"name":"Secure note"}
        ]}}"#;
        let envelope: Envelope<ListData> = serde_json::from_str(json).unwrap();
        let orgs = HashMap::from([("org-1".to_string(), "Acme".to_string())]);
        let entries: Vec<VaultEntry> = envelope
            .data
            .unwrap()
            .data
            .into_iter()
            .filter(|item| item.kind == TYPE_LOGIN)
            .map(|item| to_entry(item, &orgs))
            .collect();
        assert_eq!(entries.len(), 2, "non-login items are dropped");
        assert_eq!(entries[0].name, "GitHub");
        assert_eq!(entries[0].username.as_deref(), Some("ben"));
        assert_eq!(entries[0].host.as_deref(), Some("github.com"));
        assert!(entries[0].has_totp, "totp seed collapses to a flag");
        assert_eq!(entries[0].organization.as_deref(), Some("Acme"));
        assert!(!entries[1].has_totp);
        assert_eq!(entries[1].organization, None, "personal items carry no vault label");
    }

    #[test]
    fn totp_and_status_responses_parse() {
        let totp: Envelope<serde_json::Value> =
            serde_json::from_str(r#"{"success":true,"data":{"object":"string","data":"123456"}}"#).unwrap();
        match totp.data {
            Some(serde_json::Value::Object(map)) => assert_eq!(map.get("data").unwrap(), "123456"),
            other => panic!("unexpected totp payload: {other:?}"),
        }

        let status: Envelope<StatusData> = serde_json::from_str(
            r#"{"success":true,"data":{"object":"template","template":{"serverUrl":"https://vault.example.org","status":"locked"}}}"#,
        )
        .unwrap();
        let template = status.data.unwrap().template;
        assert_eq!(template.status, "locked");
        assert_eq!(template.server_url.as_deref(), Some("https://vault.example.org"));
    }

    #[test]
    fn failed_unlock_envelopes_carry_the_message() {
        let json = r#"{"success":false,"message":"Invalid master password."}"#;
        let envelope: Envelope<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(!envelope.success);
        assert_eq!(envelope.message.as_deref(), Some("Invalid master password."));
    }
}
