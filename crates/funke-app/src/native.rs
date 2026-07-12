//! Native window dressing Tauri doesn't expose: DWM rounded corners (Windows 11).
//! Same philosophy as `focus.rs` — one attribute call isn't worth a crate.

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const core::ffi::c_void, size: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount64() -> u64;
    fn GetUserDefaultLocaleName(name: *mut u16, size: i32) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn SetWindowDisplayAffinity(hwnd: isize, affinity: u32) -> i32;
}

/// Invisible to capture (screenshots, recorders, screen shares), still visible on the
/// monitor. Windows 10 2004+.
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
/// Older fallback: the window captures as a black rectangle. Uglier, same protection.
const WDA_MONITOR: u32 = 0x1;
const WDA_NONE: u32 = 0x0;

/// Hide a window from screen capture (or stop hiding it). Best-effort: on a Windows too
/// old for `WDA_EXCLUDEFROMCAPTURE` the black-box `WDA_MONITOR` is tried, and a refusal
/// of both is accepted silently — the shield is a hardening layer, never a gate.
pub fn set_capture_exclusion(hwnd: isize, exclude: bool) {
    unsafe {
        if !exclude {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_NONE);
        } else if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
            let _ = SetWindowDisplayAffinity(hwnd, WDA_MONITOR);
        }
    }
}

/// Seconds since boot, for the overview's info line.
pub fn uptime_secs() -> u64 {
    unsafe { GetTickCount64() / 1000 }
}

/// Windows' own language tag for this user (`de-DE`, `en-US`), which is what `language: auto`
/// follows. Empty if Windows won't say, and an empty tag reads as English.
pub fn user_locale() -> String {
    // LOCALE_NAME_MAX_LENGTH.
    let mut name = [0u16; 85];
    let written = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), name.len() as i32) };
    if written <= 1 {
        return String::new();
    }
    // The count includes the terminating null.
    String::from_utf16_lossy(&name[..written as usize - 1])
}

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

#[link(name = "wer")]
extern "system" {
    fn WerAddExcludedApplication(app: *const u16, all_users: i32) -> i32;
}

/// Keep funke out of Windows Error Reporting: a WER dump of a crashed funke could carry
/// whatever vault secret was in flight when it died. Per-user (HKCU, no admin), best
/// effort, and honest about its limits — an admin-configured LocalDumps policy or an
/// attached debugger still dumps; this closes the default collection path only.
pub fn exclude_from_error_reporting() {
    let name: Vec<u16> = "funke.exe".encode_utf16().chain(std::iter::once(0)).collect();
    let hr = unsafe { WerAddExcludedApplication(name.as_ptr(), 0) };
    if hr < 0 {
        eprintln!("warning: could not exclude funke from Windows Error Reporting (0x{hr:08x})");
    }
}

/// Ask DWM to round the window's corners (no-op before Windows 11).
pub fn round_corners(hwnd: isize) {
    let preference = DWMWCP_ROUND;
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            (&preference as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
}
