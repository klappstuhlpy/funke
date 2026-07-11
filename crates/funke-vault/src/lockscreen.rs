//! Detect whether the Windows session is locked, so the vault watchdog can lock the
//! vault the moment the screen locks. Hand-written FFI (like the app's `focus.rs`) — the
//! surface is tiny and self-contained, not worth pulling more `windows` features in.
//!
//! Heuristic (the one Chromium and friends use): try to open the *input* desktop. When the
//! workstation is locked, the input desktop is the secure "Winlogon" desktop that an
//! ordinary-privilege process cannot open (`OpenInputDesktop` returns NULL); a screensaver
//! lock switches to a desktop named other than "Default". Either way, "not the Default
//! desktop" means locked.

use std::os::raw::c_void;

type Hdesk = *mut c_void;

const UOI_NAME: i32 = 2;
const DESKTOP_READOBJECTS: u32 = 0x0001;

#[link(name = "user32")]
extern "system" {
    fn OpenInputDesktop(flags: u32, inherit: i32, desired_access: u32) -> Hdesk;
    fn CloseDesktop(desktop: Hdesk) -> i32;
    fn GetUserObjectInformationW(obj: Hdesk, index: i32, info: *mut c_void, len: u32, needed: *mut u32) -> i32;
}

/// True when the workstation is locked (or on the secure/screensaver desktop).
pub fn workstation_locked() -> bool {
    unsafe {
        let desktop = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if desktop.is_null() {
            // Can't open the input desktop → it's the secure Winlogon desktop → locked.
            return true;
        }
        let mut buf = [0u16; 256];
        let mut needed = 0u32;
        let ok = GetUserObjectInformationW(
            desktop,
            UOI_NAME,
            buf.as_mut_ptr() as *mut c_void,
            (buf.len() * 2) as u32,
            &mut needed,
        );
        let locked = if ok == 0 {
            true // couldn't read the name — treat conservatively as locked
        } else {
            let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            !String::from_utf16_lossy(&buf[..len]).eq_ignore_ascii_case("Default")
        };
        CloseDesktop(desktop);
        locked
    }
}
