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
    fn AttachConsole(process_id: u32) -> i32;
    fn GetStdHandle(id: u32) -> isize;
    fn SetStdHandle(id: u32, handle: isize) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const core::ffi::c_void,
        disposition: u32,
        flags: u32,
        template: isize,
    ) -> isize;
}

const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;
const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5; // (DWORD)-11
const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4; // (DWORD)-12
const GENERIC_WRITE: u32 = 0x4000_0000;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const OPEN_EXISTING: u32 = 3;
const INVALID_HANDLE_VALUE: isize = -1;

/// Borrow the console we were *launched from*, if there is one — and never conjure one.
///
/// Funke is a windows-subsystem binary, so Windows gives it no console: starting it from
/// the Start menu, a shortcut, or the autostart entry opens no black rectangle, which is
/// the entire point (a tray app that flashes a terminal at sign-in looks broken). The cost
/// is that a `cargo run` or a `funke.exe` typed into a shell would print into the void.
/// `AttachConsole(ATTACH_PARENT_PROCESS)` buys it back: joined to the parent's console, the
/// process's warnings land in the terminal the user is looking at.
pub fn attach_parent_console() {
    unsafe {
        // No parent console (Explorer, the Run key, the tray) — nothing to attach to, and
        // deliberately nothing to allocate.
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        adopt(STD_OUTPUT_HANDLE);
        adopt(STD_ERROR_HANDLE);
    }
}

/// Point one standard handle at the console we just joined — unless it already points
/// somewhere, which means the shell redirected it (`funke > log.txt`, a pipe). Overwriting
/// that would quietly break the redirect, so an existing handle always wins.
unsafe fn adopt(id: u32) {
    let current = GetStdHandle(id);
    if current != 0 && current != INVALID_HANDLE_VALUE {
        return;
    }
    let name: Vec<u16> = "CONOUT$".encode_utf16().chain(std::iter::once(0)).collect();
    let console = CreateFileW(
        name.as_ptr(),
        GENERIC_WRITE,
        FILE_SHARE_WRITE,
        std::ptr::null(),
        OPEN_EXISTING,
        0,
        0,
    );
    if console != INVALID_HANDLE_VALUE {
        let _ = SetStdHandle(id, console);
    }
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
