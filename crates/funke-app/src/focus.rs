//! Minimal user32 FFI for capturing/restoring the foreground window around overlay toggles.
//! Two functions aren't worth the full `windows` crate; HWND is pointer-sized, so `isize`
//! is ABI-correct here.

#[link(name = "user32")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn GetWindowTextW(hwnd: isize, buf: *mut u16, len: i32) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn AttachThreadInput(id_attach: u32, id_attach_to: u32, attach: i32) -> i32;
    fn BringWindowToTop(hwnd: isize) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentThreadId() -> u32;
}

const SW_RESTORE: i32 = 9;

/// The window that had focus before the overlay was summoned, if any.
pub fn foreground_window() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    (hwnd != 0).then_some(hwnd)
}

/// Title of a window (for matching vault entries against the app that was focused
/// before the overlay). `None` when the window is gone or untitled.
pub fn window_title(hwnd: isize) -> Option<String> {
    let mut buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    (len > 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
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

/// Force a window of *our own* process to the foreground even when another process
/// currently owns it. Windows normally refuses `SetForegroundWindow` unless the caller
/// is the foreground process, so we briefly attach our input queue to the foreground
/// thread's (the documented workaround) — needed after the Windows Hello dialog (a
/// system process) closes and would otherwise leave the overlay unfocused.
pub fn force_foreground(hwnd: isize) {
    unsafe {
        if IsIconic(hwnd) != 0 {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let fg = GetForegroundWindow();
        if fg == hwnd {
            return;
        }
        let fg_thread = if fg != 0 {
            GetWindowThreadProcessId(fg, std::ptr::null_mut())
        } else {
            0
        };
        let cur_thread = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != cur_thread && AttachThreadInput(cur_thread, fg_thread, 1) != 0;
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(cur_thread, fg_thread, 0);
        }
    }
}
