//! Window switcher provider: fuzzy-match the titles of open top-level windows.
//!
//! Enumeration runs per keystroke — `EnumWindows` over a few dozen windows is far below
//! the keystroke budget, and a snapshot can't go stale that way. Icons come from each
//! window's process executable and are cached per exe path. Enter switches to the
//! window; the secondary action force-kills its process (confirmed in the UI).

use std::collections::HashMap;
use std::sync::Mutex;

use funke_core::{Action, FuzzyMatcher, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};
use windows::core::BOOL;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, MAX_PATH};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

pub struct WindowsProvider {
    /// Icon data URLs keyed by exe path — extraction is the only per-window cost worth
    /// caching; titles and pids are re-read every query so results never go stale.
    icon_cache: Mutex<HashMap<String, Option<String>>>,
}

impl WindowsProvider {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            icon_cache: Mutex::new(HashMap::new()),
        }
    }
}

struct OpenWindow {
    hwnd: isize,
    title: String,
    pid: u32,
    exe: Option<String>,
}

impl SearchProvider for WindowsProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "windows",
            name: "Windows",
            prefix: Some("w"),
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let Some(matcher) = FuzzyMatcher::new(&query.text) else {
            return Vec::new();
        };
        open_windows()
            .into_iter()
            .filter_map(|win| {
                let process = win.exe.as_deref().and_then(|exe| {
                    std::path::Path::new(exe)
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                });
                // Match on the title and on the process name, whichever scores better.
                let score = matcher
                    .score(&win.title)
                    .into_iter()
                    .chain(process.as_deref().and_then(|name| matcher.score(name)))
                    .max()?;
                Some(ResultItem {
                    id: format!("windows:{}", win.pid),
                    provider: "windows".into(),
                    title: win.title,
                    subtitle: process,
                    icon: win.exe.as_deref().and_then(|exe| self.icon_for(exe)),
                    score,
                    actions: vec![
                        NamedAction::new("Switch to", Action::FocusWindow { hwnd: win.hwnd }),
                        NamedAction::confirmed("End process", Action::KillProcess { pid: win.pid }),
                    ],
                })
            })
            .collect()
    }
}

impl WindowsProvider {
    fn icon_for(&self, exe: &str) -> Option<String> {
        let mut cache = self.icon_cache.lock().unwrap();
        cache
            .entry(exe.to_string())
            .or_insert_with(|| funke_shell::icon_data_url(exe))
            .clone()
    }
}

/// Every switchable top-level window: visible, titled, not a tool window, not cloaked
/// (cloaked = invisible UWP leftovers and windows on other virtual desktops' backstage),
/// and not our own.
fn open_windows() -> Vec<OpenWindow> {
    let mut windows: Vec<OpenWindow> = Vec::new();
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<OpenWindow>) };
        if let Some(win) = unsafe { inspect(hwnd) } {
            windows.push(win);
        }
        BOOL(1) // keep enumerating
    }
    let _ = unsafe { EnumWindows(Some(visit), LPARAM(&mut windows as *mut _ as isize)) };
    windows
}

unsafe fn inspect(hwnd: HWND) -> Option<OpenWindow> {
    if !IsWindowVisible(hwnd).as_bool() {
        return None;
    }
    if GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
        return None;
    }
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED,
        (&mut cloaked as *mut u32).cast(),
        std::mem::size_of::<u32>() as u32,
    );
    if cloaked != 0 {
        return None;
    }

    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; len as usize + 1];
    let copied = GetWindowTextW(hwnd, &mut buf);
    let title = String::from_utf16_lossy(&buf[..copied as usize]);
    if title.trim().is_empty() {
        return None;
    }

    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 || pid == GetCurrentProcessId() {
        return None;
    }

    Some(OpenWindow {
        hwnd: hwnd.0 as isize,
        title,
        pid,
        exe: process_exe(pid),
    })
}

fn process_exe(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; MAX_PATH as usize];
        let mut len = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_does_not_blow_up_and_skips_untitled_windows() {
        // CI sessions may have zero switchable windows; only invariants are asserted.
        for win in open_windows() {
            assert!(!win.title.trim().is_empty());
            assert_ne!(win.pid, 0);
        }
    }

    #[test]
    fn results_carry_switch_and_confirmed_kill_actions() {
        let provider = WindowsProvider::new();
        for item in provider.query(&Query::new("a")) {
            assert_eq!(item.actions.len(), 2);
            assert!(!item.actions[0].confirm);
            assert!(item.actions[1].confirm, "kill must require confirmation");
        }
    }
}
