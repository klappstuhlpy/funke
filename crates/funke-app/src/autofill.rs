//! The two ways a credential gets typed, and the guard both of them pass through.
//!
//! `autotype.rs` is the *hand*: it turns [`Step`](funke_vault::Step)s into `SendInput`
//! keystrokes and asks no questions. This module is the judgment around it — where those
//! keystrokes are allowed to land, and what to do when the answer is "nowhere":
//!
//! - [`autotype`] types into the window the overlay was summoned from. Before a secret
//!   goes anywhere it asks `funke_shell::has_password_field` whether that window even
//!   shows a login form. Discord's message box does not, and a password typed into it —
//!   with the sequence's trailing Enter behind it — is a password posted to a channel.
//!   That is the accident this guard exists for; everything else it catches (the desktop,
//!   a game, an editor) is the same accident with a smaller blast radius.
//! - [`open_and_autotype`] is for the credential whose window isn't open yet: it opens the
//!   entry's website, waits for the browser to actually be *on that site* with a login
//!   form up, and fills it in. The waiting is what makes it safe — it never types into a
//!   page it cannot identify.
//!
//! A refusal is never silent and never final: the overlay comes back with the credential,
//! the reason, and a "type it anyway" that takes a confirming Enter. UI Automation cannot
//! read every window (a game, an RDP session, a terminal), and a launcher that answers
//! "no" to those with nothing but silence teaches its user to switch the guard off.
//!
//! **Everything here runs on a worker thread.** Sync Tauri commands run on the main
//! thread, which is the event loop *and* an STA — a UIA call from it can deadlock (see
//! `funke_shell::uia`), and the polling below would freeze the app outright.

use std::sync::Arc;
use std::time::{Duration, Instant};

use funke_core::Settings;
use funke_vault::{FocusContext, Step, Vault};
use tauri::{AppHandle, Emitter, Manager};

use crate::{focus, hide, show, AppState, MAIN_WINDOW};

/// A UIA tree can answer "no fields here" simply because the app hasn't built its
/// accessibility tree yet — Chromium builds one lazily, when a client like us first asks.
/// So a "no" is re-asked a few times before it is believed.
const FORM_PROBE_ATTEMPTS: usize = 3;
const FORM_PROBE_INTERVAL: Duration = Duration::from_millis(120);

/// Long enough for a cold browser to start, load, and render a login form; short enough
/// that a wrong guess (an SSO redirect we can't match) doesn't leave the user waiting.
const SITE_TIMEOUT: Duration = Duration::from_secs(20);
const SITE_POLL_INTERVAL: Duration = Duration::from_millis(400);

/// How long the entry's page is given to produce a login form on its own before we go
/// looking for its sign-in link. Long enough that a form arriving a beat late is waited
/// for rather than clicked past.
const SIGN_IN_SETTLE: Duration = Duration::from_millis(1_200);

/// Let the foreground change land before keystrokes chase it.
///
/// `SetForegroundWindow` returns before the switch has happened, so the target is *polled*
/// for the foreground rather than slept at — a window that never gets it would otherwise
/// have its secret typed into whoever did.
const FOCUS_TIMEOUT: Duration = Duration::from_millis(1_500);
const FOCUS_POLL: Duration = Duration::from_millis(30);

/// And then a beat more, because owning the foreground is not the same as being ready to
/// read: a browser-engine app (CEF, Electron) restores its window first and hands keyboard
/// focus to the renderer afterwards. In between the login field is drawn with its focus
/// ring and drops every character sent to it.
///
/// ponytail: fixed delay — the in-page focus of another process's renderer is not
/// observable (those apps expose no accessibility tree at all). Raise it if a target still
/// eats the first field.
const FOCUS_SETTLE: Duration = Duration::from_millis(250);

/// Autotype an entry into the window the overlay was summoned from (`target`).
///
/// `force` is the user's own "type anyway" from a refused attempt — the one path that
/// types a secret into a window showing no login form. Nothing else sets it.
pub fn autotype(app: AppHandle, id: String, target: Option<isize>, force: bool) {
    let state = app.state::<AppState>();
    let vault = Arc::clone(&state.vault);
    let guarded = guard_enabled(&state.settings) && !force;

    let Some(hwnd) = target else {
        // Nothing was focused when the overlay came up (the desktop, a fresh boot). There
        // is no window to type into at all, so there is nothing to refuse *into* either.
        hide(&app, false);
        return;
    };

    // Asked while the overlay is still up: the common refusal — a chat window, an editor,
    // the desktop — then costs no flicker, and the reason lands on a window that is
    // already on screen.
    if guarded && !has_login_form(hwnd) {
        block(
            &app,
            &vault,
            &id,
            no_form_reason(&app, hwnd),
            Blocked::Standing(Some(hwnd)),
        );
        return;
    }

    hide(&app, false);
    focus::focus_window(hwnd);
    await_focus(hwnd);
    type_into(&app, &vault, &id, hwnd, guarded);
}

/// Open an entry's website and fill in the login form it brings up.
///
/// The credential is typed only once the browser is demonstrably *on the entry's site*
/// (its address bar says so, judged by the same conservative matcher the context
/// suggestions use — registrable-domain equality, no fuzz) **and** a login form is up. A
/// site that never shows one, or an SSO redirect to a domain the entry doesn't name, ends
/// in a refusal rather than a guess.
pub fn open_and_autotype(app: AppHandle, id: String, target: Option<isize>) {
    let state = app.state::<AppState>();
    let vault = Arc::clone(&state.vault);

    let Some(entry) = vault.entry(&id) else {
        hide(&app, false);
        return;
    };
    let Some(url) = entry.uri.clone() else {
        // Nothing to open: an app-only entry (`androidapp://…`), or no URI at all. The
        // overlay is still up, and the window the user came from is still the one a
        // "type anyway" would go to.
        block(
            &app,
            &vault,
            &id,
            funke_core::t("vault.blocked.no_url").to_string(),
            Blocked::Standing(target),
        );
        return;
    };
    let site = entry.host.clone().unwrap_or_else(|| url.clone());

    hide(&app, false);
    if let Err(e) = open::that_detached(&url) {
        eprintln!("failed to open {url}: {e}");
        return;
    }

    match await_login_page(&vault, &id) {
        Some(hwnd) => {
            focus::focus_window(hwnd);
            await_focus(hwnd);
            // Guarded even though we just watched the form appear: between the poll and
            // the keystrokes the page can navigate, and `prepare` is also what puts the
            // caret *in* the form — a freshly loaded page rarely focuses it for us.
            type_into(&app, &vault, &id, hwnd, true);
        }
        None => block(
            &app,
            &vault,
            &id,
            funke_core::tf("vault.blocked.no_site", &[("app", &site)]),
            Blocked::Resummon,
        ),
    }
}

/// Wait for `hwnd` to actually own the foreground, then let its fields become ready.
///
/// Both halves are load-bearing. The poll is correctness: `focus_window` only *asks*, and
/// typing before the switch lands sends the credential to whatever window is still in
/// front. The settle afterwards is the browser-engine case above — foreground restored,
/// renderer not yet listening, characters silently dropped.
///
/// Timing out is not a refusal: `prepare` still has to find a field in this window before
/// anything is typed, and it is the one that reports a window it cannot aim at.
fn await_focus(hwnd: isize) {
    let deadline = Instant::now() + FOCUS_TIMEOUT;
    while Instant::now() < deadline && focus::foreground_window() != Some(hwnd) {
        std::thread::sleep(FOCUS_POLL);
    }
    std::thread::sleep(FOCUS_SETTLE);
}

/// The shared tail of both flows: aim, then type. `hwnd` is focused and settled.
fn type_into(app: &AppHandle, vault: &Arc<Vault>, id: &str, hwnd: isize, guarded: bool) {
    let mut steps = vault.autotype_steps(id);

    if guarded {
        match funke_shell::prepare(hwnd) {
            // The caret sits in a field (the user's, or the form's username field, which
            // `prepare` focused for us): type the sequence as written.
            funke_shell::Ready::Sequence => {}
            // Only the password field could be focused — a password-only page, or a form
            // whose username field UIA can't reach. Typing `{USERNAME}{TAB}` into a
            // password box is how a username ends up in a password field, and a password
            // in whatever the Tab landed on.
            funke_shell::Ready::PasswordOnly => steps = funke_vault::password_onward(&steps),
            funke_shell::Ready::Blocked => {
                block(
                    app,
                    vault,
                    id,
                    funke_core::tf("vault.blocked.no_field", &[("app", &window_label(app, hwnd))]),
                    Blocked::Resummon,
                );
                return;
            }
        }
        if steps.is_empty() {
            block(
                app,
                vault,
                id,
                funke_core::tf("vault.blocked.no_field", &[("app", &window_label(app, hwnd))]),
                Blocked::Resummon,
            );
            return;
        }
    }

    // Secrets are fetched here, at the last possible moment, and only the ones the
    // sequence names.
    let credentials = match vault.credentials(id) {
        Ok(credentials) => credentials,
        Err(e) => {
            eprintln!("vault autotype failed: {e}");
            return;
        }
    };
    let totp = if steps.contains(&Step::Totp) {
        match vault.totp(id) {
            Ok(code) => Some(code),
            Err(e) => {
                eprintln!("vault TOTP failed: {e}");
                None
            }
        }
    } else {
        None
    };

    crate::autotype::run(&steps, &credentials, totp.as_deref());

    // Credentials zeroize on drop (funke-vault); the TOTP code is ours to wipe.
    if let Some(mut code) = totp {
        zeroize::Zeroize::zeroize(&mut code);
    }
}

/// Does the window show a login form? A "no" is re-asked: an app that has never had an
/// accessibility client (Chromium especially) builds its UIA tree only once one asks, so
/// the first look can be at an empty tree.
fn has_login_form(hwnd: isize) -> bool {
    for attempt in 0..FORM_PROBE_ATTEMPTS {
        if funke_shell::has_password_field(hwnd) {
            return true;
        }
        if attempt + 1 < FORM_PROBE_ATTEMPTS {
            std::thread::sleep(FORM_PROBE_INTERVAL);
        }
    }
    false
}

/// Wait for the browser to be on the entry's site with a login form up, and hand back its
/// window. `None` on timeout — the page never showed a form, or the browser went somewhere
/// the entry doesn't name (an SSO redirect, a wrong default browser, a click elsewhere).
///
/// A saved URI is very often a *homepage* (`discord.com`, `github.com`), which shows no
/// password field at all. Rather than guessing a login URL — or, worse, looking one up in a
/// search engine, where the result is whoever won the SEO — the site is asked for its own
/// sign-in link once the page has settled (`funke_shell::click_sign_in`), and the wait goes
/// on. Once: a page that doesn't answer the first click won't answer the fifth, and clicking
/// repeatedly through someone's site is not a thing a launcher should do.
fn await_login_page(vault: &Arc<Vault>, id: &str) -> Option<isize> {
    let deadline = Instant::now() + SITE_TIMEOUT;
    let mut settled_at: Option<Instant> = None;
    let mut clicked = false;

    while Instant::now() < deadline {
        std::thread::sleep(SITE_POLL_INTERVAL);
        let Some(hwnd) = focus::foreground_window() else {
            continue;
        };
        let Some(process) = focus::process_name(hwnd) else {
            continue;
        };
        if !funke_shell::is_browser_process(&process) {
            continue;
        }
        // The address bar has to name *this* entry's site. Same scorer as the context
        // suggestions: registrable-domain equality, deliberately no fuzzy matching.
        let context = FocusContext {
            title: focus::window_title(hwnd),
            url: funke_shell::browser_url(hwnd),
            process: Some(process),
            browser: true,
        };
        if context.host().is_none() {
            continue;
        }
        let on_site = vault
            .context_scores(&context)
            .get(id)
            .is_some_and(|score| *score >= funke_vault::MIN_SUGGEST_SCORE);
        if !on_site {
            continue;
        }
        if funke_shell::has_password_field(hwnd) {
            return Some(hwnd);
        }

        // On the right site, no form: a homepage. Give the page a moment to finish
        // rendering (a half-built DOM has neither the form nor the link), then ask it for
        // its sign-in link exactly once. The click only *navigates* — every check above
        // still has to pass on the page it lands on before a secret is typed.
        let settled = *settled_at.get_or_insert_with(Instant::now);
        if !clicked && settled.elapsed() >= SIGN_IN_SETTLE {
            clicked = true;
            funke_shell::click_sign_in(hwnd);
        }
    }
    None
}

/// Whether the overlay is still on screen when a refusal happens.
enum Blocked {
    /// It never hid (the guard refused before anything was typed): keep it, and hand the
    /// window it would have typed into back to `prev_focus` — "type anyway" needs it.
    Standing(Option<isize>),
    /// It hid, and the target window has the foreground: summon it back.
    Resummon,
}

/// Put the credential back on screen with the reason nothing was typed, and an override.
fn block(app: &AppHandle, vault: &Arc<Vault>, id: &str, reason: String, state: Blocked) {
    match state {
        Blocked::Standing(hwnd) => {
            // `run_action` took prev_focus before handing us the id; the overlay is still
            // up, so put it back rather than re-capturing (the foreground window right
            // now is the overlay itself).
            let app_state = app.state::<AppState>();
            *app_state.prev_focus.lock().unwrap() = hwnd;
        }
        Blocked::Resummon => {
            show(app);
            // The target window owns the foreground, so `set_focus` alone is refused —
            // the same attach-input dance the Windows Hello dialog needs afterwards.
            if let Some(hwnd) = app
                .get_webview_window(MAIN_WINDOW)
                .and_then(|win| win.hwnd().ok())
                .map(|hwnd| hwnd.0 as isize)
            {
                focus::force_foreground(hwnd);
            }
        }
    }

    let row = funke_vault::blocked_row(vault, id, reason);
    let _ = app.emit(
        "autotype-blocked",
        serde_json::json!({ "label": funke_core::t("vault.blocked"), "item": row }),
    );
}

/// "No login form in **Discord**" — the app the credential would have gone to. Falls back
/// to the focus context captured on summon (which knows the site in a browser), then to
/// the window's own process name.
fn no_form_reason(app: &AppHandle, hwnd: isize) -> String {
    funke_core::tf("vault.blocked.no_form", &[("app", &window_label(app, hwnd))])
}

fn window_label(app: &AppHandle, hwnd: isize) -> String {
    let state = app.state::<AppState>();
    let captured = state.focus_context.lock().unwrap().label();
    captured
        .or_else(|| focus::process_name(hwnd))
        .unwrap_or_else(|| funke_core::t("vault.blocked.window").to_string())
}

fn guard_enabled(settings: &Arc<std::sync::RwLock<Settings>>) -> bool {
    settings.read().unwrap().vault_autotype_guard
}
