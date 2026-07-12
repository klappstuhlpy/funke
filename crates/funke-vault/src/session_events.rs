//! Lock the vault the moment the user walks away: session lock, sleep/hibernate, and
//! RDP disconnect, delivered as window messages instead of waiting for the watchdog's
//! next 30-second poll (which only ever saw the lock case — see [`crate::lockscreen`];
//! that poll stays, as the belt to this module's braces).
//!
//! Hand-written FFI like `lockscreen.rs` — the surface is small and self-contained.
//!
//! One subtlety decides the whole shape: `WM_POWERBROADCAST` and `WM_WTSSESSION_CHANGE`
//! are *broadcast* messages, and message-only windows (`HWND_MESSAGE`) never receive
//! broadcasts. So this is a real — invisible, never shown — top-level `WS_POPUP` window
//! on its own pumping thread, the same thread pattern the clipboard listener uses.
//!
//! Suspend is reported on both edges on purpose: `PBT_APMSUSPEND` is not guaranteed to
//! arrive before a fast sleep, so `PBT_APMRESUMEAUTOMATIC` (which always follows a wake)
//! is the trigger that cannot be missed. Locking an already-locked vault is a no-op.

use std::os::raw::c_void;
use std::sync::OnceLock;

type Hwnd = *mut c_void;
type Hinstance = *mut c_void;

const WS_POPUP: u32 = 0x8000_0000;

const WM_POWERBROADCAST: u32 = 0x0218;
const PBT_APMSUSPEND: usize = 0x4;
const PBT_APMRESUMEAUTOMATIC: usize = 0x12;

const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const WTS_REMOTE_DISCONNECT: usize = 0x4;
const WTS_SESSION_LOCK: usize = 0x7;
const NOTIFY_FOR_THIS_SESSION: u32 = 0;

#[repr(C)]
struct WndClassW {
    style: u32,
    wnd_proc: extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: Hinstance,
    icon: *mut c_void,
    cursor: *mut c_void,
    background: *mut c_void,
    menu_name: *const u16,
    class_name: *const u16,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: [i32; 2],
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    #[allow(clippy::too_many_arguments)]
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: *mut c_void,
        instance: Hinstance,
        param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Hinstance;
}

#[link(name = "wtsapi32")]
extern "system" {
    fn WTSRegisterSessionNotification(hwnd: Hwnd, flags: u32) -> i32;
}

/// One watcher per process, alive for its lifetime — the same deal the vault watchdog
/// has, and for the same reason: there is exactly one vault, and it never goes away.
static ON_WALK_AWAY: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

extern "system" fn wnd_proc(hwnd: Hwnd, msg: u32, wparam: usize, lparam: isize) -> isize {
    let walked_away = match msg {
        WM_WTSSESSION_CHANGE => matches!(wparam, WTS_SESSION_LOCK | WTS_REMOTE_DISCONNECT),
        WM_POWERBROADCAST => matches!(wparam, PBT_APMSUSPEND | PBT_APMRESUMEAUTOMATIC),
        _ => false,
    };
    if walked_away {
        if let Some(callback) = ON_WALK_AWAY.get() {
            callback();
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Start watching for "the user walked away" (session lock, RDP disconnect,
/// suspend/resume). The callback fires on the watcher thread; it decides for itself
/// whether locking applies (settings, current status). Second and later calls are
/// no-ops. Every failure path degrades silently to the watchdog poll.
pub fn watch(on_walk_away: impl Fn() + Send + Sync + 'static) {
    if ON_WALK_AWAY.set(Box::new(on_walk_away)).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let class_name = wide("funke_session_events");
        let class = WndClassW {
            style: 0,
            wnd_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance: GetModuleHandleW(std::ptr::null()),
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: class_name.as_ptr(),
        };
        if RegisterClassW(&class) == 0 {
            return;
        }
        // Top-level (parent null) and WS_POPUP, never shown: broadcasts reach it, the
        // screen never does. A message-only window would miss the broadcasts entirely.
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        // WM_POWERBROADCAST needs no registration; the WTS session messages do.
        let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);

        let mut msg = std::mem::zeroed::<Msg>();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
