//! KeePass-style autotype via `SendInput` (M4). Hand-written FFI like `focus.rs` —
//! the INPUT struct layout is the only fiddly part: a 4-byte tag + the union of
//! MOUSEINPUT (largest, 32 bytes on x64) / KEYBDINPUT, so 40 bytes total.
//!
//! Characters go in as `KEYEVENTF_UNICODE` scan codes (one down+up per UTF-16 unit,
//! surrogate pairs included), so layout and IME state can't mangle passwords.
//!
//! [`run`] executes the entry's autotype sequence (`funke_vault::sequence`): the steps
//! name the fields, the secrets are supplied here and never leave this call.

use funke_vault::{Credentials, Step};

#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
union InputUnion {
    ki: KeybdInput,
    mi: MouseInput,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    u: InputUnion,
}

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_UNICODE: u32 = 0x0004;

const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;

/// How long a `{TAB}` is given to actually move the focus before the next characters go
/// out. `SendInput` returns as soon as the events are queued, and the focus change they
/// trigger is the *target's* work, not ours — a browser-engine app (CEF, Electron) hands
/// it to its renderer asynchronously, so characters sent immediately behind a Tab land in
/// the field the Tab was meant to leave. That is a password typed into a username box.
///
/// A human's inter-key gap, roughly. There is no signal to poll instead: the focused
/// *element* inside another process's renderer is invisible to us — the apps this exists
/// for are precisely the ones that expose no accessibility tree at all.
///
/// ponytail: fixed delay, the only lever available; `{DELAY=n}` in an entry's own sequence
/// overrides it for a target that needs longer.
const TAB_SETTLE: std::time::Duration = std::time::Duration::from_millis(80);

#[link(name = "user32")]
extern "system" {
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
}

fn key(vk: u16, scan: u16, flags: u32) -> Input {
    Input {
        kind: INPUT_KEYBOARD,
        u: InputUnion {
            ki: KeybdInput {
                vk,
                scan,
                flags,
                time: 0,
                extra: 0,
            },
        },
    }
}

fn send(inputs: &[Input]) {
    if !inputs.is_empty() {
        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<Input>() as i32,
            )
        };
    }
}

/// Type an entry's sequence into whatever window has focus (the window the overlay was
/// summoned from, refocused by the caller). Field steps the item can't fill — no
/// username, no TOTP — are skipped rather than typed as empty text.
pub fn run(steps: &[Step], credentials: &Credentials, totp: Option<&str>) {
    for step in steps {
        match step {
            Step::Text(text) => type_text(text),
            Step::Username => {
                if let Some(username) = credentials.username.as_deref() {
                    type_text(username);
                }
            }
            Step::Password => {
                if let Some(password) = credentials.password.as_deref() {
                    type_text(password);
                }
            }
            Step::Totp => {
                if let Some(totp) = totp {
                    type_text(totp);
                }
            }
            Step::Tab => {
                press(VK_TAB);
                std::thread::sleep(TAB_SETTLE);
            }
            Step::Enter => press(VK_RETURN),
            Step::Delay(ms) => std::thread::sleep(std::time::Duration::from_millis(*ms)),
        }
    }
}

/// Type literal text into whatever window has focus.
fn type_text(text: &str) {
    let mut inputs = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        inputs.push(key(0, unit, KEYEVENTF_UNICODE));
        inputs.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    send(&inputs);
}

/// Press and release a virtual key (Tab between fields, Enter to submit).
fn press(vk: u16) {
    send(&[key(vk, 0, 0), key(vk, 0, KEYEVENTF_KEYUP)]);
}

/// Walk the caret back `count` characters — how a snippet's `{CURSOR}` is honoured after
/// the text has landed. A no-op for the usual case of no marker (`count` 0).
pub fn caret_left(count: usize) {
    const VK_LEFT: u16 = 0x25;
    let mut inputs = Vec::with_capacity(count * 2);
    for _ in 0..count {
        inputs.push(key(VK_LEFT, 0, 0));
        inputs.push(key(VK_LEFT, 0, KEYEVENTF_KEYUP));
    }
    send(&inputs);
}

/// Send Ctrl+V to the focused window — how a clipboard clip gets pasted.
///
/// Deliberately *not* [`type_text`]: a clip is arbitrary text, and typing it means
/// sending every newline in it as an Enter keypress. In a chat window or a form that
/// submits the half-pasted message; Ctrl+V inserts the text as text. The caller has
/// already put the clip on the clipboard.
pub fn paste() {
    const VK_CONTROL: u16 = 0x11;
    const VK_V: u16 = 0x56;
    send(&[
        key(VK_CONTROL, 0, 0),
        key(VK_V, 0, 0),
        key(VK_V, 0, KEYEVENTF_KEYUP),
        key(VK_CONTROL, 0, KEYEVENTF_KEYUP),
    ]);
}
