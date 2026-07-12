//! Autotype sequences: the KeePass-style template that says *what* gets typed into the
//! target window and in which order.
//!
//! A sequence is parsed into [`Step`]s, which name the fields they want rather than
//! carrying them — no secret ever lives inside a parsed sequence. The app crate resolves
//! `Username`/`Password`/`Totp` at action time (from freshly fetched, zeroized
//! credentials) and drives `SendInput`.
//!
//! Precedence, most specific first:
//! 1. the entry's own `autotype` custom field in Bitwarden,
//! 2. [`Settings::vault_autotype_sequence`](funke_core::Settings) — the user's default,
//! 3. the built-in [`DEFAULT`] (plus `{ENTER}` when `vault_autotype_enter` is on).

/// Username, Tab, password — the sequence that fits almost every login form. The
/// trailing `{ENTER}` is appended separately, per the `vault_autotype_enter` setting.
pub const DEFAULT: &str = "{USERNAME}{TAB}{PASSWORD}";

/// One instruction in a sequence. Field steps are placeholders resolved by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Type literal characters.
    Text(String),
    Username,
    Password,
    /// The current TOTP code (fetched from the CLI at action time).
    Totp,
    Tab,
    Enter,
    /// Wait, for forms that reveal the password field only after the username
    /// (`{DELAY=500}`).
    Delay(u64),
}

/// Parse a template into steps. Unknown `{TOKEN}`s are typed literally rather than
/// dropped — a typo shows up in the target window instead of silently vanishing, and a
/// password containing braces still round-trips.
pub fn parse(template: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut literal = String::new();
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let token = &after[..close];
        match step_for(token) {
            Some(step) => {
                literal.push_str(&rest[..open]);
                if !literal.is_empty() {
                    steps.push(Step::Text(std::mem::take(&mut literal)));
                }
                steps.push(step);
            }
            // Not a token we know: keep it as typed text and move past it.
            None => literal.push_str(&rest[..open + 1 + close + 1]),
        }
        rest = &after[close + 1..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        steps.push(Step::Text(literal));
    }
    steps
}

/// The tail of a sequence from its first `{PASSWORD}` on — what to type when the caret
/// could only be put in the **password** field itself (a password-only page, or a form
/// whose username field UI Automation can't reach; see `funke_shell::Ready::PasswordOnly`).
///
/// Typing the whole sequence there would put the username in the password box and the
/// password wherever `{TAB}` happened to land. A sequence with no `{PASSWORD}` at all
/// yields nothing: there is no honest way to run `{USERNAME}{TAB}` into a password field,
/// and the caller reports that as a blocked autotype rather than typing something else.
pub fn password_onward(steps: &[Step]) -> Vec<Step> {
    match steps.iter().position(|step| *step == Step::Password) {
        Some(start) => steps[start..].to_vec(),
        None => Vec::new(),
    }
}

fn step_for(token: &str) -> Option<Step> {
    if let Some(ms) = token
        .strip_prefix("DELAY=")
        .or_else(|| token.strip_prefix("delay="))
        .or_else(|| token.strip_prefix("Delay="))
    {
        // Cap it: a stray {DELAY=999999} must not wedge the action thread for a quarter hour.
        return ms.trim().parse::<u64>().ok().map(|ms| Step::Delay(ms.min(5_000)));
    }
    match token.to_ascii_uppercase().as_str() {
        "USERNAME" | "USER" => Some(Step::Username),
        "PASSWORD" | "PASS" => Some(Step::Password),
        "TOTP" => Some(Step::Totp),
        "TAB" => Some(Step::Tab),
        "ENTER" | "RETURN" => Some(Step::Enter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_sequence_is_user_tab_password() {
        assert_eq!(parse(DEFAULT), vec![Step::Username, Step::Tab, Step::Password]);
        assert_eq!(
            parse("{USERNAME}{TAB}{PASSWORD}{ENTER}"),
            vec![Step::Username, Step::Tab, Step::Password, Step::Enter]
        );
    }

    #[test]
    fn tokens_are_case_insensitive_and_aliased() {
        assert_eq!(
            parse("{user}{tab}{pass}{return}{totp}"),
            vec![Step::Username, Step::Tab, Step::Password, Step::Enter, Step::Totp]
        );
    }

    #[test]
    fn literal_text_and_delays_survive() {
        assert_eq!(
            parse("{USERNAME}{ENTER}{DELAY=500}{PASSWORD}"),
            vec![Step::Username, Step::Enter, Step::Delay(500), Step::Password]
        );
        assert_eq!(
            parse("prefix {USERNAME} suffix"),
            vec![
                Step::Text("prefix ".into()),
                Step::Username,
                Step::Text(" suffix".into())
            ]
        );
        assert_eq!(parse("{DELAY=999999}"), vec![Step::Delay(5_000)], "delays are capped");
    }

    /// The caret is in the password box: everything the sequence would have typed *before*
    /// the password belongs to a field that isn't there.
    #[test]
    fn a_password_only_target_gets_the_sequence_from_the_password_on() {
        assert_eq!(
            password_onward(&parse("{USERNAME}{TAB}{PASSWORD}{ENTER}")),
            vec![Step::Password, Step::Enter]
        );
        assert_eq!(
            password_onward(&parse("{USERNAME}{TAB}{PASSWORD}{TAB}{TOTP}{ENTER}")),
            vec![Step::Password, Step::Tab, Step::Totp, Step::Enter]
        );
        // Nothing to salvage: a sequence that never types a password has no meaning in a
        // password field, so it types nothing at all.
        assert!(password_onward(&parse("{USERNAME}{ENTER}")).is_empty());
        assert!(password_onward(&[]).is_empty());
    }

    #[test]
    fn unknown_tokens_and_stray_braces_are_typed_literally() {
        assert_eq!(
            parse("{NOPE}{USERNAME}"),
            vec![Step::Text("{NOPE}".into()), Step::Username]
        );
        assert_eq!(parse("a{b"), vec![Step::Text("a{b".into())]);
        assert!(parse("").is_empty());
    }
}
