//! Reading the focused browser's address bar via UI Automation (DESIGN.md §5).
//!
//! This is how a credential suggestion knows *which site* you are on rather than just
//! "a browser is open". UIA is the same public accessibility surface password managers
//! and screen readers use: ask the window for its Document element and read the URL out
//! of its ValuePattern; fall back to the address-bar Edit control when a browser doesn't
//! expose the document (or exposes it without a value).
//!
//! Deliberately read-only and slow-path: the tree walk can take tens of milliseconds, so
//! the launcher runs it on a background thread after the overlay is already up — it must
//! never sit between the hotkey and the window.
//!
//! COM is initialized lazily per calling thread, MTA: a UIA call from an STA thread that
//! doesn't pump messages can deadlock.

use std::mem::ManuallyDrop;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_ValuePatternId,
};

/// Executable stems (lowercase, no `.exe`) whose foreground window is a web browser, so
/// the URL — not the process name — says what credential belongs to it. Chromium forks
/// and Firefox forks alike; anything unlisted is treated as an ordinary app.
const BROWSERS: &[&str] = &[
    "chrome",
    "msedge",
    "firefox",
    "brave",
    "vivaldi",
    "opera",
    "opera_gx",
    "arc",
    "zen",
    "librewolf",
    "waterfox",
    "floorp",
    "thorium",
    "chromium",
    "iexplore",
];

thread_local! {
    static COM_INIT: () = {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    };
}

/// Is this process a web browser (so its address bar, not its name, identifies the site)?
pub fn is_browser_process(stem: &str) -> bool {
    BROWSERS.contains(&stem.to_ascii_lowercase().trim_end_matches(".exe"))
}

/// The URL currently shown in a browser window, or `None` when it can't be read — an
/// unsupported browser, accessibility unavailable, or simply a page with no address
/// (the caller then falls back to the window title and process name).
pub fn browser_url(hwnd: isize) -> Option<String> {
    COM_INIT.with(|()| ());
    unsafe {
        let automation: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let window = automation
            .ElementFromHandle(HWND(hwnd as *mut core::ffi::c_void))
            .ok()?;

        // The document element carries the page's URL in every major browser. The
        // address bar (an Edit) is the fallback: it holds what the user *sees*, which
        // for a focused-but-unedited bar is the same URL.
        for control_type in [UIA_DocumentControlTypeId.0, UIA_EditControlTypeId.0] {
            let condition = automation
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(control_type))
                .ok()?;
            if let Ok(element) = window.FindFirst(TreeScope_Descendants, &condition) {
                if let Some(url) = value_of(&element).filter(|value| looks_like_url(value)) {
                    return Some(url);
                }
            }
        }
        None
    }
}

/// A `VT_I4` VARIANT — what `CreatePropertyCondition` wants for a control-type id. The
/// raw Win32 VARIANT is a bare union, so it gets filled by hand; an integer variant owns
/// nothing, so there is nothing to `VariantClear`.
fn int_variant(value: i32) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: value },
            }),
        },
    }
}

/// The element's ValuePattern text, if it exposes one.
unsafe fn value_of(element: &IUIAutomationElement) -> Option<String> {
    let pattern: IUIAutomationValuePattern = element.GetCurrentPatternAs(UIA_ValuePatternId).ok()?;
    let value = pattern.CurrentValue().ok()?.to_string();
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// Cheap sanity gate so a half-typed search term in the address bar (or some unrelated
/// Edit control that answered first) never poses as a URL.
fn looks_like_url(value: &str) -> bool {
    if value.contains(char::is_whitespace) {
        return false;
    }
    value.starts_with("http://") || value.starts_with("https://") || (value.contains('.') && !value.contains('@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_are_recognized_by_stem_with_or_without_the_extension() {
        assert!(is_browser_process("chrome"));
        assert!(is_browser_process("MsEdge.exe"));
        assert!(!is_browser_process("discord"));
        assert!(!is_browser_process(""));
    }

    #[test]
    fn only_url_shaped_values_are_accepted() {
        assert!(looks_like_url("https://github.com/login"));
        assert!(looks_like_url("github.com"));
        assert!(!looks_like_url("how to reset a password"), "a search term is not a URL");
        assert!(
            !looks_like_url("ben@example.com"),
            "an email in some form field is not a URL"
        );
        assert!(!looks_like_url("Untitled"));
    }
}
