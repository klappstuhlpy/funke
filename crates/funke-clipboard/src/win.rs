//! The Win32 clipboard: reading what was copied, writing text back, and being told when
//! it changes.
//!
//! Two things here are load-bearing beyond "move text around".
//!
//! **The exclusion formats.** Windows lets a copier mark its content as not-for-recording
//! — `ExcludeClipboardContentFromMonitorProcessing`, plus `CanIncludeInClipboardHistory`
//! and `CanUploadToCloudClipboard` set to 0. Password managers (Bitwarden, KeePass,
//! 1Password) set them on every secret they copy. We honour them on the way *in*
//! ([`read_text`] returns `None`), so another manager's password never lands in our
//! history — and we set them on the way *out* ([`write_secret`]), so the vault's own
//! copies stay out of our history, out of Win+V, and off the cloud clipboard. That last
//! part is a fix in its own right: a secret copied from Funke used to be recorded by
//! Windows like any other text.
//!
//! **The listener.** `AddClipboardFormatListener` needs an HWND with a message pump, so
//! [`watch`] owns a message-only window on its own thread. It never touches the UI and
//! never blocks the caller.

use std::sync::OnceLock;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, HWND_MESSAGE,
    MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE, WNDCLASSW,
};

/// Content marked with any of these is somebody's secret — never record it.
const EXCLUDE_FORMATS: [PCWSTR; 3] = [
    w!("ExcludeClipboardContentFromMonitorProcessing"),
    w!("CanIncludeInClipboardHistory"),
    w!("CanUploadToCloudClipboard"),
];

/// The clipboard is a single global lock, and *every* app that touches it holds it for a
/// moment — including other clipboard managers, which wake on the same notification we do
/// and read it at the same instant. Losing that race must not cost us the clip, so we wait
/// through the contention rather than a token handful of tries.
const OPEN_RETRIES: u32 = 25;
const OPEN_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(20);

/// The clipboard is a global lock: hold it for as short as possible, always release it.
struct ClipboardLock;

impl ClipboardLock {
    fn acquire() -> Option<Self> {
        for attempt in 0..OPEN_RETRIES {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Some(Self);
            }
            if attempt + 1 < OPEN_RETRIES {
                std::thread::sleep(OPEN_RETRY_DELAY);
            }
        }
        None
    }
}

impl Drop for ClipboardLock {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

/// What was on the clipboard — and, when there was no text for us, *why*.
///
/// The distinction matters to the recorder: "somebody's secret" and "not text" mean there
/// is deliberately nothing to keep, but [`Read::Busy`] means we simply lost the race for
/// the lock, and treating that as "nothing" would silently punch holes in the history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    Text(String),
    /// Marked as excluded from clipboard monitors — a password manager's copy.
    Excluded,
    /// An image, a file list, an empty clipboard: nothing this crate records.
    NotText,
    /// Another process held the clipboard open for longer than we were willing to wait.
    Busy,
}

/// Read the clipboard, saying why when there is nothing to take.
pub fn read() -> Read {
    let Some(_lock) = ClipboardLock::acquire() else {
        return Read::Busy;
    };
    if is_excluded() {
        return Read::Excluded;
    }
    match read_unicode_text() {
        Some(text) => Read::Text(text),
        None => Read::NotText,
    }
}

/// The Unicode text on the clipboard, or `None` when there is none — or when its owner
/// marked it as excluded from clipboard monitors, which is precisely what a password
/// manager does with a password. (`{CLIPBOARD}` in a snippet wants exactly this: text or
/// nothing, with no interest in the reason.)
pub fn read_text() -> Option<String> {
    match read() {
        Read::Text(text) => Some(text),
        _ => None,
    }
}

/// Caller must hold the clipboard lock.
fn read_unicode_text() -> Option<String> {
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT.0 as u32) }.ok()?;
    let global = HGLOBAL(handle.0);
    let ptr = unsafe { GlobalLock(global) } as *const u16;
    if ptr.is_null() {
        return None;
    }
    // The clipboard's text is NUL-terminated; find it rather than trusting GlobalSize,
    // which reports the allocation, not the string.
    let mut len = 0usize;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(ptr, len) });
    let _ = unsafe { GlobalUnlock(global) };
    Some(text)
}

/// Does the current clipboard content carry one of the "don't record me" markers?
fn is_excluded() -> bool {
    EXCLUDE_FORMATS.iter().any(|name| {
        let format = unsafe { RegisterClipboardFormatW(*name) };
        format != 0 && unsafe { IsClipboardFormatAvailable(format) }.is_ok()
    })
}

/// Put ordinary text on the clipboard.
pub fn write_text(text: &str) -> Result<(), String> {
    write(text, false)
}

/// Put a *secret* on the clipboard: the same text, plus the markers that keep it out of
/// clipboard monitors — ours, the Windows Win+V history, and the cloud clipboard.
pub fn write_secret(text: &str) -> Result<(), String> {
    write(text, true)
}

fn write(text: &str, secret: bool) -> Result<(), String> {
    let _lock = ClipboardLock::acquire().ok_or("the clipboard is busy")?;
    // Taking ownership is what EmptyClipboard means; only then may we set formats.
    unsafe { EmptyClipboard() }.map_err(|e| e.to_string())?;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let handle = alloc_copy(bytes_of(&wide))?;
    // The system owns the memory once SetClipboardData succeeds — and only then, so a
    // failure means we still hold it and must not leak the clipboard lock either.
    unsafe { SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0))) }.map_err(|e| e.to_string())?;

    if secret {
        for name in EXCLUDE_FORMATS {
            let format = unsafe { RegisterClipboardFormatW(name) };
            if format == 0 {
                continue;
            }
            // The convention these markers use is a DWORD 0 payload.
            if let Ok(zero) = alloc_copy(&0u32.to_ne_bytes()) {
                let _ = unsafe { SetClipboardData(format, Some(HANDLE(zero.0))) };
            }
        }
    }
    Ok(())
}

fn bytes_of(wide: &[u16]) -> &[u8] {
    // Safety: u16 has no padding and no invalid bit patterns; this is a plain reinterpret.
    unsafe { std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), std::mem::size_of_val(wide)) }
}

/// A moveable global block holding `bytes` — the shape every clipboard format wants.
fn alloc_copy(bytes: &[u8]) -> Result<HGLOBAL, String> {
    let global = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }.map_err(|e| e.to_string())?;
    let ptr = unsafe { GlobalLock(global) };
    if ptr.is_null() {
        return Err("failed to lock clipboard memory".into());
    }
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    let _ = unsafe { GlobalUnlock(global) };
    Ok(global)
}

type Callback = Box<dyn Fn() + Send + Sync + 'static>;
static ON_CHANGE: OnceLock<Callback> = OnceLock::new();

/// Call `on_change` whenever the clipboard's content changes, for the life of the process.
///
/// Spawns the message-only window and its pump on a dedicated thread — `GetMessageW`
/// blocks, and the listener must never sit on a thread anything else needs. Calling this
/// more than once is a no-op: there is one clipboard, so one listener.
pub fn watch(on_change: impl Fn() + Send + Sync + 'static) {
    if ON_CHANGE.set(Box::new(on_change)).is_err() {
        return;
    }
    std::thread::spawn(|| unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(instance) => instance,
            Err(e) => return eprintln!("clipboard listener: no module handle: {e}"),
        };
        let class = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: w!("FunkeClipboardListener"),
            ..Default::default()
        };
        RegisterClassW(&class);

        // HWND_MESSAGE: invisible, never enumerated, exists only to receive messages.
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("FunkeClipboardListener"),
            PCWSTR::null(),
            WINDOW_STYLE(0),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance.into()),
            None,
        );
        let window = match window {
            Ok(window) => window,
            Err(e) => return eprintln!("clipboard listener: no window: {e}"),
        };
        if let Err(e) = AddClipboardFormatListener(window) {
            return eprintln!("clipboard listener: not registered: {e}");
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
}

unsafe extern "system" fn wndproc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        if let Some(on_change) = ON_CHANGE.get() {
            on_change();
        }
        return LRESULT(0);
    }
    DefWindowProcW(window, msg, wparam, lparam)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Read, tolerating the fact that the clipboard is a *shared global*: every other
    /// clipboard monitor on the machine — a running Funke included — wakes on the same
    /// notification our write just raised and grabs the lock at the same instant. A single
    /// look can lose that race, which is why [`ClipboardHistory::capture`] retries too.
    fn read_settled() -> Read {
        for _ in 0..10 {
            match read() {
                Read::Busy | Read::NotText => std::thread::sleep(std::time::Duration::from_millis(30)),
                settled => return settled,
            }
        }
        read()
    }

    /// The whole point of the module, exercised against the real Win32 clipboard: an
    /// ordinary copy comes back, and a *secret* copy is invisible to anything that reads
    /// the clipboard the way a monitor does — which is exactly how the vault's passwords
    /// stay out of the history, out of Win+V, and off the cloud clipboard.
    ///
    /// One test, not three: the clipboard is a single global resource, and separate
    /// #[test]s would race each other in the parallel harness.
    #[test]
    fn secrets_are_written_but_hidden_from_monitors_while_ordinary_text_round_trips() {
        // A headless or locked session has no clipboard at all — don't fail the suite over
        // it. Probing with a write, not a bare open: holding the lock only to release it
        // again is what invites the contention this test then has to fight.
        if write_text("funke round trip").is_err() {
            eprintln!("no clipboard available in this session — skipping");
            return;
        }
        assert_eq!(read_settled(), Read::Text("funke round trip".into()));

        // Unicode must survive the UTF-16 trip intact.
        write_text("Grüße 🎉").expect("unicode is written");
        assert_eq!(read_settled(), Read::Text("Grüße 🎉".into()));

        write_secret("hunter2").expect("the secret is written");
        assert_eq!(
            read_settled(),
            Read::Excluded,
            "a secret carries the exclusion markers, so a clipboard monitor must not see it"
        );
        assert_eq!(read_text(), None, "…and it is not offered as text either");

        // It really is on the clipboard for the user to paste — it is only *monitors* that
        // are shut out. Reading past our own guard proves the text is still there.
        let lock = ClipboardLock::acquire().expect("clipboard");
        assert_eq!(read_unicode_text().as_deref(), Some("hunter2"));
        drop(lock);
    }

    // [`Read::Busy`] has no unit test on purpose: it arises only from *cross-process*
    // contention, and there is no way to provoke that from inside one. (Holding the lock
    // on another thread proves nothing — `OpenClipboard(NULL)` from a second thread of the
    // process that already owns it succeeds.) It is not hypothetical: the test above
    // failed exactly this way the first time it ran while a Funke instance was up, whose
    // listener wakes on the same notification and grabs the clipboard at the same instant.
    // That is what the retry budget and the `Busy` arm in `ClipboardHistory::capture` are
    // for — a lost race must cost a moment's wait, never the clip.
}
