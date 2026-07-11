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
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn QueryFullProcessImageNameW(process: isize, flags: u32, buf: *mut u16, size: *mut u32) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

const SW_RESTORE: i32 = 9;
/// The least privilege that still names a process — works across integrity levels where
/// `PROCESS_QUERY_INFORMATION` would be refused.
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

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

/// The executable behind a window, as a lowercase stem (`…\Discord.exe` → `discord`).
/// This is what tells the vault that the app you came from *is* Discord, so its
/// credential can be offered. `None` when the process is gone or refuses to be opened
/// (elevated processes do, and get no context — by design, not a bug).
pub fn process_name(hwnd: isize) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process == 0 {
            return None;
        }
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        if !ok || len == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let stem = std::path::Path::new(&path)
            .file_stem()?
            .to_string_lossy()
            .to_lowercase();
        (!stem.is_empty()).then_some(stem)
    }
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
