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

use crate::cli;
use crate::job::KillOnDropJob;
use crate::VaultEntry;

const START_TIMEOUT: Duration = Duration::from_secs(15);
/// Unlock runs the KDF (Argon2/PBKDF2) — give it room.
const UNLOCK_TIMEOUT: Duration = Duration::from_secs(60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Bitwarden item type 1 = login (the only kind we surface).
const TYPE_LOGIN: u8 = 1;

pub enum StartError {
    NoCli,
    /// A CLI is installed, but it isn't the one Bitwarden signed, and the user has asked for
    /// that to be a refusal rather than a warning (`Settings::vault_require_signed_cli`).
    Unverified,
    Failed(String),
}

/// Every `bw` invocation in this crate, built from the pinned absolute path — never from the
/// bare name, which would re-walk `PATH` on each spawn (see [`crate::cli`]).
fn bw() -> Option<Command> {
    let mut command = Command::new(&cli::cli()?.path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Some(command)
}

/// The running `bw serve` child plus the job object that bounds its lifetime.
///
/// The job is the load-bearing half: with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` set, the
/// kernel terminates the server the moment this process's last handle to the job closes —
/// which happens even when funke crashes and no destructor runs. `kill()` stays for the
/// graceful paths (and as the whole story on the rare machine where the job couldn't be
/// created).
pub struct ServeProcess {
    child: Child,
    _job: Option<KillOnDropJob>,
}

impl ServeProcess {
    pub fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }
}

/// What `/status` reports: the vault state plus the configured server (which decides
/// where favicons come from).
pub struct StatusInfo {
    pub status: String,
    pub server_url: Option<String>,
}

/// Find the CLI, spawn `bw serve`, and wait until `/status` answers.
/// Returns the child (job-bound, see [`ServeProcess`]), the port, and the reported status.
pub fn start(require_signed: bool) -> Result<(ServeProcess, u16, StatusInfo), StartError> {
    start_with(None, require_signed)
}

/// [`start`], but with `BW_SESSION` in the child's environment — the server comes up
/// already unlocked (the Windows Hello path).
pub fn start_with_session(session: &str, require_signed: bool) -> Result<(ServeProcess, u16, StatusInfo), StartError> {
    start_with(Some(session), require_signed)
}

fn start_with(session: Option<&str>, require_signed: bool) -> Result<(ServeProcess, u16, StatusInfo), StartError> {
    let Some(resolved) = cli::cli() else {
        return Err(StartError::NoCli);
    };
    // The opt-in refusal. The default is to warn and carry on — an npm-installed CLI is a
    // shim around a Node script and cannot be signed at all, and bricking that install would
    // teach people to turn the whole check off.
    if require_signed && !resolved.trust.verified() {
        return Err(StartError::Unverified);
    }
    if !cli_available() {
        return Err(StartError::NoCli);
    }
    let port = free_port().map_err(|e| StartError::Failed(format!("no free port: {e}")))?;

    let Some(mut command) = bw() else {
        return Err(StartError::NoCli);
    };
    command
        .args(["serve", "--hostname", "127.0.0.1", "--port", &port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(session) = session {
        command.env("BW_SESSION", session);
    }
    let mut child = command.spawn().map_err(|e| StartError::Failed(e.to_string()))?;

    // Bind the child's lifetime to ours at the kernel: a crashed funke must not leave an
    // unlocked REST API listening. Best-effort — no job is a warning, never a refusal.
    let job = match KillOnDropJob::new().and_then(|job| job.assign(&child).map(|()| job)) {
        Ok(job) => Some(job),
        Err(e) => {
            eprintln!("warning: bw serve is not job-bound (it may outlive a crashed funke): {e}");
            None
        }
    };

    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Ok(status) = status(port) {
            return Ok((ServeProcess { child, _job: job }, port, status));
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
    // The spawn this pin exists for: the master password rides in this child's environment.
    let mut command = bw().ok_or("no bw CLI found")?;
    command
        .args(["unlock", "--raw", "--passwordenv", "FUNKE_BW_PASSWORD"])
        .env("FUNKE_BW_PASSWORD", password)
        .stdin(Stdio::null());
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

/// `bw --version` succeeding is the cheapest "the CLI we pinned actually runs" probe — a
/// file being where we expect it is not the same as it working (a broken npm shim, a
/// half-deleted install).
fn cli_available() -> bool {
    let Some(mut command) = bw() else {
        return false;
    };
    command.arg("--version").stdout(Stdio::null()).stderr(Stdio::null());
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
    /// Bitwarden custom fields. Only `autotype` is read (the per-entry sequence); the
    /// rest — which may hold secrets — is dropped here and never cached.
    fields: Option<Vec<Field>>,
}

#[derive(Deserialize)]
struct Field {
    name: Option<String>,
    value: Option<String>,
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
    let uris = login.and_then(|l| l.uris.as_ref());
    VaultEntry {
        id: item.id,
        name: item.name,
        username: login.and_then(|l| l.username.clone()).filter(|u| !u.is_empty()),
        host: uris
            .and_then(|uris| uris.iter().find_map(|u| u.uri.as_deref().and_then(host_of)))
            .map(str::to_string),
        uri: login_uri(item.fields.as_deref(), uris.map(Vec::as_slice)),
        has_totp: login.and_then(|l| l.totp.as_deref()).is_some_and(|t| !t.is_empty()),
        organization: item.organization_id.and_then(|id| orgs.get(&id).cloned()),
        autotype: item.fields.as_ref().and_then(|fields| autotype_field(fields)),
    }
}

/// Which page "open website & autofill" opens, most specific first:
///
/// 1. the item's **`loginurl` custom field** — the same escape hatch `autotype` is: when a
///    heuristic gets it wrong, the entry itself gets to say where its login lives;
/// 2. the **most login-shaped URI** the item already carries ([`looks_like_login`]) — an
///    item saved with both `github.com` and `github.com/login` means the second one here;
/// 3. failing that, its first web URI (a homepage; the sign-in link on it is the browser's
///    problem, and `funke_shell::click_sign_in` asks the *page* for it rather than guessing
///    a URL — see DESIGN §5).
fn login_uri(fields: Option<&[Field]>, uris: Option<&[Uri]>) -> Option<String> {
    if let Some(explicit) = fields.and_then(named_field("loginurl")).and_then(|uri| web_uri(&uri)) {
        return Some(explicit);
    }
    let web: Vec<String> = uris
        .unwrap_or_default()
        .iter()
        .filter_map(|uri| uri.uri.as_deref().and_then(web_uri))
        .collect();
    web.iter()
        .find(|uri| looks_like_login(uri))
        .or_else(|| web.first())
        .cloned()
}

/// Does this URL's *path* say it is a sign-in page? Only the path is read — a host called
/// `login.example.com` is a site, not a page, and matching on the query string would let
/// `?next=/login` masquerade as one.
fn looks_like_login(uri: &str) -> bool {
    const MARKERS: &[&str] = &["login", "log-in", "signin", "sign-in", "auth", "anmelden", "session"];
    let after_scheme = uri.split_once("://").map_or(uri, |(_, rest)| rest);
    let Some((_host, path)) = after_scheme
        .split(['?', '#'])
        .next()
        .and_then(|rest| rest.split_once('/'))
    else {
        return false;
    };
    let path = path.to_ascii_lowercase();
    MARKERS.iter().any(|marker| path.contains(marker))
}

/// The item's website, as something a browser can be handed — what "open website &
/// autofill" opens. Bitwarden URIs are free text: they carry `androidapp://com.discord`
/// and `iosapp://` entries that name an app rather than a page, and those are skipped
/// (opening one would hand the shell a scheme nothing on Windows answers). A scheme-less
/// `github.com` is a website with the scheme left off, and gets `https://`.
fn web_uri(uri: &str) -> Option<String> {
    let uri = uri.trim();
    let (scheme, rest) = match uri.split_once("://") {
        Some((scheme, rest)) => (Some(scheme.to_ascii_lowercase()), rest),
        None => (None, uri),
    };
    // A host, not a note: an entry whose "URI" is the word "Steam" names an app, and
    // https://Steam is not a page.
    let host = host_of(rest)?;
    if !host.contains('.') && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    match scheme.as_deref() {
        Some("http") | Some("https") => Some(uri.to_string()),
        // A bare host (or `www.…`) — the shape a user types into the address bar.
        None => Some(format!("https://{uri}")),
        Some(_) => None,
    }
}

/// The item's `autotype` custom field — a KeePass-style sequence overriding the default
/// for this login (see [`crate::sequence`]).
fn autotype_field(fields: &[Field]) -> Option<String> {
    named_field("autotype")(fields)
}

/// One custom field by name, non-empty and trimmed. Case-insensitive, and a `funke-`
/// prefix works too (`funke-autotype`, `funke-loginurl`) for vaults that already use the
/// bare name for something of their own. The rest of an item's custom fields — which may
/// hold secrets — is never read.
fn named_field(name: &'static str) -> impl Fn(&[Field]) -> Option<String> {
    move |fields| {
        fields
            .iter()
            .find(|field| {
                field.name.as_deref().is_some_and(|candidate| {
                    let candidate = candidate.trim();
                    candidate.eq_ignore_ascii_case(name) || candidate.eq_ignore_ascii_case(&format!("funke-{name}"))
                })
            })
            .and_then(|field| field.value.clone())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    }
}

/// `https://github.com/login` → `github.com`, tolerant of scheme-less URIs.
pub(crate) fn host_of(uri: &str) -> Option<&str> {
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

    /// What "open website & autofill" is allowed to hand the shell: a page, never an
    /// app-URI or a note the user parked in the URI field.
    #[test]
    fn only_web_uris_can_be_opened() {
        assert_eq!(
            web_uri("https://github.com/login").as_deref(),
            Some("https://github.com/login")
        );
        assert_eq!(web_uri("github.com").as_deref(), Some("https://github.com"));
        assert_eq!(
            web_uri("http://192.168.1.1/admin").as_deref(),
            Some("http://192.168.1.1/admin")
        );
        assert_eq!(web_uri("androidapp://com.discord"), None);
        assert_eq!(web_uri("iosapp://com.steam"), None);
        assert_eq!(web_uri("Steam"), None, "a name is not a website");
        assert_eq!(web_uri(""), None);
    }

    fn uris(list: &[&str]) -> Vec<Uri> {
        list.iter()
            .map(|uri| Uri {
                uri: Some((*uri).to_string()),
            })
            .collect()
    }

    fn field(name: &str, value: &str) -> Field {
        Field {
            name: Some(name.into()),
            value: Some(value.into()),
        }
    }

    /// Which page gets opened, and why: the entry's own say-so first, then the URI that
    /// looks like a sign-in page, then whatever it has. Nothing is ever *constructed* — a
    /// login URL Funke invented is a page nobody vouched for.
    #[test]
    fn the_login_page_is_chosen_from_what_the_entry_carries() {
        // A homepage is what most entries hold; it is used as-is (the sign-in link on it is
        // asked for at open time, not guessed here).
        assert_eq!(
            login_uri(None, Some(&uris(&["https://github.com"]))).as_deref(),
            Some("https://github.com")
        );
        // …but if the entry already knows the login page, that one wins, whatever its order.
        assert_eq!(
            login_uri(None, Some(&uris(&["https://github.com", "https://github.com/login"]))).as_deref(),
            Some("https://github.com/login")
        );
        // The `loginurl` field overrides everything — the escape hatch for a heuristic that
        // got it wrong, exactly like `autotype`.
        assert_eq!(
            login_uri(
                Some(&[field("loginurl", "https://id.example.com/enter")]),
                Some(&uris(&["https://example.com/signin"]))
            )
            .as_deref(),
            Some("https://id.example.com/enter")
        );
        // App-only entries have no page to open at all.
        assert_eq!(login_uri(None, Some(&uris(&["androidapp://com.discord"]))), None);
        assert_eq!(login_uri(None, None), None);
    }

    #[test]
    fn only_a_uris_path_can_make_it_look_like_a_login_page() {
        assert!(looks_like_login("https://github.com/login"));
        assert!(looks_like_login("https://accounts.google.com/signin/v2"));
        assert!(looks_like_login("https://example.de/anmelden"));
        assert!(!looks_like_login("https://github.com"));
        // The host is a site, not a page: `login.example.com/dashboard` is not a login form,
        // and a query string is the site's text, not its address.
        assert!(!looks_like_login("https://login.example.com/dashboard"));
        assert!(!looks_like_login("https://example.com/?next=/login"));
    }

    #[test]
    fn list_response_parses_and_drops_secrets_and_non_logins() {
        let json = r#"{"success":true,"data":{"object":"list","data":[
            {"object":"item","id":"aaa","type":1,"name":"GitHub","organizationId":"org-1",
             "fields":[{"name":"Autotype","value":"{USERNAME}{ENTER}{DELAY=800}{PASSWORD}{ENTER}"},
                       {"name":"recovery","value":"super secret"}],
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
        assert_eq!(
            entries[0].autotype.as_deref(),
            Some("{USERNAME}{ENTER}{DELAY=800}{PASSWORD}{ENTER}"),
            "the autotype custom field is the per-entry sequence"
        );
        assert!(!entries[1].has_totp);
        assert_eq!(entries[1].organization, None, "personal items carry no vault label");
        assert_eq!(entries[1].autotype, None);

        // Only `autotype` is kept — other custom fields (which may hold secrets) are dropped.
        let cached = format!("{:?}", entries);
        assert!(!cached.contains("super secret"));
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
