//! Inline calculator: queries that look like arithmetic evaluate immediately and the
//! result tops the list; Enter copies it to the clipboard.

use funke_core::{glyph_data_url, Action, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

pub struct CalcProvider;

/// An equals sign — the row's title already carries the computed value.
const CALC_GLYPH: &str = "<path d='M6.5 9.5h11'/><path d='M6.5 14.5h11'/>";

/// Well above any fuzzy score so the result always leads the list.
const CALC_SCORE: i64 = 400;

impl SearchProvider for CalcProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "calc",
            name: "Calculator",
            prefix: None,
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let text = query.text.trim();
        if !looks_like_math(text) {
            return Vec::new();
        }
        let Some(value) = evaluate(text) else {
            return Vec::new();
        };
        vec![ResultItem {
            id: format!("calc:{text}"),
            provider: "calc".into(),
            title: format!("= {value}"),
            subtitle: Some(format!("{text} — Enter copies the result")),
            icon: Some(glyph_data_url(CALC_GLYPH)),
            score: CALC_SCORE,
            actions: vec![NamedAction::new("Copy result", Action::CopyText { text: value })],
        }]
    }
}

/// Digits plus arithmetic characters only, with at least one digit and one operator —
/// cheap enough to run on every keystroke without ever evaluating prose.
fn looks_like_math(text: &str) -> bool {
    let mut has_digit = false;
    let mut has_operator = false;
    for c in text.chars() {
        match c {
            '0'..='9' => has_digit = true,
            '+' | '-' | '*' | '/' | '^' => has_operator = true,
            '(' | ')' | '.' | ' ' => {}
            _ => return false,
        }
    }
    has_digit && has_operator
}

fn evaluate(text: &str) -> Option<String> {
    let value = meval::eval_str(text).ok()?;
    if !value.is_finite() {
        return None;
    }
    // Render integers without a trailing ".0"; keep floats as-is.
    if value.fract() == 0.0 && value.abs() < 1e15 {
        Some(format!("{}", value as i64))
    } else {
        Some(format!("{value}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use funke_core::Query;

    #[test]
    fn prose_is_not_math() {
        assert!(!looks_like_math("firefox"));
        assert!(!looks_like_math("report q3"));
        assert!(!looks_like_math("123"));
        assert!(looks_like_math("2+2*3"));
        assert!(looks_like_math("(1.5 + 2) ^ 3"));
    }

    #[test]
    fn evaluates_and_formats() {
        let items = CalcProvider.query(&Query::new("2+2*3"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "= 8");
        assert!(matches!(items[0].primary_action(), Some(Action::CopyText { text }) if text == "8"));

        assert_eq!(evaluate("10/4"), Some("2.5".to_string()));
        assert_eq!(evaluate("1/0"), None, "non-finite results are dropped");
        assert!(CalcProvider.query(&Query::new("hello")).is_empty());
    }
}
