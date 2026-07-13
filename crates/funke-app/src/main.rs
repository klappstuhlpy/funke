#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autofill;
mod autotype;
mod focus;
mod native;
mod providers;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use funke_core::{Action, FrecencyStore, Query, RecentsStore, Registry, ResultItem, Settings};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

const MAIN_WINDOW: &str = "main";
const SETTINGS_WINDOW: &str = "settings";
/// The overlay height follows its content (`resize_overlay`, driven by the UI);
/// its width comes from settings.
const OVERLAY_MIN_HEIGHT: f64 = 56.0;
const OVERLAY_MAX_HEIGHT: f64 = 560.0;

struct AppState {
    /// Behind an `RwLock` so `reload_plugins` can register newly installed plugins live.
    registry: RwLock<Registry>,
    /// Shared with providers that read preferences per query (e.g. the web engine).
    settings: Arc<RwLock<Settings>>,
    settings_path: PathBuf,
    /// The Bitwarden backend (`bw serve` child + entry cache), shared with its provider.
    vault: Arc<funke_vault::Vault>,
    /// In-memory clipboard history, shared with its provider. Never persisted.
    clipboard: Arc<funke_clipboard::ClipboardHistory>,
    /// Installed out-of-process plugins, shared with their providers for action routing.
    plugins: Arc<funke_plugin::host::PluginManager>,
    plugins_dir: PathBuf,
    frecency: Mutex<FrecencyStore>,
    frecency_path: PathBuf,
    recents: Mutex<RecentsStore>,
    recents_path: PathBuf,
    /// HWND of whatever had focus before the overlay was summoned, so we can hand
    /// focus back on dismiss (and, from M4 on, autotype into it).
    prev_focus: Mutex<Option<isize>>,
    /// What that window *is* — title, process, and (in browsers) the URL in the address
    /// bar. Filled by a background thread on every summon, because reading the URL via
    /// UI Automation costs tens of milliseconds and nothing may sit between the hotkey
    /// and the overlay. Drives the vault's context suggestions and its search boost.
    focus_context: Mutex<funke_vault::FocusContext>,
    /// A Windows Hello prompt is up: its focus steal must not dismiss the overlay.
    hello_in_flight: std::sync::atomic::AtomicBool,
    /// The overlay is in the masked master-password prompt (declared by the UI). One of
    /// the two capture-shield triggers — see [`refresh_capture_shield`].
    shield_prompt: std::sync::atomic::AtomicBool,
    /// The last reply the overlay rendered contained vault rows (derived in `search` /
    /// `overview`). The other capture-shield trigger.
    shield_rows: std::sync::atomic::AtomicBool,
    /// The exclusion state actually applied to the overlay HWND, so redundant
    /// `SetWindowDisplayAffinity` calls are never issued — see [`refresh_capture_shield`].
    shield_applied: std::sync::atomic::AtomicBool,
    /// Bumped by every `search`. A provider that answers after its keystroke's deadline
    /// finds its generation superseded and its rows are dropped — cancellation without the
    /// providers having to cooperate in it (`Registry::search_streaming`).
    query_generation: Arc<std::sync::atomic::AtomicU64>,
    /// What the overlay is currently showing, so a late batch can be merged into it.
    live: Mutex<LiveSearch>,
}

/// The rows on screen for one generation of the search, kept so that a provider arriving
/// after the deadline can be merged into them and re-ranked rather than appended blindly.
#[derive(Default)]
struct LiveSearch {
    generation: u64,
    /// Ranked and boosted — the list `search` replied with, plus every late batch since.
    items: Vec<ResultItem>,
    /// Rows that landed between the fan-out returning and its reply being stored. A
    /// straggler can win that race by microseconds; rather than drop it or emit it *before*
    /// the reply it belongs to, it rides along inside that reply.
    early: Vec<ResultItem>,
    /// The reply for `generation` has been stored: from here on, late rows merge and emit.
    armed: bool,
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One titled group of results; sections are ordered by their best-ranked item.
#[derive(serde::Serialize, Clone)]
struct Section {
    label: String,
    items: Vec<ResultItem>,
}

/// A reply to one keystroke. The generation is what lets the overlay tell a late batch that
/// belongs to the query on screen from one belonging to a query the user has typed past.
#[derive(serde::Serialize, Clone)]
struct SearchReply {
    generation: u64,
    sections: Vec<Section>,
}

#[tauri::command]
fn search(app: AppHandle, state: tauri::State<'_, AppState>, text: String) -> SearchReply {
    use std::sync::atomic::Ordering;

    let generation = state.query_generation.fetch_add(1, Ordering::SeqCst) + 1;
    // Claim the screen for this generation *before* the fan-out, so a provider that answers
    // between the deadline and the reply being stored knows which query it is answering.
    *state.live.lock().unwrap() = LiveSearch {
        generation,
        ..Default::default()
    };

    let settings = state.settings.read().unwrap().clone();
    let registry = state.registry.read().unwrap();
    let late_app = app.clone();
    let mut items = registry.search_streaming(
        &Query::new(text),
        |meta| settings.provider_enabled(meta.id),
        funke_core::DEFAULT_DEADLINE,
        generation,
        Arc::clone(&state.query_generation),
        move |_provider, items| merge_late_results(&late_app, generation, items),
    );

    boost(&state, &mut items);

    let mut live = state.live.lock().unwrap();
    if live.generation != generation {
        // Superseded while we were waiting: the overlay is already showing a newer query.
        // Our rows still go back to our own caller, which drops them — but they must not
        // overwrite what is on screen.
        return SearchReply {
            generation,
            sections: sections_for(&registry, items),
        };
    }
    items.append(&mut live.early);
    items.sort_by_key(|item| std::cmp::Reverse(item.score));
    items.truncate(Registry::MAX_RESULTS);
    live.items = items;
    live.armed = true;
    let reply = SearchReply {
        generation,
        sections: sections_for(&registry, live.items.clone()),
    };
    let has_vault_rows = live.items.iter().any(|item| item.provider == "vault");
    drop(live);

    // Vault rows on screen are one of the capture-shield triggers (SECURITY.md).
    state.shield_rows.store(has_vault_rows, Ordering::SeqCst);
    refresh_capture_shield(&app);
    reply
}

/// A provider that missed the deadline has answered. Merge its rows into the generation
/// they belong to and re-rank the whole list — ranking is the registry's job, so the
/// overlay is handed finished sections and never has to sort anything itself.
///
/// Runs on the orchestrator's collector thread. Nothing here creates a window, which is the
/// one thing that would have to happen on the main thread.
fn merge_late_results(app: &AppHandle, generation: u64, mut items: Vec<ResultItem>) {
    use std::sync::atomic::Ordering;

    let state = app.state::<AppState>();
    if state.query_generation.load(Ordering::SeqCst) != generation {
        return;
    }
    boost(&state, &mut items);

    let mut live = state.live.lock().unwrap();
    if live.generation != generation {
        return;
    }
    if !live.armed {
        // The reply these rows belong to has not been sent yet: ride along inside it.
        live.early.append(&mut items);
        return;
    }
    live.items.append(&mut items);
    live.items.sort_by_key(|item| std::cmp::Reverse(item.score));
    live.items.truncate(Registry::MAX_RESULTS);
    let merged = live.items.clone();
    let has_vault_rows = merged.iter().any(|item| item.provider == "vault");
    // Released before the registry is read: `search` takes the registry first and `live`
    // second, so holding `live` while waiting for the registry here would invert the order
    // — and a `reload_plugins` writer queued between the two readers would deadlock both.
    drop(live);

    let registry = state.registry.read().unwrap();
    let reply = SearchReply {
        generation,
        sections: sections_for(&registry, merged),
    };
    drop(registry);

    // A vault row that arrives late raises the shield exactly as one that arrived on time.
    state.shield_rows.store(has_vault_rows, Ordering::SeqCst);
    refresh_capture_shield(app);
    let _ = app.emit("search-late-results", reply);
}

/// Frecency and vault context, the two things the app knows about ranking that the
/// providers don't. Applied identically to the rows that made the deadline and to the ones
/// that didn't — a late row must not rank differently for having been late.
fn boost(state: &AppState, items: &mut [ResultItem]) {
    let now = unix_now();
    let store = state.frecency.lock().unwrap();
    for item in items.iter_mut() {
        item.score += store.boost(&item.id, now);
    }
    drop(store);

    // Context boost: vault entries belonging to the window that was focused before the
    // overlay (a Steam login window floats the Steam credential, a GitHub tab the GitHub
    // one). Same scorer as the overview's suggestions — see funke_vault::context.
    if !items.iter().any(|item| item.provider == "vault") {
        return;
    }
    let scores = state.vault.context_scores(&state.focus_context.lock().unwrap());
    for item in items.iter_mut() {
        if let Some(entry_id) = item.id.strip_prefix("vault:") {
            item.score += scores.get(entry_id).copied().unwrap_or(0);
        }
    }
}

/// Group ranked rows by section label, keeping global rank order both across sections (a
/// section sits where its best item ranks) and within each section.
fn sections_for(registry: &Registry, items: Vec<ResultItem>) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for item in items {
        let label = registry
            .provider_name(&item.provider)
            .unwrap_or_else(|| funke_core::i18n::t("results.fallback"));
        match sections.iter_mut().find(|section| section.label == label) {
            Some(section) => section.items.push(item),
            None => sections.push(Section {
                label: label.to_string(),
                items: vec![item],
            }),
        }
    }
    sections
}

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Learn what the user picked, so it ranks sooner next time.
fn record_frecency(state: &AppState, id: &str) {
    let mut store = state.frecency.lock().unwrap();
    store.record(id, unix_now());
    if let Err(e) = store.save(&state.frecency_path) {
        eprintln!("failed to persist frecency store: {e}");
    }
}

/// Run one of the item's actions by index (Enter = 0, Shift+Enter = 1, actions menu =
/// any). The UI never interprets actions — it sends the whole item and an index back.
/// Confirmation for destructive actions happens UI-side before this is invoked.
#[tauri::command]
fn run_action(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    item: ResultItem,
    action_index: usize,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let action = item
        .actions
        .get(action_index)
        .or_else(|| item.actions.first())
        .ok_or("item has no actions")?
        .action
        .clone();
    // Copies return focus so the user can paste right away; launches keep the focus
    // they take, so those dismiss without restoring.
    let restore_focus = matches!(action, Action::CopyText { .. });
    match action {
        Action::AppControl { command } if command == "quit" => {
            app.exit(0);
            return Ok(());
        }
        Action::AppControl { command } if command == "settings" => {
            hide(&app, false); // the settings window takes focus itself
            open_settings_window(&app);
            return Ok(());
        }
        Action::AppControl { command } if command == "clipboard-clear" => {
            state.clipboard.clear();
            let _ = app.emit("clipboard-changed", ());
            return Ok(());
        }
        Action::AppControl { command } => return Err(format!("unknown control command: {command}")),
        Action::LaunchApp { target } => {
            launch(&target).map_err(|e| format!("failed to launch {target}: {e}"))?;
        }
        Action::OpenPath { path } => {
            open::that_detached(&path).map_err(|e| format!("failed to open {path}: {e}"))?;
        }
        Action::OpenUrl { url } => {
            open::that_detached(&url).map_err(|e| format!("failed to open {url}: {e}"))?;
        }
        Action::RevealPath { path } => {
            std::process::Command::new("explorer.exe")
                .arg("/select,")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("failed to reveal {path}: {e}"))?;
        }
        Action::CopyText { text } => {
            funke_clipboard::write_text(&text).map_err(|e| format!("failed to copy: {e}"))?;
        }
        Action::PasteText { text } => {
            // Ctrl+V, not keystrokes: a clip may be multi-line, and typing a newline into
            // a chat window sends the half-pasted message (see autotype::paste).
            funke_clipboard::write_text(&text).map_err(|e| format!("failed to copy: {e}"))?;
            let target = state.prev_focus.lock().unwrap().take();
            hide(&app, false);
            if let Some(hwnd) = target {
                focus::focus_window(hwnd);
                // Let the focus change land before the keystroke does.
                std::thread::sleep(std::time::Duration::from_millis(150));
                autotype::paste();
            }
            return Ok(());
        }
        Action::ClipboardForget { id } => {
            state.clipboard.forget(id);
            // The list the user is looking at must lose the row now, not on next summon.
            let _ = app.emit("clipboard-changed", ());
            return Ok(());
        }
        Action::SnippetCopy { id } => {
            let expansion = expand_snippet(&state, &id)?;
            funke_clipboard::write_text(&expansion.text).map_err(|e| format!("failed to copy: {e}"))?;
        }
        Action::SnippetPaste { id } => {
            let expansion = expand_snippet(&state, &id)?;
            // Ctrl+V, not keystrokes: snippets are routinely multi-line, and a typed
            // newline is an Enter — it would send the half-pasted message (autotype::paste).
            funke_clipboard::write_text(&expansion.text).map_err(|e| format!("failed to copy: {e}"))?;
            let target = state.prev_focus.lock().unwrap().take();
            hide(&app, false);
            if let Some(hwnd) = target {
                focus::focus_window(hwnd);
                std::thread::sleep(std::time::Duration::from_millis(150));
                autotype::paste();
                // {CURSOR}: walk the caret back into the hole the snippet left for it.
                autotype::caret_left(expansion.cursor_back);
            }
            return Ok(());
        }
        Action::PromptVaultUnlock => {
            // The overlay stays visible and switches into the masked password prompt.
            let _ = app.emit("vault-unlock", ());
            return Ok(());
        }
        Action::VaultHelloUnlock => {
            use std::sync::atomic::Ordering;
            // Never block here: sync commands run on the main thread, which is an STA
            // — waiting for the WinRT sign operation there deadlocks the event loop
            // before the Hello dialog can even appear. The whole flow gets its own
            // thread; the flag both suppresses hide-on-blur while the dialog is up and
            // swallows repeat presses while one prompt is already pending.
            if state.hello_in_flight.swap(true, Ordering::SeqCst) {
                return Ok(());
            }
            let hwnd = app
                .get_webview_window(MAIN_WINDOW)
                .and_then(|win| win.hwnd().ok())
                .map(|hwnd| hwnd.0 as isize)
                .unwrap_or(0);
            let vault = Arc::clone(&state.vault);
            let app = app.clone();
            std::thread::spawn(move || {
                let unlocked = vault.hello_unlock();
                let state = app.state::<AppState>();
                state.hello_in_flight.store(false, Ordering::SeqCst);
                // The Hello dialog (a system process) took the foreground; a plain
                // set_focus is refused, so force the overlay back with the attach-input
                // dance, then move the webview caret into the search field.
                if hwnd != 0 {
                    focus::force_foreground(hwnd);
                }
                if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
                    let _ = win.set_focus();
                }
                match unlocked {
                    // The overlay re-runs its current `v …` query against the unlocked vault.
                    Ok(()) => {
                        let _ = app.emit("vault-unlocked", ());
                    }
                    // Cancelled / expired session: the overlay falls back to the masked
                    // password prompt with the reason shown.
                    Err(e) => {
                        let _ = app.emit("vault-unlock-failed", e);
                    }
                }
            });
            return Ok(());
        }
        Action::VaultCopy { id, field } => {
            let value = if field == "totp" {
                state.vault.totp(&id)?
            } else {
                let creds = state.vault.credentials(&id)?;
                match field.as_str() {
                    "username" => creds.username.clone(),
                    _ => creds.password.clone(),
                }
                .ok_or_else(|| format!("this item has no {field}"))?
            };
            copy_with_autoclear(value)?;
        }
        Action::PluginInvoke {
            plugin,
            item,
            action_index,
        } => {
            state.plugins.invoke(&plugin, &item, action_index)?;
        }
        // Both autofill flows get a thread of their own, for the same reason
        // `VaultHelloUnlock` does: this command runs on the main thread, which is the
        // event loop and an STA — and they inspect the target window through UI
        // Automation, wait for pages to load, and sleep between keystrokes. See
        // `autofill`, which owns the login-form guard the secrets pass through.
        Action::VaultAutotype { id, force } => {
            // The overlay is *not* hidden here: `autofill` hides it once it knows the
            // secret is going somewhere, so a refusal keeps the window it needs to explain
            // itself in. Frecency is recorded up front, since this arm never reaches the
            // tail below (a credential you autotype is one you reach for).
            record_frecency(&state, &item.id);
            let target = state.prev_focus.lock().unwrap().take();
            let app = app.clone();
            std::thread::spawn(move || autofill::autotype(app, id, target, force));
            return Ok(());
        }
        Action::VaultOpenAutotype { id } => {
            record_frecency(&state, &item.id);
            let target = state.prev_focus.lock().unwrap().take();
            let app = app.clone();
            std::thread::spawn(move || autofill::open_and_autotype(app, id, target));
            return Ok(());
        }
        Action::FocusWindow { hwnd } => {
            // Hide first: the overlay must be gone before foreground moves, and the
            // switched-to window keeps focus (no prev_focus restore).
            hide(&app, false);
            focus::focus_window(hwnd);
        }
        Action::KillProcess { pid } => {
            std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("failed to kill pid {pid}: {e}"))?;
        }
        Action::RunCommand { program, args } => {
            std::process::Command::new(&program)
                .args(&args)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
                .map_err(|e| format!("failed to run {program}: {e}"))?;
        }
    }
    hide(&app, restore_focus);

    // Both stores below are files. The clipboard's whole promise is that it is memory
    // only, so a clip may enter neither: recents would write its *text* to disk, and
    // frecency would key a lasting boost to an id ("clipboard:12") that the next launch
    // hands to an entirely unrelated clip.
    let persistable = item.provider != funke_clipboard::PROVIDER_ID;

    if persistable {
        record_frecency(&state, &item.id);
    }

    // Launcher controls, copied calc results, window handles (stale after the window
    // closes), and plugin rows (item ids may be ephemeral) aren't meaningful
    // "recents". The vault is excluded wholesale — account names are private, and its
    // status rows ("Bitwarden CLI not found", …) wear ordinary OpenUrl actions that
    // an action-based filter would let through.
    if persistable
        && item.provider != "vault"
        && !matches!(
            item.primary_action(),
            Some(
                Action::AppControl { .. }
                    | Action::CopyText { .. }
                    | Action::FocusWindow { .. }
                    | Action::PluginInvoke { .. }
            )
        )
    {
        let mut recents = state.recents.lock().unwrap();
        recents.record(item);
        if let Err(e) = recents.save(&state.recents_path) {
            eprintln!("failed to persist recents store: {e}");
        }
    }
    Ok(())
}

/// Resolve a snippet's placeholders, now — `{DATE}` means today and `{CLIPBOARD}` means
/// whatever is on the clipboard as the action runs, so this can't happen at query time.
fn expand_snippet(state: &AppState, id: &str) -> Result<funke_snippets::Expansion, String> {
    let settings = state.settings.read().unwrap();
    let snippet = funke_snippets::find(&settings, id).ok_or("that snippet no longer exists")?;
    let context = funke_snippets::Context {
        clipboard: funke_clipboard::read_text(),
    };
    Ok(funke_snippets::expand(&snippet.content, &context))
}

/// How long a copied secret may sit on the clipboard before it is wiped.
const CLIPBOARD_CLEAR_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Copy a secret and clear it from the clipboard after [`CLIPBOARD_CLEAR_AFTER`],
/// unless the user has copied something else in the meantime.
///
/// The write carries the clipboard-exclusion markers, so the secret is invisible to
/// clipboard monitors — Funke's own history, the Windows Win+V history, and the cloud
/// clipboard alike. Auto-clear alone never covered those: whatever recorded the password
/// in its own store within the 30 s window kept it there afterwards.
fn copy_with_autoclear(value: String) -> Result<(), String> {
    use zeroize::Zeroize;
    funke_clipboard::write_secret(&value).map_err(|e| format!("failed to copy: {e}"))?;
    std::thread::spawn(move || {
        std::thread::sleep(CLIPBOARD_CLEAR_AFTER);
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            if clipboard.get_text().is_ok_and(|current| current == value) {
                let _ = clipboard.clear();
            }
        }
        let mut value = value;
        value.zeroize();
    });
    Ok(())
}

/// Unlock the vault with the master password typed into the overlay's masked prompt.
/// Async so the KDF (run twice when Hello minting is on) blocks a worker, not the
/// main-thread event loop.
///
/// With Hello enabled this unlock *also* shows a Hello prompt — sealing the new session
/// key means signing a challenge with the TPM, and that is a dialog from another process
/// stealing the foreground. So it needs the same two guards `VaultHelloUnlock` has: the
/// in-flight flag, or the blur hides the overlay out from under the unlock, and the
/// foreground reclaim, or the caret never comes back to the search field.
#[tauri::command]
async fn vault_unlock(app: AppHandle, state: tauri::State<'_, AppState>, password: String) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use zeroize::Zeroize;

    let prompts = state.vault.hello_enabled();
    if prompts {
        state.hello_in_flight.store(true, Ordering::SeqCst);
    }
    let result = state.vault.unlock(&password);
    let mut password = password;
    password.zeroize();

    if prompts {
        state.hello_in_flight.store(false, Ordering::SeqCst);
        if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
            if let Ok(hwnd) = win.hwnd() {
                focus::force_foreground(hwnd.0 as isize);
            }
            let _ = win.set_focus();
        }
    }
    result
}

/// Data for the empty-overlay overview: what the focused app suggests, recently opened
/// items, and a small info line.
#[derive(serde::Serialize)]
struct Overview {
    /// Vault entries for the app that was focused when the overlay was summoned — or the
    /// unlock row, when the vault is locked and can't answer. Empty most of the time.
    suggestions: Vec<ResultItem>,
    /// What those suggestions are *for* ("Discord", "github.com"), for the section label.
    suggestion_label: Option<String>,
    recents: Vec<ResultItem>,
    uptime_secs: u64,
}

/// How many credentials the overview offers at most — a shortlist, not a search.
const MAX_SUGGESTIONS: usize = 3;

#[tauri::command]
fn overview(app: AppHandle, state: tauri::State<'_, AppState>) -> Overview {
    let context = state.focus_context.lock().unwrap().clone();
    let suggestions = if state.settings.read().unwrap().provider_enabled("vault") {
        funke_vault::suggestions(&state.vault, &context, MAX_SUGGESTIONS)
    } else {
        Vec::new()
    };
    // Context suggestions are vault rows (a credential name for the app in front of you),
    // so they raise the capture shield exactly like a `v` search does.
    state
        .shield_rows
        .store(!suggestions.is_empty(), std::sync::atomic::Ordering::SeqCst);
    refresh_capture_shield(&app);
    Overview {
        suggestion_label: (!suggestions.is_empty()).then(|| context.label()).flatten(),
        suggestions,
        recents: state.recents.lock().unwrap().top(5),
        uptime_secs: native::uptime_secs(),
    }
}

/// Drop one item from the recents list (the ✕ on an overview row).
#[tauri::command]
fn remove_recent(state: tauri::State<'_, AppState>, id: String) {
    let mut recents = state.recents.lock().unwrap();
    recents.remove(&id);
    if let Err(e) = recents.save(&state.recents_path) {
        eprintln!("failed to persist recents store: {e}");
    }
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Settings {
    state.settings.read().unwrap().clone()
}

/// Apply and persist new settings. The hotkey is re-registered live; a rejected
/// binding (bad syntax, conflict) restores the old one and errors back to the UI.
#[tauri::command]
fn save_settings(app: AppHandle, state: tauri::State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let old = state.settings.read().unwrap().clone();

    if old.hotkey != settings.hotkey {
        let _ = app.global_shortcut().unregister(old.hotkey.as_str());
        if let Err(e) = register_hotkey(&app, &settings.hotkey) {
            let _ = register_hotkey(&app, &old.hotkey);
            return Err(funke_core::i18n::tf(
                "hotkey.rejected",
                &[("hotkey", &settings.hotkey), ("error", &e.to_string())],
            ));
        }
    }

    if old.autostart != settings.autostart {
        let autolaunch = app.autolaunch();
        let applied = if settings.autostart {
            autolaunch.enable()
        } else {
            autolaunch.disable()
        };
        if let Err(e) = applied {
            eprintln!("autostart toggle failed: {e}");
        }
    }

    // Switching Hello unlock off must also drop the persisted session key.
    if old.vault_hello && !settings.vault_hello {
        state.vault.forget_hello_session();
    }

    // Providers read the locale as they build each row, so this is all a language change
    // takes — the next keystroke comes back translated, with no restart and no re-index.
    apply_locale(&settings);

    *state.settings.write().unwrap() = settings.clone();
    if let Err(e) = settings.save(&state.settings_path) {
        eprintln!("failed to persist settings: {e}");
    }
    // The overlay re-themes itself (accent, width) and re-translates itself off this event.
    let _ = app.emit("settings-changed", &settings);
    // Toggling the capture shield off must release an exclusion that is currently up.
    refresh_capture_shield(&app);
    Ok(())
}

/// Resolve `language` (`auto` → whatever Windows is set to) and hand it to the catalogue.
fn apply_locale(settings: &Settings) {
    let tag = match settings.language.as_str() {
        "auto" | "" => native::user_locale(),
        explicit => explicit.to_string(),
    };
    funke_core::i18n::set_locale(funke_core::Locale::parse(&tag));
}

/// The language the UI should render in — `auto` already resolved, so the frontend never has
/// to guess what Windows is set to.
#[tauri::command]
fn locale() -> &'static str {
    funke_core::i18n::locale().tag()
}

/// A provider the settings UI may toggle (apps and launcher control stay always-on).
#[derive(serde::Serialize)]
struct ToggleableProvider {
    id: &'static str,
    name: &'static str,
}

#[tauri::command]
fn list_providers(state: tauri::State<'_, AppState>) -> Vec<ToggleableProvider> {
    state
        .registry
        .read()
        .unwrap()
        .providers()
        .into_iter()
        // Plugins get their own settings pane; apps + launcher control stay always-on.
        .filter(|meta| !matches!(meta.id, "apps" | "control") && !meta.id.starts_with("plugin:"))
        .map(|meta| ToggleableProvider {
            id: meta.id,
            name: meta.name,
        })
        .collect()
}

/// An installed plugin, as the settings Plugins pane shows it.
#[derive(serde::Serialize)]
struct InstalledPlugin {
    /// Provider id (`plugin:<id>`) — what `disabled_providers` stores.
    id: String,
    name: String,
    version: String,
    description: String,
    prefix: Option<String>,
}

#[tauri::command]
fn list_plugins(state: tauri::State<'_, AppState>) -> Vec<InstalledPlugin> {
    installed_plugins(&state)
}

fn installed_plugins(state: &AppState) -> Vec<InstalledPlugin> {
    let mut plugins: Vec<InstalledPlugin> = state
        .plugins
        .handles()
        .into_iter()
        .map(|handle| {
            let manifest = &handle.manifest;
            InstalledPlugin {
                id: format!("plugin:{}", manifest.id),
                name: manifest.name.clone(),
                version: manifest.version.clone(),
                description: manifest.description.clone(),
                prefix: manifest.prefix.clone(),
            }
        })
        .collect();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}

/// Open (creating if needed) the folder plugins are installed into.
#[tauri::command]
fn open_plugins_folder(state: tauri::State<'_, AppState>) -> Result<(), String> {
    std::fs::create_dir_all(&state.plugins_dir).map_err(|e| e.to_string())?;
    open::that_detached(&state.plugins_dir).map_err(|e| e.to_string())
}

/// Re-scan the plugins folder and register any newly installed plugins live, so a
/// freshly dropped-in plugin works without relaunching. Additive only (see
/// `PluginManager::reload`); returns the refreshed installed list for the UI.
#[tauri::command]
fn reload_plugins(state: tauri::State<'_, AppState>) -> Vec<InstalledPlugin> {
    load_new_plugins(&state);
    installed_plugins(&state)
}

fn load_new_plugins(state: &AppState) {
    let added = state.plugins.reload(&state.plugins_dir);
    if !added.is_empty() {
        let mut registry = state.registry.write().unwrap();
        for handle in added {
            registry.register(Box::new(funke_plugin::host::PluginProvider::new(handle)));
        }
    }
}

/// A catalog entry as the Plugins pane shows it: the curated listing plus whether it is
/// already on disk.
#[derive(serde::Serialize)]
struct CatalogPlugin {
    #[serde(flatten)]
    entry: funke_plugin::catalog::CatalogEntry,
    installed: bool,
}

/// Fetch the curated index. Async — and the fetch itself goes to a blocking thread, because
/// a sync command would run on the main (STA) thread and freeze the settings window for the
/// length of a network round trip.
#[tauri::command]
async fn browse_plugins(app: AppHandle) -> Result<Vec<CatalogPlugin>, String> {
    let entries =
        tauri::async_runtime::spawn_blocking(|| funke_plugin::catalog::fetch(funke_plugin::catalog::CATALOG_URL))
            .await
            .map_err(|e| e.to_string())??;
    let state = app.state::<AppState>();
    let installed: std::collections::HashSet<String> = state
        .plugins
        .handles()
        .iter()
        .map(|handle| handle.manifest.id.clone())
        .collect();
    Ok(entries
        .into_iter()
        .map(|entry| CatalogPlugin {
            installed: installed.contains(&entry.id),
            entry,
        })
        .collect())
}

/// Install a catalog entry: fetch the index again (so the pinned hash is the one currently
/// under review, not one a stale UI is holding), download, verify, unpack, then register the
/// plugin live. The frontend never gets to name a URL or a hash — only an id in the catalog.
#[tauri::command]
async fn install_plugin(app: AppHandle, id: String) -> Result<Vec<InstalledPlugin>, String> {
    let dir = app.state::<AppState>().plugins_dir.clone();
    let installed = tauri::async_runtime::spawn_blocking(move || {
        let entry = funke_plugin::catalog::fetch(funke_plugin::catalog::CATALOG_URL)?
            .into_iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| format!("`{id}` is not in the plugin catalog"))?;
        funke_plugin::catalog::install(&entry, &dir)
    })
    .await
    .map_err(|e| e.to_string())?;
    installed?;
    let state = app.state::<AppState>();
    load_new_plugins(&state);
    Ok(installed_plugins(&state))
}

/// Uninstall: stop the child process, drop its provider, then delete the folder — in that
/// order, because Windows will not delete a running executable.
#[tauri::command]
async fn remove_plugin(app: AppHandle, id: String) -> Result<Vec<InstalledPlugin>, String> {
    let (plugins, dir) = {
        let state = app.state::<AppState>();
        (Arc::clone(&state.plugins), state.plugins_dir.clone())
    };
    let bare = id.strip_prefix("plugin:").unwrap_or(&id).to_string();
    {
        let state = app.state::<AppState>();
        state.registry.write().unwrap().unregister(&format!("plugin:{bare}"));
    }
    let removed = tauri::async_runtime::spawn_blocking(move || {
        plugins.remove(&bare);
        funke_plugin::catalog::remove(&bare, &dir)
    })
    .await
    .map_err(|e| e.to_string())?;
    removed?;
    Ok(installed_plugins(&app.state::<AppState>()))
}

#[derive(serde::Serialize)]
struct Engine {
    id: &'static str,
    name: &'static str,
}

/// Whether Everything is serving file search right now, so the Files pane can say which
/// index it is talking about. Asked when the pane opens — it is a fact about this moment.
#[tauri::command]
fn everything_is_indexing() -> bool {
    funke_files::everything_is_indexing()
}

/// Open a link from the settings window in the user's browser (the About pane's links).
/// The scheme is checked rather than trusted: a command is callable by anything running in
/// the webview, and `open` hands whatever it is given to the shell — which would happily
/// launch a local executable or a UNC path.
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("refusing to open a non-https link: {url}"));
    }
    open::that_detached(&url).map_err(|e| format!("failed to open {url}: {e}"))
}

#[tauri::command]
fn list_engines() -> Vec<Engine> {
    funke_utils::ENGINES
        .iter()
        .map(|(id, name, _)| Engine { id, name })
        .collect()
}

/// Native folder picker for the file-index roots list in settings. Blocking is fine:
/// commands run off the main thread, and the settings window just waits for the dialog.
#[tauri::command]
fn pick_index_root(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    app.dialog().file().blocking_pick_folder().map(|path| path.to_string())
}

/// Check GitHub Releases for a newer version and, if found, download + stage the update
/// (applied on next launch). **Dormant until configured**: returns a friendly message
/// when `plugins.updater` (endpoints + a signing `pubkey`) isn't set in tauri.conf.json —
/// see docs/DESIGN.md §8 for the one-time keypair setup.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app
        .updater()
        .map_err(|_| funke_core::i18n::t("update.unconfigured").to_string())?;
    match updater.check().await.map_err(|e| e.to_string())? {
        Some(update) => {
            let version = update.version.clone();
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| e.to_string())?;
            Ok(funke_core::i18n::tf("update.installed", &[("version", &version)]))
        }
        None => Ok(funke_core::i18n::t("update.none").to_string()),
    }
}

/// The settings window starts hidden and calls this once its DOM is styled, so it
/// never flashes an unstyled webview.
#[tauri::command]
fn settings_ready(app: AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn close_settings(app: AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = win.close();
    }
}

/// Create (or refocus) the settings window — **never on the main thread**.
///
/// `WebviewWindowBuilder::build()` creates the window on the event loop and blocks until
/// the webview is ready. Sync commands run on the main thread, which *is* the event loop:
/// building there deadlocks — the HWND appears, the call never returns, and the wedged
/// creation takes every later window operation down with it (which is why the tray's
/// Settings item stopped responding too, once the overlay's had hung). Off the main thread
/// the loop stays free to finish the job, so both callers hop onto a thread of their own.
/// Same seam, same reason as `VaultHelloUnlock`.
fn open_settings_window(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || build_settings_window(&app));
}

/// Unlike the overlay the settings window is a normal one: built on demand, destroyed on
/// close — invariant 2 covers only the overlay. Never call this on the main thread; see above.
fn build_settings_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App("settings.html".into()))
        .title("Funke Settings")
        .inner_size(780.0, 560.0)
        // Fixed size: the panes are laid out for this one, the window is frameless (there is
        // no grip to drag anyway), and a settings window is a dialog — nothing in it rewards
        // being made bigger. The content column scrolls instead.
        .resizable(false)
        .maximizable(false)
        .decorations(false)
        .shadow(true)
        .center()
        .visible(false) // shown by settings_ready once the UI has painted
        .build();
    match built {
        Ok(win) => {
            if let Ok(hwnd) = win.hwnd() {
                native::round_corners(hwnd.0 as isize);
            }
        }
        Err(e) => eprintln!("failed to open settings window: {e}"),
    }
}

/// Bind the summon hotkey. Used at startup and whenever settings change it.
fn register_hotkey(app: &AppHandle, hotkey: &str) -> Result<(), tauri_plugin_global_shortcut::Error> {
    app.global_shortcut().on_shortcut(hotkey, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            toggle(app);
        }
    })
}

fn launch(target: &str) -> std::io::Result<()> {
    if target.starts_with("shell:") {
        // AUMIDs (Start Menu / Store apps) can only be launched through the shell.
        std::process::Command::new("explorer.exe")
            .arg(target)
            .spawn()
            .map(|_| ())
    } else {
        std::process::Command::new(target).spawn().map(|_| ())
    }
}

#[tauri::command]
fn hide_overlay(app: AppHandle) {
    hide(&app, true);
}

/// The UI entering/leaving the masked master-password prompt. The prompt is drawn by the
/// webview, so only the UI knows the moment it appears — the shield must be up before the
/// first masked character is typed.
#[tauri::command]
fn set_capture_shield(app: AppHandle, state: tauri::State<'_, AppState>, active: bool) {
    state.shield_prompt.store(active, std::sync::atomic::Ordering::SeqCst);
    refresh_capture_shield(&app);
}

/// Apply the capture-shield policy to the overlay window: excluded from screen capture
/// while it shows vault content (the masked prompt and/or vault rows) and the setting is
/// on; capturable otherwise. Recomputed on every trigger change, so plain results never
/// stay stuck behind a stale exclusion.
///
/// Two guards protect the transparent WebView2 window, which reacts to
/// `SetWindowDisplayAffinity` — even a no-op `WDA_NONE` — by materializing an opaque
/// (white) surface: the affinity is only ever touched on a **real transition**, and only
/// while the window is **visible**. Without them, the webview's boot-time overview (it
/// pre-renders while hidden) painted a blank bar over the desktop at every startup.
/// Ordering keeps this sound: `hide` clears the triggers and refreshes *before* the
/// window hides, so an engaged shield is always released while the window can take the
/// call; a trigger that flips while hidden is applied by the next visible refresh (the
/// summon path re-queries the overview before any vault content can render).
fn refresh_capture_shield(app: &AppHandle) {
    use std::sync::atomic::Ordering;
    let state = app.state::<AppState>();
    let exclude = state.settings.read().unwrap().vault_capture_shield
        && (state.shield_prompt.load(Ordering::SeqCst) || state.shield_rows.load(Ordering::SeqCst));
    let Some(win) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if !win.is_visible().unwrap_or(false) {
        return;
    }
    if state.shield_applied.swap(exclude, Ordering::SeqCst) == exclude {
        return;
    }
    if let Ok(hwnd) = win.hwnd() {
        native::set_capture_exclusion(hwnd.0 as isize, exclude);
    }
}

/// Grow/shrink the window to fit the result list (called by the UI after each render),
/// keeping the top edge anchored so the panel expands downward like Spotlight.
#[tauri::command]
fn resize_overlay(app: AppHandle, state: tauri::State<'_, AppState>, height: f64) {
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        let width = state.settings.read().unwrap().overlay_width;
        let height = height.clamp(OVERLAY_MIN_HEIGHT, OVERLAY_MAX_HEIGHT);
        let _ = win.set_size(tauri::LogicalSize::new(width, height));
    }
}

/// Center horizontally, roughly a quarter down the screen — the Spotlight position.
fn position_overlay(win: &tauri::WebviewWindow) {
    if let (Ok(Some(monitor)), Ok(size)) = (win.current_monitor(), win.outer_size()) {
        let mpos = monitor.position();
        let msize = monitor.size();
        let x = mpos.x + ((msize.width as i32 - size.width as i32) / 2).max(0);
        let y = mpos.y + (msize.height as f64 * 0.24) as i32;
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }
}

fn show(app: &AppHandle) {
    let state = app.state::<AppState>();
    let previous = focus::foreground_window();
    *state.prev_focus.lock().unwrap() = previous;
    // Stale context must never outlive its window: clear now, refill in the background.
    *state.focus_context.lock().unwrap() = funke_vault::FocusContext::default();
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        position_overlay(&win);
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit("overlay-shown", ());
    }
    if let Some(hwnd) = previous {
        capture_context(app, hwnd);
    }
}

/// Work out what the window we came from *is* — its title, its process, and, in a
/// browser, the URL in the address bar (UI Automation, which can take tens of
/// milliseconds and would otherwise be felt between the hotkey and the overlay).
///
/// Off-thread by design: the overlay is already up and rendering its overview when this
/// starts, so the result arrives via `focus-context` and the overview refreshes in place.
fn capture_context(app: &AppHandle, hwnd: isize) {
    let app = app.clone();
    std::thread::spawn(move || {
        let process = focus::process_name(hwnd);
        let browser = process.as_deref().is_some_and(funke_shell::is_browser_process);
        let context = funke_vault::FocusContext {
            title: focus::window_title(hwnd),
            url: browser.then(|| funke_shell::browser_url(hwnd)).flatten(),
            process,
            browser,
        };
        let state = app.state::<AppState>();
        // A summon that came and went while we were reading the tree must not be
        // retro-fitted with a context from the window before it.
        if *state.prev_focus.lock().unwrap() != Some(hwnd) {
            return;
        }
        *state.focus_context.lock().unwrap() = context;
        let _ = app.emit("focus-context", ());
    });
}

fn hide(app: &AppHandle, restore_focus: bool) {
    // A hidden window keeps its display affinity, so drop the shield with the content:
    // the next summon starts on the overview, which decides its own shielding.
    {
        use std::sync::atomic::Ordering;
        let state = app.state::<AppState>();
        state.shield_prompt.store(false, Ordering::SeqCst);
        state.shield_rows.store(false, Ordering::SeqCst);
        // Dismissing the overlay retires the query on it: a provider still working on that
        // keystroke now finds its generation superseded, exactly as if the user had typed
        // on. Nothing arrives into an overlay that has been put away.
        state.query_generation.fetch_add(1, Ordering::SeqCst);
    }
    refresh_capture_shield(app);
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        let _ = win.hide();
        // Lets the UI reset to the overview while invisible, so the next summon
        // never flashes the previous search for a frame.
        let _ = win.emit("overlay-hidden", ());
    }
    if restore_focus {
        let state = app.state::<AppState>();
        let prev = state.prev_focus.lock().unwrap().take();
        if let Some(hwnd) = prev {
            focus::focus_window(hwnd);
        }
    }
}

fn toggle(app: &AppHandle) {
    let visible = app
        .get_webview_window(MAIN_WINDOW)
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        hide(app, true);
    } else {
        show(app);
    }
}

fn build_registry(
    settings: Arc<RwLock<Settings>>,
    vault: Arc<funke_vault::Vault>,
    clipboard: Arc<funke_clipboard::ClipboardHistory>,
    plugins: &funke_plugin::host::PluginManager,
) -> Registry {
    let mut registry = Registry::new();
    registry.register(Box::new(providers::ControlProvider));
    registry.register(Box::new(funke_apps::AppsProvider::spawn()));
    registry.register(Box::new(funke_files::FilesProvider::spawn(Arc::clone(&settings))));
    registry.register(Box::new(funke_utils::CalcProvider));
    registry.register(Box::new(funke_utils::SystemProvider));
    registry.register(Box::new(funke_utils::WebSearchProvider::spawn(Arc::clone(&settings))));
    registry.register(Box::new(funke_windows::WindowsProvider::new()));
    registry.register(Box::new(funke_vault::VaultProvider::new(vault)));
    registry.register(Box::new(funke_clipboard::ClipboardProvider::new(clipboard)));
    registry.register(Box::new(funke_snippets::SnippetsProvider::new(Arc::clone(&settings))));
    for handle in plugins.handles() {
        registry.register(Box::new(funke_plugin::host::PluginProvider::new(handle)));
    }
    registry
}

fn data_path(file: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("funke")
        .join(file)
}

/// The installer's "Start Funke when I sign in" checkbox leaves a marker file here rather
/// than writing the Run key itself (`installer/hooks.nsh`): the key's exact shape — value
/// name, quoting, the StartupApproved companion entry — belongs to auto-launch, and routing
/// the request through `settings.autostart` is what keeps the Settings toggle and the
/// registry from ever disagreeing. Consumed once; `setup` then enables it like any other
/// persisted choice. Only ever turns autostart *on*, so a reinstall can't undo a user's no.
fn consume_autostart_request(settings: &RwLock<Settings>, settings_path: &Path) {
    let marker = data_path(".autostart-request");
    if !marker.exists() {
        return;
    }
    std::fs::remove_file(&marker).ok();
    let mut settings = settings.write().unwrap();
    if settings.autostart {
        return;
    }
    settings.autostart = true;
    if let Err(e) = settings.save(settings_path) {
        eprintln!("failed to persist the installer's autostart request: {e}");
    }
}

fn main() {
    // Before anything can hold a secret: crash dumps of this process must not be
    // collected (a WER dump would carry whatever was in flight when it died).
    native::exclude_from_error_reporting();
    let settings_path = data_path("settings.json");
    let frecency_path = data_path("frecency.json");
    let recents_path = data_path("recents.json");
    let settings = Arc::new(RwLock::new(Settings::load(&settings_path)));
    consume_autostart_request(&settings, &settings_path);
    let vault = Arc::new(funke_vault::Vault::new(Arc::clone(&settings)));
    // Recording starts now, not on the first `c` query: a history that only remembers
    // what you copied *after* you thought to open it would be useless.
    let clipboard = funke_clipboard::ClipboardHistory::new(Arc::clone(&settings));
    clipboard.watch();
    let plugins_dir = data_path("plugins");
    let plugins = Arc::new(funke_plugin::host::PluginManager::discover(&plugins_dir));
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            registry: RwLock::new(build_registry(
                Arc::clone(&settings),
                Arc::clone(&vault),
                Arc::clone(&clipboard),
                &plugins,
            )),
            settings,
            settings_path,
            vault,
            clipboard,
            plugins,
            plugins_dir,
            frecency: Mutex::new(FrecencyStore::load(&frecency_path)),
            frecency_path,
            recents: Mutex::new(RecentsStore::load(&recents_path)),
            recents_path,
            prev_focus: Mutex::new(None),
            focus_context: Mutex::new(funke_vault::FocusContext::default()),
            hello_in_flight: std::sync::atomic::AtomicBool::new(false),
            shield_prompt: std::sync::atomic::AtomicBool::new(false),
            shield_rows: std::sync::atomic::AtomicBool::new(false),
            shield_applied: std::sync::atomic::AtomicBool::new(false),
            query_generation: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            live: Mutex::new(LiveSearch::default()),
        })
        .invoke_handler(tauri::generate_handler![
            search,
            run_action,
            hide_overlay,
            resize_overlay,
            set_capture_shield,
            overview,
            remove_recent,
            get_settings,
            save_settings,
            list_providers,
            list_engines,
            locale,
            everything_is_indexing,
            open_url,
            pick_index_root,
            settings_ready,
            close_settings,
            vault_unlock,
            list_plugins,
            open_plugins_folder,
            reload_plugins,
            browse_plugins,
            install_plugin,
            remove_plugin,
            check_update
        ])
        .setup(|app| {
            // Native glass: acrylic backdrop + DWM rounded corners; the window shadow
            // comes from DWM too ("shadow": true), so CSS never fakes any of it.
            if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
                if let Ok(hwnd) = win.hwnd() {
                    native::round_corners(hwnd.0 as isize);
                }
                if let Err(e) = window_vibrancy::apply_acrylic(&win, Some((26, 24, 21, 160))) {
                    eprintln!("acrylic backdrop unavailable ({e}); panel tint carries the theme alone");
                }
            }

            let settings = app.state::<AppState>().settings.read().unwrap().clone();

            // Before anything builds a row or a menu: providers translate as they go, so the
            // catalogue has to know the language first.
            apply_locale(&settings);

            // Re-render the overlay when background favicon fetches land, so vault
            // icons appear in place instead of only after a close/reopen.
            {
                let handle = app.handle().clone();
                app.state::<AppState>().vault.set_icons_listener(move || {
                    let _ = handle.emit("vault-icons-updated", ());
                });
            }

            // Boot `bw serve` now (on its own thread) rather than on the first `v`
            // query, so the vault answers the moment it is summoned. With the
            // provider toggled off nothing starts; re-enabling falls back to the
            // lazy first-query start.
            if settings.provider_enabled("vault") {
                app.state::<AppState>().vault.ensure_started();
            }

            // Registered here (not via Builder::with_shortcuts) so a conflict with another
            // launcher degrades to a warning instead of aborting startup.
            if let Err(e) = register_hotkey(app.handle(), &settings.hotkey) {
                eprintln!(
                    "failed to register {}: {e} — is another launcher (e.g. PowerToys Run) using it?",
                    settings.hotkey
                );
            }

            // Re-assert the persisted choice; the registry entry may have been removed
            // externally (e.g. via Task Manager's startup tab).
            if settings.autostart {
                if let Err(e) = app.autolaunch().enable() {
                    eprintln!("failed to enable autostart: {e}");
                }
            }

            // The tray menu is built once, so it wears the language chosen at startup; a
            // change takes effect on the next launch. Everything the user reads *during* a
            // session — results, settings — retranslates live.
            let show_item = MenuItem::with_id(
                app,
                "show",
                funke_core::tf("tray.show", &[("hotkey", &settings.hotkey)]),
                true,
                None::<&str>,
            )?;
            let settings_item = MenuItem::with_id(app, "settings", funke_core::t("tray.settings"), true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", funke_core::t("tray.quit"), true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &settings_item, &quit_item])?;
            TrayIconBuilder::with_id("funke-tray")
                .icon(app.default_window_icon().expect("bundle icon is configured").clone())
                .tooltip("Funke")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show" => show(app),
                    "settings" => open_settings_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            println!(
                "Funke is running in the tray - press {} to open the overlay.",
                settings.hotkey
            );
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != MAIN_WINDOW {
                return;
            }
            match event {
                // Clicking outside dismisses the overlay; focus already moved on its own,
                // so don't steal it back. (Unless the blur is the Windows Hello dialog
                // taking over mid-unlock — the overlay must survive that.)
                WindowEvent::Focused(false) => {
                    let hello = window
                        .app_handle()
                        .state::<AppState>()
                        .hello_in_flight
                        .load(std::sync::atomic::Ordering::SeqCst);
                    if hello {
                        return;
                    }
                    let _ = window.hide();
                    let _ = window.emit("overlay-hidden", ());
                }
                // Alt+F4 & friends hide instead of destroying the only window.
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    let _ = window.emit("overlay-hidden", ());
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to run funke")
        .run(|app, event| {
            // Child processes (bw serve, plugins) must never outlive the launcher.
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<AppState>();
                state.vault.shutdown();
                state.plugins.shutdown();
            }
        });
}
