//! Everything's IPC: a `WM_COPYDATA` conversation with the `EVERYTHING_TASKBAR_NOTIFICATION`
//! window that voidtools' Everything keeps open while it runs.
//!
//! Deliberately **no SDK, no DLL**. `Everything64.dll` would have to be vendored and shipped
//! next to the binary, and its license is not ours to re-license; the IPC it wraps is a
//! documented message protocol we can speak ourselves in safe-ish Rust. It also means the
//! integration costs nothing when Everything is absent: no DLL to fail to load, just a
//! `FindWindowW` that comes back empty.
//!
//! The protocol, in the two structs that matter (all fields `DWORD`, little-endian):
//!
//! ```text
//! query  (WM_COPYDATA, dwData = 2)   reply (WM_COPYDATA, dwData = REPLY_ID)
//!   reply_hwnd                         totfolders, totfiles, totitems
//!   reply_copydata_message             numfolders, numfiles, numitems
//!   search_flags                       offset
//!   offset                             items[numitems]: { flags, name_offset, path_offset }
//!   max_results                        …then the strings, null-terminated UTF-16, at
//!   search string, null-terminated     byte offsets counted from the start of the reply
//! ```
//!
//! An item's *name* and *path* come back separately (Everything stores them that way), so a
//! hit is the two joined — and a folder is flagged, not inferred from the name.

use std::ffi::c_void;
use std::time::{Duration, Instant};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, FindWindowW, PeekMessageW, RegisterClassW, SendMessageW,
    TranslateMessage, HWND_MESSAGE, MSG, PM_REMOVE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_COPYDATA, WNDCLASSW,
};

/// The window Everything keeps open for exactly this purpose. Its absence *is* the
/// feature detection: no window, no Everything, and we quietly stay out of the way.
const EVERYTHING_CLASS: PCWSTR = w!("EVERYTHING_TASKBAR_NOTIFICATION");
/// `EVERYTHING_IPC_COPYDATAQUERYW` — "here is a query, in UTF-16".
const QUERY_ID: usize = 2;
/// Ours to choose: Everything echoes it back as the reply's `dwData`, which is how the
/// window proc knows a `WM_COPYDATA` is the answer to our question and not someone else's.
const REPLY_ID: usize = 0;

/// How long we wait for Everything to answer. It answers in single-digit milliseconds off
/// its in-memory index; this is the ceiling that keeps a wedged Everything from ever
/// costing a keystroke more than a blink (the plugin host's timeout, for the same reason).
const REPLY_TIMEOUT: Duration = Duration::from_millis(250);

const HEADER_BYTES: usize = 28; // 7 DWORDs
const ITEM_BYTES: usize = 12; // flags + name offset + path offset
/// `EVERYTHING_IPC_FOLDER` — the item is a directory.
const FLAG_FOLDER: u32 = 1;

/// One thing Everything found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// The file name alone (`report.xlsx`) — what the fuzzy scorer ranks against.
    pub name: String,
    /// The full path, name included.
    pub path: String,
    pub is_dir: bool,
}

/// Is Everything running right now? Cheap enough to ask per keystroke, and it must be
/// asked that often: the user can quit Everything while Funke stays up.
pub fn is_running() -> bool {
    everything_window().is_some()
}

fn everything_window() -> Option<HWND> {
    unsafe { FindWindowW(EVERYTHING_CLASS, None) }
        .ok()
        .filter(|w| !w.is_invalid())
}

/// The receiving end of the conversation, owned by the worker thread (see [`crate::Everything`]).
/// A message-only window: invisible, never enumerated, exists only to be sent an answer.
pub struct Receiver {
    window: HWND,
}

impl Receiver {
    pub fn create() -> Option<Self> {
        unsafe {
            let instance = GetModuleHandleW(None).ok()?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: w!("FunkeEverythingReply"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let window = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("FunkeEverythingReply"),
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
            )
            .ok()?;
            Some(Self { window })
        }
    }

    /// Ask Everything, and wait for the answer. Empty when Everything isn't running, said
    /// no, or didn't answer in time — all of which mean the same thing to a caller: no rows.
    pub fn query(&self, search: &str, max_results: u32) -> Vec<Hit> {
        let Some(target) = everything_window() else {
            return Vec::new();
        };
        take_reply(); // A late answer to a query we gave up on must not be read as this one's.

        let payload = query_payload(self.window, search, max_results);
        let data = COPYDATASTRUCT {
            dwData: QUERY_ID,
            cbData: payload.len() as u32,
            lpData: payload.as_ptr() as *mut c_void,
        };
        // Everything returns TRUE if it took the query. The reply is a *sent* message, so it
        // can land inside this very call — a thread blocked in SendMessage still receives
        // them — which is why the slot is checked before the pump loop rather than after.
        let accepted = unsafe {
            SendMessageW(
                target,
                WM_COPYDATA,
                Some(WPARAM(self.window.0 as usize)),
                Some(LPARAM(&data as *const _ as isize)),
            )
        };
        if accepted.0 == 0 {
            return Vec::new();
        }

        let deadline = Instant::now() + REPLY_TIMEOUT;
        loop {
            if let Some(bytes) = take_reply() {
                return parse_reply(&bytes);
            }
            if Instant::now() >= deadline {
                return Vec::new();
            }
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, Some(self.window), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

// The window and its proc live on the worker thread and nowhere else, so the reply can be
// handed over in a thread-local — no lock, and no way for two queries to cross.
thread_local! {
    static REPLY: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn take_reply() -> Option<Vec<u8>> {
    REPLY.with(|reply| reply.borrow_mut().take())
}

unsafe extern "system" fn wndproc(window: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA {
        let data = lparam.0 as *const COPYDATASTRUCT;
        if !data.is_null() && (*data).dwData == REPLY_ID {
            let data = &*data;
            if !data.lpData.is_null() {
                let bytes = std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize).to_vec();
                REPLY.with(|reply| *reply.borrow_mut() = Some(bytes));
            }
            return LRESULT(1);
        }
    }
    DefWindowProcW(window, msg, wparam, lparam)
}

/// The query struct, laid out by hand — five `DWORD`s and a null-terminated UTF-16 string.
///
/// `reply_hwnd` really is a 32-bit field in a 64-bit protocol: Everything has always
/// declared it `DWORD`, and Windows guarantees an HWND fits in 32 bits.
fn query_payload(reply_to: HWND, search: &str, max_results: u32) -> Vec<u8> {
    let mut payload = Vec::with_capacity(20 + (search.len() + 1) * 2);
    payload.extend_from_slice(&(reply_to.0 as usize as u32).to_le_bytes());
    payload.extend_from_slice(&(REPLY_ID as u32).to_le_bytes());
    payload.extend_from_slice(&0u32.to_le_bytes()); // search flags: no case/whole-word/regex
    payload.extend_from_slice(&0u32.to_le_bytes()); // offset: from the first result
    payload.extend_from_slice(&max_results.to_le_bytes());
    for unit in search.encode_utf16().chain(std::iter::once(0)) {
        payload.extend_from_slice(&unit.to_le_bytes());
    }
    payload
}

/// Read the reply buffer. Everything is trusted, but this parses a raw byte blob from
/// another process: every offset is bounds-checked and a malformed item is skipped rather
/// than panicked over.
fn parse_reply(bytes: &[u8]) -> Vec<Hit> {
    let Some(count) = dword(bytes, 20) else {
        return Vec::new(); // numitems
    };
    let count = count as usize;
    let mut hits = Vec::with_capacity(count.min(1024));
    for index in 0..count {
        let item = HEADER_BYTES + index * ITEM_BYTES;
        let (Some(flags), Some(name_at), Some(path_at)) =
            (dword(bytes, item), dword(bytes, item + 4), dword(bytes, item + 8))
        else {
            break; // Truncated item array: nothing sane left to read.
        };
        let (Some(name), Some(parent)) = (wide_at(bytes, name_at as usize), wide_at(bytes, path_at as usize)) else {
            continue;
        };
        let path = if parent.is_empty() {
            name.clone() // A drive root has no parent to join to.
        } else {
            format!("{}\\{}", parent.trim_end_matches('\\'), name)
        };
        hits.push(Hit {
            name,
            path,
            is_dir: flags & FLAG_FOLDER != 0,
        });
    }
    hits
}

fn dword(bytes: &[u8], at: usize) -> Option<u32> {
    let field: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(u32::from_le_bytes(field))
}

/// A null-terminated UTF-16 string at a byte offset into the reply.
fn wide_at(bytes: &[u8], at: usize) -> Option<String> {
    let tail = bytes.get(at..)?;
    let units: Vec<u16> = tail
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();
    // No terminator before the buffer ran out — the blob is malformed, not merely empty.
    if units.len() * 2 >= tail.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a reply the way Everything does, so the parser is tested against the layout it
    /// claims to understand: header, item table, then the strings the items point into.
    fn reply(items: &[(u32, &str, &str)]) -> Vec<u8> {
        let mut head = Vec::new();
        let count = items.len() as u32;
        for field in [0, 0, count, 0, 0, count, 0] {
            head.extend_from_slice(&u32::to_le_bytes(field));
        }
        let strings_at = HEADER_BYTES + items.len() * ITEM_BYTES;
        let mut table = Vec::new();
        let mut strings = Vec::new();
        for (flags, name, path) in items {
            table.extend_from_slice(&flags.to_le_bytes());
            table.extend_from_slice(&((strings_at + strings.len()) as u32).to_le_bytes());
            for unit in name.encode_utf16().chain(std::iter::once(0)) {
                strings.extend_from_slice(&unit.to_le_bytes());
            }
            table.extend_from_slice(&((strings_at + strings.len()) as u32).to_le_bytes());
            for unit in path.encode_utf16().chain(std::iter::once(0)) {
                strings.extend_from_slice(&unit.to_le_bytes());
            }
        }
        head.extend_from_slice(&table);
        head.extend_from_slice(&strings);
        head
    }

    #[test]
    fn a_reply_becomes_hits_with_the_name_joined_onto_its_path() {
        let hits = reply(&[
            (0, "report.xlsx", r"C:\Users\me\Documents"),
            (FLAG_FOLDER, "Projects", r"C:\Users\me"),
        ]);
        let hits = parse_reply(&hits);
        assert_eq!(
            hits,
            vec![
                Hit {
                    name: "report.xlsx".into(),
                    path: r"C:\Users\me\Documents\report.xlsx".into(),
                    is_dir: false,
                },
                Hit {
                    name: "Projects".into(),
                    path: r"C:\Users\me\Projects".into(),
                    is_dir: true,
                },
            ]
        );
    }

    #[test]
    fn a_trailing_separator_on_the_parent_is_not_doubled() {
        let hits = parse_reply(&reply(&[(0, "boot.ini", r"C:\")]));
        assert_eq!(hits[0].path, r"C:\boot.ini");
    }

    #[test]
    fn non_ascii_names_survive_the_utf16_round_trip() {
        let hits = parse_reply(&reply(&[(0, "Angebot Grüße.pdf", r"C:\tmp")]));
        assert_eq!(hits[0].name, "Angebot Grüße.pdf");
    }

    /// The buffer comes from another process: a truncated or lying one must yield no rows,
    /// never a panic.
    #[test]
    fn a_malformed_reply_yields_nothing_rather_than_panicking() {
        assert!(parse_reply(&[]).is_empty());
        assert!(parse_reply(&[0; 8]).is_empty());

        // Header claims two items but the table is cut short.
        let mut truncated = Vec::new();
        for field in [0u32, 0, 2, 0, 0, 2, 0] {
            truncated.extend_from_slice(&field.to_le_bytes());
        }
        truncated.extend_from_slice(&[0; 6]);
        assert!(parse_reply(&truncated).is_empty());

        // An item pointing at a string that runs off the end of the buffer.
        let mut runaway = Vec::new();
        for field in [0u32, 0, 1, 0, 0, 1, 0] {
            runaway.extend_from_slice(&field.to_le_bytes());
        }
        runaway.extend_from_slice(&0u32.to_le_bytes()); // flags
        runaway.extend_from_slice(&9999u32.to_le_bytes()); // name: nowhere
        runaway.extend_from_slice(&9999u32.to_le_bytes()); // path: nowhere
        assert!(parse_reply(&runaway).is_empty());
    }

    #[test]
    fn the_query_payload_is_five_dwords_then_the_search_in_utf16() {
        let payload = query_payload(HWND(0x1234 as *mut _), "hi", 25);
        assert_eq!(dword(&payload, 0), Some(0x1234), "reply window");
        assert_eq!(dword(&payload, 4), Some(REPLY_ID as u32), "reply message id");
        assert_eq!(dword(&payload, 8), Some(0), "search flags");
        assert_eq!(dword(&payload, 12), Some(0), "offset");
        assert_eq!(dword(&payload, 16), Some(25), "max results");
        assert_eq!(wide_at(&payload, 20).as_deref(), Some("hi"));
    }
}
