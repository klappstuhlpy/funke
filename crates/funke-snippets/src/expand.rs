//! Placeholder expansion: turning a snippet's stored text into the text that actually
//! gets pasted.
//!
//! The token vocabulary deliberately mirrors the vault's autotype sequences
//! (`funke-vault::sequence`), down to the rule that an **unknown token is typed
//! literally** — `{FOO}` comes out as `{FOO}`. A snippet is text the user wrote; the
//! expander must never eat part of it because it guessed at a token.
//!
//! Expansion happens when the snippet is *pasted*, never when it is listed: `{DATE}`
//! means the day you use it, and `{CLIPBOARD}` means what you copied — resolving either
//! at query time would bake in whatever was true while you were still typing the search.

use chrono::Local;

/// What the caller must supply for the dynamic tokens. The clipboard is passed in rather
/// than read here so this stays a pure function (and so the crate doesn't have to depend
/// on another provider).
#[derive(Debug, Default, Clone)]
pub struct Context {
    /// Text currently on the clipboard, for `{CLIPBOARD}`.
    pub clipboard: Option<String>,
}

/// A snippet, resolved and ready to paste.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expansion {
    pub text: String,
    /// How many characters the caret must move *left* after the text lands, to sit where
    /// `{CURSOR}` was. Zero means "leave the caret at the end", the usual case.
    pub cursor_back: usize,
}

/// Expand `content`, resolving the dynamic tokens against `context` and `now`.
///
/// `now` is a parameter rather than a call to the clock so the tests are deterministic —
/// the same reason `FrecencyStore` takes its timestamps from the caller.
pub fn expand_at(content: &str, context: &Context, now: chrono::DateTime<Local>) -> Expansion {
    let mut text = String::with_capacity(content.len());
    let mut cursor_at: Option<usize> = None;
    let mut rest = content;

    while let Some(open) = rest.find('{') {
        text.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // An unclosed brace is just a brace the user typed.
            text.push_str(&rest[open..]);
            return finish(text, cursor_at);
        };
        let token = &after[..close];
        match resolve(token, context, now) {
            Some(value) => text.push_str(&value),
            None if token.eq_ignore_ascii_case("CURSOR") => {
                // Only the first {CURSOR} wins; a second one is meaningless (there is one
                // caret) and is dropped rather than typed, so it can't corrupt the text.
                cursor_at.get_or_insert(text.chars().count());
            }
            // Not a token we know: the braces and everything in them are the user's text.
            None => {
                text.push('{');
                text.push_str(token);
                text.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    text.push_str(rest);
    finish(text, cursor_at)
}

/// [`expand_at`] against the current local time.
pub fn expand(content: &str, context: &Context) -> Expansion {
    expand_at(content, context, Local::now())
}

fn finish(text: String, cursor_at: Option<usize>) -> Expansion {
    let total = text.chars().count();
    Expansion {
        cursor_back: cursor_at.map_or(0, |at| total.saturating_sub(at)),
        text,
    }
}

/// `None` for tokens this function doesn't produce text for — `{CURSOR}` (handled by the
/// caller, since it is a position and not text) and anything unknown.
fn resolve(token: &str, context: &Context, now: chrono::DateTime<Local>) -> Option<String> {
    let upper = token.to_ascii_uppercase();
    match upper.as_str() {
        "DATE" => Some(now.format("%Y-%m-%d").to_string()),
        "TIME" => Some(now.format("%H:%M").to_string()),
        "DATETIME" => Some(now.format("%Y-%m-%d %H:%M").to_string()),
        "CLIPBOARD" => Some(context.clipboard.clone().unwrap_or_default()),
        "NEWLINE" | "ENTER" => Some("\n".into()),
        "TAB" => Some("\t".into()),
        _ => {
            // {DATE:%d.%m.%Y} — a strftime format of the user's choosing.
            let (name, format) = token.split_once(':')?;
            match name.trim().to_ascii_uppercase().as_str() {
                "DATE" | "TIME" | "DATETIME" => Some(now.format(format).to_string()),
                _ => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(text: &str) -> Expansion {
        let now = Local.with_ymd_and_hms(2026, 7, 11, 14, 30, 0).unwrap();
        expand_at(text, &Context::default(), now)
    }

    #[test]
    fn date_and_time_tokens_resolve_to_the_moment_of_pasting() {
        assert_eq!(at("Today is {DATE}.").text, "Today is 2026-07-11.");
        assert_eq!(at("{TIME}").text, "14:30");
        assert_eq!(at("{DATETIME}").text, "2026-07-11 14:30");
        // Tokens are case-insensitive, as in vault sequences.
        assert_eq!(at("{date}").text, "2026-07-11");
        // …and a strftime format of your own works: German dates, for one.
        assert_eq!(at("{DATE:%d.%m.%Y}").text, "11.07.2026");
    }

    #[test]
    fn the_clipboard_token_takes_what_you_copied() {
        let context = Context {
            clipboard: Some("https://example.com".into()),
        };
        let now = Local.with_ymd_and_hms(2026, 7, 11, 14, 30, 0).unwrap();
        assert_eq!(
            expand_at("See [link]({CLIPBOARD})", &context, now).text,
            "See [link](https://example.com)"
        );
        // Nothing on the clipboard: the token vanishes rather than pasting the word.
        assert_eq!(at("See [link]({CLIPBOARD})").text, "See [link]()");
    }

    #[test]
    fn cursor_marks_where_the_caret_lands_and_is_not_typed() {
        let expansion = at("<div>{CURSOR}</div>");
        assert_eq!(expansion.text, "<div></div>", "the marker itself is never typed");
        assert_eq!(expansion.cursor_back, 6, "back over '</div>'");

        // No marker: the caret stays at the end, which is what you want almost always.
        assert_eq!(at("plain text").cursor_back, 0);
    }

    /// The rule that keeps a snippet honest: it is text the user wrote, so anything the
    /// expander doesn't recognize comes out exactly as typed.
    #[test]
    fn unknown_tokens_and_stray_braces_survive_verbatim() {
        assert_eq!(
            at("fn main() { println!(\"{}\"); }").text,
            "fn main() { println!(\"{}\"); }"
        );
        assert_eq!(at("{NOPE} and {").text, "{NOPE} and {");
        assert_eq!(at("style={{ color: red }}").text, "style={{ color: red }}");
    }

    #[test]
    fn whitespace_tokens_let_a_one_line_field_hold_a_multi_line_snippet() {
        assert_eq!(at("a{NEWLINE}b{TAB}c").text, "a\nb\tc");
    }
}
