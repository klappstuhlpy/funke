//! Native window dressing Tauri doesn't expose: DWM rounded corners (Windows 11).
//! Same philosophy as `focus.rs` — one attribute call isn't worth a crate.

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attribute: u32, value: *const core::ffi::c_void, size: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetTickCount64() -> u64;
}

/// Seconds since boot, for the overview's info line.
pub fn uptime_secs() -> u64 {
    unsafe { GetTickCount64() / 1000 }
}

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

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
