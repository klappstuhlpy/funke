//! The launcher's half: discover installed plugins, run each as a child process
//! (spawned lazily on its first query), and adapt them into `SearchProvider`s.
//!
//! Install layout — one folder per plugin under the plugins directory:
//!
//! ```text
//! %APPDATA%/funke/plugins/
//! └── my-plugin/
//!     ├── plugin.json      ← manifest (see [`Manifest`])
//!     └── my-plugin.exe    ← `entry`, any executable speaking the protocol
//! ```
//!
//! Each plugin gets one worker thread that owns its stdio and serializes requests.
//! Queries wait [`QUERY_TIMEOUT`] then give up (invariant 6: a slow plugin may lose
//! its own results, never block the launcher); a dead/crashed plugin yields empty
//! results until restart. Discovery runs once at startup.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, SyncSender};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

use funke_core::{Action, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};
use serde::Deserialize;

use crate::proto::{PluginInfo, PluginItem, QueryResult, Request, Response, PROTOCOL_VERSION};

const QUERY_TIMEOUT: Duration = Duration::from_millis(300);
const INVOKE_TIMEOUT: Duration = Duration::from_secs(5);

/// `plugin.json`, next to the plugin executable.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    /// Stable identifier (lowercase, no spaces) — becomes provider id `plugin:<id>`.
    pub id: String,
    /// Display name: the results section label and the settings row title.
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Executable path relative to the plugin's folder.
    pub entry: String,
    /// Optional scope keyword, like the built-ins' `f`/`w`/`v`.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Only answer prefix-scoped queries (see `ProviderMeta::prefix_only`).
    #[serde(default)]
    pub prefix_only: bool,
}

type Job = (Request, SyncSender<Result<serde_json::Value, String>>);

pub struct PluginHandle {
    pub manifest: Manifest,
    dir: PathBuf,
    /// `None` until first use; `Some(None)` if the plugin failed permanently.
    worker: OnceLock<Option<Sender<Job>>>,
    next_id: Mutex<u64>,
}

impl PluginHandle {
    fn new(manifest: Manifest, dir: PathBuf) -> Self {
        Self {
            manifest,
            dir,
            worker: OnceLock::new(),
            next_id: Mutex::new(0),
        }
    }

    pub fn query(&self, text: &str) -> Vec<PluginItem> {
        let result = self.request("query", serde_json::json!({ "text": text }), QUERY_TIMEOUT);
        match result {
            Ok(value) => serde_json::from_value::<QueryResult>(value)
                .map(|r| r.items)
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }

    pub fn invoke(&self, item_id: &str, action_index: usize) -> Result<(), String> {
        self.request(
            "invoke",
            serde_json::json!({ "item_id": item_id, "action_index": action_index }),
            INVOKE_TIMEOUT,
        )
        .map(|_| ())
    }

    fn request(&self, method: &str, params: serde_json::Value, timeout: Duration) -> Result<serde_json::Value, String> {
        let sender = self
            .ensure_running()
            .as_ref()
            .ok_or_else(|| format!("plugin {} is not running", self.manifest.id))?;
        let id = {
            let mut next = self.next_id.lock().unwrap();
            *next += 1;
            *next
        };
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        sender
            .send((Request::new(id, method, params), reply_tx))
            .map_err(|_| format!("plugin {} exited", self.manifest.id))?;
        match reply_rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(format!("plugin {} timed out", self.manifest.id)),
            Err(RecvTimeoutError::Disconnected) => Err(format!("plugin {} died mid-request", self.manifest.id)),
        }
    }

    /// Spawn the child + worker on first use. A failed spawn is remembered as `None`
    /// so a broken plugin costs one attempt, not one per keystroke.
    fn ensure_running(&self) -> &Option<Sender<Job>> {
        self.worker
            .get_or_init(|| match spawn_worker(&self.manifest, &self.dir) {
                Ok(sender) => Some(sender),
                Err(e) => {
                    eprintln!("plugin {} failed to start: {e}", self.manifest.id);
                    None
                }
            })
    }
}

/// Boot the child process, run the `initialize` handshake, and hand back the job
/// channel of a worker thread that owns the child's stdio for its lifetime.
fn spawn_worker(manifest: &Manifest, dir: &Path) -> Result<Sender<Job>, String> {
    let entry = dir.join(&manifest.entry);
    let mut command = Command::new(&entry);
    command
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| format!("{}: {e}", entry.display()))?;
    let mut stdin = child.stdin.take().ok_or("no stdin pipe")?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout pipe")?);

    // Handshake synchronously so version mismatches fail at load time.
    let info: PluginInfo = {
        let request = Request::new(0, "initialize", serde_json::json!({ "protocol": PROTOCOL_VERSION }));
        let value = roundtrip(&mut stdin, &mut stdout, &request)?;
        serde_json::from_value(value).map_err(|e| format!("bad initialize result: {e}"))?
    };
    if info.protocol != PROTOCOL_VERSION {
        let _ = child.kill();
        return Err(format!(
            "protocol mismatch: plugin speaks v{}, host v{PROTOCOL_VERSION}",
            info.protocol
        ));
    }

    let (job_tx, job_rx): (Sender<Job>, Receiver<Job>) = mpsc::channel();
    let plugin_id = manifest.id.clone();
    std::thread::spawn(move || {
        for (request, reply) in job_rx {
            let result = roundtrip(&mut stdin, &mut stdout, &request);
            let died = result.is_err();
            let _ = reply.send(result); // receiver may have timed out — fine
            if died {
                break;
            }
        }
        let _ = stdin.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":0,\"method\":\"shutdown\"}\n");
        let _ = child.kill();
        eprintln!("plugin {plugin_id} worker stopped");
    });
    Ok(job_tx)
}

/// Write one request, read lines until the matching response id comes back.
fn roundtrip(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
    request: &Request,
) -> Result<serde_json::Value, String> {
    let mut line = serde_json::to_string(request).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
    stdin.flush().map_err(|e| e.to_string())?;

    let mut buf = String::new();
    loop {
        buf.clear();
        let read = stdout.read_line(&mut buf).map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("plugin closed its stdout".into());
        }
        let Ok(response) = serde_json::from_str::<Response>(&buf) else {
            continue; // stray output on stdout — skip
        };
        if response.id != request.id {
            continue; // stale answer to a timed-out request
        }
        return match (response.result, response.error) {
            (Some(result), _) => Ok(result),
            (None, Some(error)) => Err(error.message),
            (None, None) => Err("response had neither result nor error".into()),
        };
    }
}

/// All installed plugins, shared between their providers and `run_action` routing.
#[derive(Default)]
pub struct PluginManager {
    plugins: HashMap<String, Arc<PluginHandle>>,
}

impl PluginManager {
    /// Scan `<dir>/*/plugin.json` once. Unreadable manifests are skipped with a log
    /// line — one broken plugin must not take the launcher down (invariant 4).
    pub fn discover(dir: &Path) -> Self {
        let mut plugins = HashMap::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Self { plugins };
        };
        for entry in entries.flatten() {
            let folder = entry.path();
            let manifest_path = folder.join("plugin.json");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest: Manifest = match std::fs::read_to_string(&manifest_path)
                .map_err(|e| e.to_string())
                .and_then(|raw| serde_json::from_str(&raw).map_err(|e| e.to_string()))
            {
                Ok(manifest) => manifest,
                Err(e) => {
                    eprintln!("skipping plugin at {}: {e}", manifest_path.display());
                    continue;
                }
            };
            plugins.insert(manifest.id.clone(), Arc::new(PluginHandle::new(manifest, folder)));
        }
        Self { plugins }
    }

    pub fn handles(&self) -> impl Iterator<Item = &Arc<PluginHandle>> {
        self.plugins.values()
    }

    pub fn invoke(&self, plugin_id: &str, item_id: &str, action_index: usize) -> Result<(), String> {
        self.plugins
            .get(plugin_id)
            .ok_or_else(|| format!("no such plugin: {plugin_id}"))?
            .invoke(item_id, action_index)
    }

    /// Ask every *running* plugin to exit (never-started ones have no process).
    /// The worker kills the child if the polite shutdown doesn't take.
    pub fn shutdown(&self) {
        for handle in self.plugins.values() {
            if let Some(Some(sender)) = handle.worker.get() {
                let (reply_tx, reply_rx) = mpsc::sync_channel(1);
                let _ = sender.send((Request::new(u64::MAX, "shutdown", serde_json::Value::Null), reply_tx));
                let _ = reply_rx.recv_timeout(Duration::from_millis(200));
            }
        }
    }
}

/// One installed plugin as a `SearchProvider`. Metadata strings must be `&'static`,
/// so the (few, small, once-per-run) manifest strings are deliberately leaked.
pub struct PluginProvider {
    handle: Arc<PluginHandle>,
    id: &'static str,
    name: &'static str,
    prefix: Option<&'static str>,
}

impl PluginProvider {
    pub fn new(handle: Arc<PluginHandle>) -> Self {
        let manifest = &handle.manifest;
        Self {
            id: Box::leak(format!("plugin:{}", manifest.id).into_boxed_str()),
            name: Box::leak(manifest.name.clone().into_boxed_str()),
            prefix: manifest
                .prefix
                .clone()
                .map(|prefix| &*Box::leak(prefix.into_boxed_str())),
            handle,
        }
    }
}

impl SearchProvider for PluginProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: self.id,
            name: self.name,
            prefix: self.prefix,
            prefix_only: self.handle.manifest.prefix_only,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        if query.is_empty() {
            return Vec::new();
        }
        let plugin_id = &self.handle.manifest.id;
        self.handle
            .query(&query.text)
            .into_iter()
            .map(|item| to_result_item(plugin_id, self.id, item))
            .collect()
    }
}

fn to_result_item(plugin_id: &str, provider_id: &str, item: PluginItem) -> ResultItem {
    let mut actions: Vec<NamedAction> = item
        .actions
        .iter()
        .enumerate()
        .map(|(index, action)| NamedAction {
            label: action.label.clone(),
            confirm: action.confirm,
            action: Action::PluginInvoke {
                plugin: plugin_id.to_string(),
                item: item.id.clone(),
                action_index: index,
            },
        })
        .collect();
    if actions.is_empty() {
        // Items must always be runnable; default to invoking action 0.
        actions.push(NamedAction::new(
            "Run",
            Action::PluginInvoke {
                plugin: plugin_id.to_string(),
                item: item.id.clone(),
                action_index: 0,
            },
        ));
    }
    ResultItem {
        id: format!("{provider_id}:{}", item.id),
        provider: provider_id.to_string(),
        title: item.title,
        subtitle: item.subtitle,
        icon: item.icon,
        score: item.score,
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::PluginAction;

    #[test]
    fn plugin_items_map_to_opaque_invoke_routes() {
        let item = PluginItem {
            id: "row-1".into(),
            title: "Hello".into(),
            subtitle: None,
            icon: None,
            score: 42,
            actions: vec![
                PluginAction {
                    label: "Copy".into(),
                    confirm: false,
                },
                PluginAction {
                    label: "Delete".into(),
                    confirm: true,
                },
            ],
        };
        let result = to_result_item("demo", "plugin:demo", item);
        assert_eq!(result.id, "plugin:demo:row-1");
        assert_eq!(result.actions.len(), 2);
        assert!(result.actions[1].confirm);
        assert!(matches!(
            &result.actions[1].action,
            Action::PluginInvoke { plugin, item, action_index } if plugin == "demo" && item == "row-1" && *action_index == 1
        ));
    }

    #[test]
    fn actionless_items_get_a_default_run_action() {
        let item = PluginItem {
            id: "r".into(),
            title: "T".into(),
            subtitle: None,
            icon: None,
            score: 1,
            actions: vec![],
        };
        let result = to_result_item("demo", "plugin:demo", item);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].label, "Run");
    }

    #[test]
    fn discovery_survives_missing_dirs_and_broken_manifests() {
        let manager = PluginManager::discover(Path::new("Z:\\definitely\\missing"));
        assert_eq!(manager.handles().count(), 0);

        let dir = std::env::temp_dir().join("funke-plugin-discovery-test");
        let broken = dir.join("broken");
        std::fs::create_dir_all(&broken).unwrap();
        std::fs::write(broken.join("plugin.json"), "not json {").unwrap();
        let good = dir.join("good");
        std::fs::create_dir_all(&good).unwrap();
        std::fs::write(
            good.join("plugin.json"),
            r#"{"id":"good","name":"Good","entry":"good.exe"}"#,
        )
        .unwrap();

        let manager = PluginManager::discover(&dir);
        assert_eq!(manager.handles().count(), 1);
        assert!(manager.plugins.contains_key("good"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
