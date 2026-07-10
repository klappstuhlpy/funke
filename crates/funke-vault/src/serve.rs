//! `bw serve` lifecycle and its localhost REST surface.
//!
//! The server is spawned windowless on a random free port and answers, once up:
//! `GET /status`, `POST /unlock {password}`, `POST /lock`, `POST /sync`,
//! `GET /list/object/items`, `GET /object/item/{id}`. Responses arrive as
//! `{ success, data, message }` envelopes; failures are 4xx with the same envelope.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

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

/// Find the CLI, spawn `bw serve`, and wait until `/status` answers.
/// Returns the child, the port, and the reported vault status string.
pub fn start() -> Result<(Child, u16, String), StartError> {
    if !cli_available() {
        return Err(StartError::NoCli);
    }
    let port = free_port().map_err(|e| StartError::Failed(format!("no free port: {e}")))?;

    let mut command = Command::new("bw");
    command
        .args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
}

pub fn status(port: u16) -> Result<String, String> {
    let envelope: Envelope<StatusData> = ureq::get(&url(port, "/status"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    envelope
        .data
        .map(|data| data.template.status)
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
    login: Option<Login>,
}

#[derive(Deserialize)]
struct Login {
    username: Option<String>,
    password: Option<String>,
    uris: Option<Vec<Uri>>,
}

#[derive(Deserialize)]
struct Uri {
    uri: Option<String>,
}

/// The searchable cache: logins only, secrets dropped on the floor here.
pub fn list_entries(port: u16) -> Result<Vec<VaultEntry>, String> {
    let envelope: Envelope<ListData> = ureq::get(&url(port, "/list/object/items"))
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let items = envelope.data.ok_or("item list had no data")?.data;
    Ok(items
        .into_iter()
        .filter(|item| item.kind == TYPE_LOGIN)
        .map(to_entry)
        .collect())
}

fn to_entry(item: Item) -> VaultEntry {
    let login = item.login.as_ref();
    VaultEntry {
        id: item.id,
        name: item.name,
        username: login.and_then(|l| l.username.clone()).filter(|u| !u.is_empty()),
        host: login
            .and_then(|l| l.uris.as_ref())
            .and_then(|uris| uris.iter().find_map(|u| u.uri.as_deref().and_then(host_of)))
            .map(str::to_string),
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
            {"object":"item","id":"aaa","type":1,"name":"GitHub",
             "login":{"username":"ben","password":"hunter2","uris":[{"uri":"https://github.com"}]}},
            {"object":"item","id":"bbb","type":2,"name":"Secure note"}
        ]}}"#;
        let envelope: Envelope<ListData> = serde_json::from_str(json).unwrap();
        let entries: Vec<VaultEntry> = envelope
            .data
            .unwrap()
            .data
            .into_iter()
            .filter(|item| item.kind == TYPE_LOGIN)
            .map(to_entry)
            .collect();
        assert_eq!(entries.len(), 1, "non-login items are dropped");
        assert_eq!(entries[0].name, "GitHub");
        assert_eq!(entries[0].username.as_deref(), Some("ben"));
        assert_eq!(entries[0].host.as_deref(), Some("github.com"));
    }

    #[test]
    fn failed_unlock_envelopes_carry_the_message() {
        let json = r#"{"success":false,"message":"Invalid master password."}"#;
        let envelope: Envelope<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(!envelope.success);
        assert_eq!(envelope.message.as_deref(), Some("Invalid master password."));
    }
}
