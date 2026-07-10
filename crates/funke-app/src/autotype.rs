//! KeePass-style autotype via `SendInput` (M4). Hand-written FFI like `focus.rs` —
//! the INPUT struct layout is the only fiddly part: a 4-byte tag + the union of
//! MOUSEINPUT (largest, 32 bytes on x64) / KEYBDINPUT, so 40 bytes total.
//!
//! Characters go in as `KEYEVENTF_UNICODE` scan codes (one down+up per UTF-16 unit,
//! surrogate pairs included), so layout and IME state can't mangle passwords.

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

pub const VK_TAB: u16 = 0x09;
pub const VK_RETURN: u16 = 0x0D;

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

/// Type literal text into whatever window has focus.
pub fn type_text(text: &str) {
    let mut inputs = Vec::with_capacity(text.len() * 2);
    for unit in text.encode_utf16() {
        inputs.push(key(0, unit, KEYEVENTF_UNICODE));
        inputs.push(key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
    }
    send(&inputs);
}

/// Press and release a virtual key (Tab between fields, Enter to submit).
pub fn press(vk: u16) {
    send(&[key(vk, 0, 0), key(vk, 0, KEYEVENTF_KEYUP)]);
}
