#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autotype;
mod focus;
mod native;
mod providers;

use std::path::PathBuf;
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
    registry: Registry,
    /// Shared with providers that read preferences per query (e.g. the web engine).
    settings: Arc<RwLock<Settings>>,
    settings_path: PathBuf,
    /// The Bitwarden backend (`bw serve` child + entry cache), shared with its provider.
    vault: Arc<funke_vault::Vault>,
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
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// One titled group of results; sections are ordered by their best-ranked item.
#[derive(serde::Serialize)]
struct Section {
    label: String,
    items: Vec<ResultItem>,
}

#[tauri::command]
fn search(state: tauri::State<'_, AppState>, text: String) -> Vec<Section> {
    let settings = state.settings.read().unwrap().clone();
    let mut items = state
        .registry
        .search_enabled(&Query::new(text), |meta| settings.provider_enabled(meta.id));
    let store = state.frecency.lock().unwrap();
    let now = unix_now();
    for item in &mut items {
        item.score += store.boost(&item.id, now);
    }
    items.sort_by_key(|item| std::cmp::Reverse(item.score));

    // Group by section label, keeping global rank order both across sections (a section
    // sits where its best item ranks) and within each section.
    let mut sections: Vec<Section> = Vec::new();
    for item in items {
        let label = state.registry.provider_name(&item.provider).unwrap_or("Results");
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
            arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.set_text(text))
                .map_err(|e| format!("failed to copy: {e}"))?;
        }
        Action::PromptVaultUnlock => {
            // The overlay stays visible and switches into the masked password prompt.
            let _ = app.emit("vault-unlock", ());
            return Ok(());
        }
        Action::VaultCopy { id, field } => {
            let creds = state.vault.credentials(&id)?;
            let value = match field.as_str() {
                "username" => creds.username.clone(),
                _ => creds.password.clone(),
            }
            .ok_or_else(|| format!("this item has no {field}"))?;
            copy_with_autoclear(value)?;
        }
        Action::PluginInvoke {
            plugin,
            item,
            action_index,
        } => {
            state.plugins.invoke(&plugin, &item, action_index)?;
        }
        Action::VaultAutotype { id } => {
            let creds = state.vault.credentials(&id)?;
            let target = state.prev_focus.lock().unwrap().take();
            hide(&app, false);
            if let Some(hwnd) = target {
                focus::focus_window(hwnd);
            }
            // Give the focus change a beat to land before keystrokes flow.
            std::thread::sleep(std::time::Duration::from_millis(150));
            if let Some(username) = creds.username.as_deref() {
                autotype::type_text(username);
                autotype::press(autotype::VK_TAB);
            }
            if let Some(password) = creds.password.as_deref() {
                autotype::type_text(password);
                autotype::press(autotype::VK_RETURN);
            }
            // creds zeroize on drop (funke-vault Credentials).
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

    let mut store = state.frecency.lock().unwrap();
    store.record(&item.id, unix_now());
    if let Err(e) = store.save(&state.frecency_path) {
        eprintln!("failed to persist frecency store: {e}");
    }
    drop(store);

    // Launcher controls, copied calc results, window handles (stale after the window
    // closes), vault entries (account names are private), and plugin rows (item ids
    // may be ephemeral) aren't meaningful "recents".
    if !matches!(
        item.primary_action(),
        Some(
            Action::AppControl { .. }
                | Action::CopyText { .. }
                | Action::FocusWindow { .. }
                | Action::VaultAutotype { .. }
                | Action::VaultCopy { .. }
                | Action::PromptVaultUnlock
                | Action::PluginInvoke { .. }
        )
    ) {
        let mut recents = state.recents.lock().unwrap();
        recents.record(item);
        if let Err(e) = recents.save(&state.recents_path) {
            eprintln!("failed to persist recents store: {e}");
        }
    }
    Ok(())
}

/// How long a copied secret may sit on the clipboard before it is wiped.
const CLIPBOARD_CLEAR_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Copy a secret and clear it from the clipboard after [`CLIPBOARD_CLEAR_AFTER`],
/// unless the user has copied something else in the meantime.
fn copy_with_autoclear(value: String) -> Result<(), String> {
    use zeroize::Zeroize;
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(&value))
        .map_err(|e| format!("failed to copy: {e}"))?;
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
#[tauri::command]
fn vault_unlock(state: tauri::State<'_, AppState>, password: String) -> Result<(), String> {
    use zeroize::Zeroize;
    let result = state.vault.unlock(&password);
    let mut password = password;
    password.zeroize();
    result
}

/// Data for the empty-overlay overview: recently opened items plus a small info line.
#[derive(serde::Serialize)]
struct Overview {
    recents: Vec<ResultItem>,
    uptime_secs: u64,
}

#[tauri::command]
fn overview(state: tauri::State<'_, AppState>) -> Overview {
    Overview {
        recents: state.recents.lock().unwrap().top(5),
        uptime_secs: native::uptime_secs(),
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
            return Err(format!("Couldn't bind “{}”: {e}", settings.hotkey));
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

    *state.settings.write().unwrap() = settings.clone();
    if let Err(e) = settings.save(&state.settings_path) {
        eprintln!("failed to persist settings: {e}");
    }
    // The overlay re-themes itself (accent, width) off this event.
    let _ = app.emit("settings-changed", &settings);
    Ok(())
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
    let mut plugins: Vec<InstalledPlugin> = state
        .plugins
        .handles()
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

#[derive(serde::Serialize)]
struct Engine {
    id: &'static str,
    name: &'static str,
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

/// Create (or refocus) the settings window. Unlike the overlay it is a normal window:
/// built on demand, destroyed on close — invariant 2 covers only the overlay.
fn open_settings_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS_WINDOW, WebviewUrl::App("settings.html".into()))
        .title("Funke Settings")
        .inner_size(780.0, 560.0)
        .min_inner_size(640.0, 440.0)
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
    *state.prev_focus.lock().unwrap() = focus::foreground_window();
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        position_overlay(&win);
        let _ = win.show();
        let _ = win.set_focus();
        let _ = win.emit("overlay-shown", ());
    }
}

fn hide(app: &AppHandle, restore_focus: bool) {
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
    plugins: &funke_plugin::host::PluginManager,
) -> Registry {
    let mut registry = Registry::new();
    registry.register(Box::new(providers::ControlProvider));
    registry.register(Box::new(funke_apps::AppsProvider::spawn()));
    registry.register(Box::new(funke_files::FilesProvider::spawn(Arc::clone(&settings))));
    registry.register(Box::new(funke_utils::CalcProvider));
    registry.register(Box::new(funke_utils::SystemProvider));
    registry.register(Box::new(funke_utils::WebSearchProvider::spawn(settings)));
    registry.register(Box::new(funke_windows::WindowsProvider::new()));
    registry.register(Box::new(funke_vault::VaultProvider::new(vault)));
    for handle in plugins.handles() {
        registry.register(Box::new(funke_plugin::host::PluginProvider::new(Arc::clone(handle))));
    }
    registry
}

fn data_path(file: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("funke")
        .join(file)
}

fn main() {
    let settings_path = data_path("settings.json");
    let frecency_path = data_path("frecency.json");
    let recents_path = data_path("recents.json");
    let settings = Arc::new(RwLock::new(Settings::load(&settings_path)));
    let vault = Arc::new(funke_vault::Vault::new());
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
        .manage(AppState {
            registry: build_registry(Arc::clone(&settings), Arc::clone(&vault), &plugins),
            settings,
            settings_path,
            vault,
            plugins,
            plugins_dir,
            frecency: Mutex::new(FrecencyStore::load(&frecency_path)),
            frecency_path,
            recents: Mutex::new(RecentsStore::load(&recents_path)),
            recents_path,
            prev_focus: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            search,
            run_action,
            hide_overlay,
            resize_overlay,
            overview,
            get_settings,
            save_settings,
            list_providers,
            list_engines,
            pick_index_root,
            settings_ready,
            close_settings,
            vault_unlock,
            list_plugins,
            open_plugins_folder
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

            let show_item = MenuItem::with_id(app, "show", format!("Show ({})", settings.hotkey), true, None::<&str>)?;
            let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
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
                // so don't steal it back.
                WindowEvent::Focused(false) => {
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
