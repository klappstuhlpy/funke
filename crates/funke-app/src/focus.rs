//! Minimal user32 FFI for capturing/restoring the foreground window around overlay toggles.
//! Two functions aren't worth the full `windows` crate; HWND is pointer-sized, so `isize`
//! is ABI-correct here.

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
}

const SW_RESTORE: i32 = 9;

/// The window that had focus before the overlay was summoned, if any.
pub fn foreground_window() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    (hwnd != 0).then_some(hwnd)
}

/// Bring a window to the foreground, restoring it first if it's minimized (the window
/// switcher targets those too). Also used to hand focus back after dismissing the
/// overlay, and ahead of autotype in M4.
pub fn focus_window(hwnd: isize) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}
