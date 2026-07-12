//! Where a secret may be typed: UI Automation inspection of an autotype target.
//!
//! Autotype is `SendInput` — the keystrokes go wherever focus happens to be, and the
//! target window has no say in it. That is fine on a login form and a catastrophe in a
//! chat box: a password typed into Discord's message bar, followed by the sequence's
//! trailing Enter, is a password published to a channel. So before a secret is typed the
//! target is asked two questions, through the same public accessibility surface `uia.rs`
//! reads address bars from:
//!
//! 1. Does this window show a **login form** at all — a password field UIA can see
//!    ([`has_password_field`])? A chat window, the desktop, a game, an editor do not, and
//!    none of them is a place a password belongs.
//! 2. Is the caret **in a field** ([`prepare`])? A browser parked on a login page with
//!    focus on the page body would otherwise swallow the username and fire the Enter at
//!    whatever happens to listen. When it isn't, the login form's own field is focused
//!    first — which is also how "open website & autofill" aims at a freshly loaded page.
//!
//! Both answers are *advisory*, and the host treats them as such. A window UIA cannot
//! read — a game, an RDP session, a terminal — looks exactly like a window with nothing
//! to type into, so a refusal is offered to the user as "type anyway", never enforced as
//! proof of danger.
//!
//! Cost: a descendant search over a large page runs into the tens of milliseconds, so
//! every call here belongs on the action path (a background thread), never on the
//! keystroke path. COM is initialized per calling thread, MTA — see [`crate::uia`].

use std::ffi::c_void;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    IUIAutomation, IUIAutomationElement, IUIAutomationElementArray, IUIAutomationInvokePattern,
    IUIAutomationValuePattern, TreeScope_Descendants, UIA_ButtonControlTypeId, UIA_ComboBoxControlTypeId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_HasKeyboardFocusPropertyId,
    UIA_HyperlinkControlTypeId, UIA_InvokePatternId, UIA_IsKeyboardFocusablePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsPasswordPropertyId, UIA_ValuePatternId, UIA_WindowControlTypeId,
};

use crate::uia::{automation, bool_variant, int_variant};

/// How far up from the password field we look for its form's username field. Enough to
/// climb out of a `<div>` nest, bounded so the walk can't wander into the browser's own
/// chrome (the ascent stops at the document element regardless — see [`username_field`]).
const MAX_FORM_ANCESTORS: usize = 6;

/// What [`prepare`] managed to aim at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ready {
    /// An editable field has the keyboard focus (the user's caret, or the form's username
    /// field, which we focused). Type the sequence as written.
    Sequence,
    /// Only the password field could be focused — the form has no username field, or none
    /// we could reach. The caller must drop the steps that precede `{PASSWORD}`, or the
    /// username lands in the password box.
    PasswordOnly,
    /// Nothing to type into: no login form, or no field we could put the caret in (which
    /// includes "the window never came to the foreground", where `SendInput` would type
    /// into someone else entirely).
    Blocked,
}

/// Does this window show a login form — a password field UI Automation can see?
///
/// Asked *before* the overlay hides, so the common refusal (a chat window, the desktop)
/// costs no flicker. Offscreen fields don't count: a site's hidden password input, or the
/// login form of a tab that isn't the one in front, must not vouch for what *is* in front.
pub fn has_password_field(hwnd: isize) -> bool {
    unsafe {
        let Some((automation, window)) = window_element(hwnd) else {
            return false;
        };
        password_field(&automation, &window).is_some()
    }
}

/// Put the caret where a credential may go, and say what may be typed there.
///
/// Called after the target window has been brought back to the foreground: the focus we
/// find (or set) is the focus `SendInput` will type into. A window with no login form is
/// [`Ready::Blocked`] here too — this is the guard, `prepare` is simply where it lands
/// after the window has been refocused.
pub fn prepare(hwnd: isize) -> Ready {
    unsafe {
        let Some((automation, window)) = window_element(hwnd) else {
            return Ready::Blocked;
        };
        let Some(password) = password_field(&automation, &window) else {
            return Ready::Blocked;
        };

        // The caret is already in a field of this window: type where the user put it.
        // (Focus *inside the window's subtree* — a dialog or notification that stole the
        // foreground has no descendant here, and is refused rather than typed into.)
        if focused_element(&automation, &window).is_some_and(|element| editable(&element)) {
            return Ready::Sequence;
        }

        // Focus is on the page body, a button, nothing at all: aim at the form ourselves.
        if let Some(username) = username_field(&automation, &password) {
            if username.SetFocus().is_ok() {
                return Ready::Sequence;
            }
        }
        if password.SetFocus().is_ok() {
            return Ready::PasswordOnly;
        }
        Ready::Blocked
    }
}

/// Click the page's own "Sign in" — for the entry whose saved URI is a homepage.
///
/// The alternative was guessing a URL (`https://github.com` + `/login`) or, worse, looking
/// one up in a search engine: a page chosen by SEO is a page an attacker can choose for
/// you, and it would be autofilled. So nothing is guessed. The site is asked what its own
/// sign-in affordance is, through the accessibility tree, and that link is invoked.
///
/// Fenced three ways, because this is the one place Funke acts *on* a page:
/// 1. **Inside the document only.** A browser's own chrome has a "Sign in" button (Edge's
///    profile, Chrome's sync) — clicking that would open the browser vendor's login.
/// 2. **Exact names only** ([`is_sign_in_label`]). `"Sign in with Google"` *contains*
///    "sign in", and clicking it hands the session to a third-party IdP the entry never
///    named. Containment is not good enough here; equality is.
/// 3. It clicks, it does not type. Whether a secret follows is still decided afterwards by
///    the address bar (the host must still be the entry's) and the login-form guard.
///
/// `true` when something was invoked — the caller keeps waiting for the form either way.
pub fn click_sign_in(hwnd: isize) -> bool {
    unsafe {
        let Some((automation, window)) = window_element(hwnd) else {
            return false;
        };
        // Browsers only: the document *is* the fence. A native window without one gets
        // nothing clicked, which is the safe default for a feature about web logins.
        let Some(document) = document_element(&automation, &window) else {
            return false;
        };
        let Some(candidates) = clickable(&automation, &document) else {
            return false;
        };
        for index in 0..candidates.Length().unwrap_or(0) {
            let Ok(element) = candidates.GetElement(index) else {
                continue;
            };
            let name = element.CurrentName().map(|name| name.to_string()).unwrap_or_default();
            if !is_sign_in_label(&name) {
                continue;
            }
            if let Ok(invoke) = element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) {
                if invoke.Invoke().is_ok() {
                    return true;
                }
            }
        }
        false
    }
}

/// Is this the site's *own* sign-in link, and nothing else?
///
/// Equality against a small vocabulary, after normalizing away case, punctuation and the
/// decorations links wear ("Log in →", "SIGN IN"). Deliberately **not** containment: the
/// names it must refuse — "Sign in with Google", "Sign in with Apple", "Can't log in?" —
/// all contain the words, and all lead somewhere the vault entry never named.
fn is_sign_in_label(name: &str) -> bool {
    const LABELS: &[&str] = &[
        "log in",
        "login",
        "sign in",
        "signin",
        "log on",
        "logon",
        "anmelden",
        "einloggen",
        "jetzt anmelden",
    ];
    let normalized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    LABELS.contains(&normalized.as_str())
}

/// The page inside a browser window (`None` in a native app — it has no document).
unsafe fn document_element(automation: &IUIAutomation, window: &IUIAutomationElement) -> Option<IUIAutomationElement> {
    let is_document = automation
        .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(UIA_DocumentControlTypeId.0))
        .ok()?;
    window.FindFirst(TreeScope_Descendants, &is_document).ok()
}

/// Every visible link and button in the page — the pool [`click_sign_in`] picks from.
unsafe fn clickable(automation: &IUIAutomation, document: &IUIAutomationElement) -> Option<IUIAutomationElementArray> {
    let link = automation
        .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(UIA_HyperlinkControlTypeId.0))
        .ok()?;
    let button = automation
        .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(UIA_ButtonControlTypeId.0))
        .ok()?;
    let on_screen = automation
        .CreatePropertyCondition(UIA_IsOffscreenPropertyId, &bool_variant(false))
        .ok()?;
    let either = automation.CreateOrCondition(&link, &button).ok()?;
    let visible = automation.CreateAndCondition(&either, &on_screen).ok()?;
    document.FindAll(TreeScope_Descendants, &visible).ok()
}

/// The UIA element for a top-level window, plus the automation instance it came from
/// (elements and conditions must not be mixed across instances).
unsafe fn window_element(hwnd: isize) -> Option<(IUIAutomation, IUIAutomationElement)> {
    let automation = automation()?;
    let window = automation.ElementFromHandle(HWND(hwnd as *mut c_void)).ok()?;
    Some((automation, window))
}

/// The window's first visible password field — the whole guard, in one FindFirst.
unsafe fn password_field(automation: &IUIAutomation, window: &IUIAutomationElement) -> Option<IUIAutomationElement> {
    let is_password = automation
        .CreatePropertyCondition(UIA_IsPasswordPropertyId, &bool_variant(true))
        .ok()?;
    let on_screen = automation
        .CreatePropertyCondition(UIA_IsOffscreenPropertyId, &bool_variant(false))
        .ok()?;
    let visible_password = automation.CreateAndCondition(&is_password, &on_screen).ok()?;
    window.FindFirst(TreeScope_Descendants, &visible_password).ok()
}

/// The username field belonging to `password`'s form: the first focusable, non-password
/// Edit in the smallest ancestor that contains one.
///
/// The ascent stops at the document (or the window, in a native app) on purpose. One
/// level further up in a browser sits the **address bar** — an Edit, focusable, not a
/// password — and typing a username into it and pressing Enter would navigate away with
/// the username in the URL. A password-only page (the second step of an email-first
/// login) legitimately has no username field, and gets `None`.
unsafe fn username_field(automation: &IUIAutomation, password: &IUIAutomationElement) -> Option<IUIAutomationElement> {
    let text_field = automation
        .CreatePropertyCondition(UIA_ControlTypePropertyId, &int_variant(UIA_EditControlTypeId.0))
        .ok()?;
    let not_password = automation
        .CreatePropertyCondition(UIA_IsPasswordPropertyId, &bool_variant(false))
        .ok()?;
    let focusable = automation
        .CreatePropertyCondition(UIA_IsKeyboardFocusablePropertyId, &bool_variant(true))
        .ok()?;
    let on_screen = automation
        .CreatePropertyCondition(UIA_IsOffscreenPropertyId, &bool_variant(false))
        .ok()?;
    let editable = automation.CreateAndCondition(&text_field, &not_password).ok()?;
    let reachable = automation.CreateAndCondition(&focusable, &on_screen).ok()?;
    let candidate = automation.CreateAndCondition(&editable, &reachable).ok()?;

    let walker = automation.ControlViewWalker().ok()?;
    let mut scope = walker.GetParentElement(password).ok()?;
    for _ in 0..MAX_FORM_ANCESTORS {
        if let Ok(field) = scope.FindFirst(TreeScope_Descendants, &candidate) {
            return Some(field);
        }
        if is_boundary(&scope) {
            return None;
        }
        scope = walker.GetParentElement(&scope).ok()?;
    }
    None
}

/// The element that holds the keyboard focus *within* this window, if any. Nothing focused
/// means the window never made it to the foreground — which is a refusal, not a detail:
/// `SendInput` would type into whoever holds the foreground instead.
unsafe fn focused_element(automation: &IUIAutomation, window: &IUIAutomationElement) -> Option<IUIAutomationElement> {
    let focused = automation
        .CreatePropertyCondition(UIA_HasKeyboardFocusPropertyId, &bool_variant(true))
        .ok()?;
    window.FindFirst(TreeScope_Descendants, &focused).ok()
}

/// Does this element take typed text?
///
/// An Edit (or an editable ComboBox) is a field unless its ValuePattern says read-only;
/// one that exposes no ValuePattern at all still counts, because that is how browsers
/// present a `contenteditable`. A **Document** is the other way round: in a browser the
/// whole page is one, so it counts only when it says outright that it is writable (a
/// rich-text editor does; an article does not).
unsafe fn editable(element: &IUIAutomationElement) -> bool {
    if !element.CurrentIsEnabled().map(|on| on.as_bool()).unwrap_or(false) {
        return false;
    }
    let read_only = element
        .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
        .ok()
        .and_then(|value| value.CurrentIsReadOnly().ok())
        .map(|flag| flag.as_bool());
    match element.CurrentControlType().map(|kind| kind.0).unwrap_or_default() {
        kind if kind == UIA_EditControlTypeId.0 || kind == UIA_ComboBoxControlTypeId.0 => read_only != Some(true),
        kind if kind == UIA_DocumentControlTypeId.0 => read_only == Some(false),
        _ => false,
    }
}

/// The page (or the window itself): as far up as a form search may go.
unsafe fn is_boundary(element: &IUIAutomationElement) -> bool {
    matches!(
        element.CurrentControlType().map(|kind| kind.0).unwrap_or_default(),
        kind if kind == UIA_DocumentControlTypeId.0 || kind == UIA_WindowControlTypeId.0
    )
}

impl Ready {
    /// Anything may be typed at all.
    pub fn is_blocked(self) -> bool {
        self == Ready::Blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole safety of [`click_sign_in`] is in this vocabulary. A false positive here
    /// is a click into someone else's identity provider — or into "Delete account".
    #[test]
    fn only_a_sites_own_sign_in_link_is_clickable() {
        for name in [
            "Log in",
            "LOGIN",
            "Sign in",
            " Sign In ",
            "Log in →",
            "Anmelden",
            "Einloggen",
        ] {
            assert!(is_sign_in_label(name), "{name} is the site's own sign-in");
        }
        for name in [
            // The dangerous ones: every single one *contains* "sign in" or "log in", which
            // is exactly why containment was not good enough.
            "Sign in with Google",
            "Sign in with Apple",
            "Continue with Google",
            "Can't log in?",
            "Sign in to your Microsoft account",
            "Login help",
            "Sign up",
            "Logout",
            "",
        ] {
            assert!(!is_sign_in_label(name), "{name} must never be clicked for us");
        }
    }
}
