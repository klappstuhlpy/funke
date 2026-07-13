//! Inline calculator: queries that look like arithmetic evaluate immediately and the
//! result tops the list; Enter copies it to the clipboard.
//!
//! It also converts units (`100 mb in gb`, `72 f in c`) — see [`crate::units`], which owns the
//! grammar and the table. The two share this provider because they share the user's intent: you
//! typed a number and you want a different number back. They are asked in that order, and a
//! conversion never reaches the arithmetic parser, which would make nothing of `in` anyway.

use funke_core::{glyph_data_url, Action, NamedAction, ProviderMeta, Query, ResultItem, SearchProvider};

use crate::units;

pub struct CalcProvider;

/// An equals sign — the row's title already carries the computed value.
const CALC_GLYPH: &str = "<path d='M6.5 9.5h11'/><path d='M6.5 14.5h11'/>";

/// Well above any fuzzy score so the result always leads the list.
const CALC_SCORE: i64 = 400;

impl SearchProvider for CalcProvider {
    fn metadata(&self) -> ProviderMeta {
        ProviderMeta {
            id: "calc",
            name: funke_core::t("provider.calculator"),
            prefix: None,
            prefix_only: false,
        }
    }

    fn query(&self, query: &Query) -> Vec<ResultItem> {
        let text = query.text.trim();

        // Asked first, and it says no to prose without evaluating anything — a conversion is
        // the only shape here that contains letters, so letting arithmetic look at it first
        // would mean widening the arithmetic gate to admit words.
        if let Some(conversion) = units::convert(text) {
            return vec![row(
                text,
                format!("= {}", conversion.labelled()),
                vec![
                    // The bare number is what goes into a spreadsheet; the labelled one is what
                    // goes into a sentence. Both are one keystroke away.
                    NamedAction::new(
                        funke_core::t("action.copy_result"),
                        Action::CopyText {
                            text: conversion.value.clone(),
                        },
                    ),
                    NamedAction::new(
                        funke_core::t("action.copy_with_unit"),
                        Action::CopyText {
                            text: conversion.labelled(),
                        },
                    ),
                ],
            )];
        }

        if !looks_like_math(text) {
            return Vec::new();
        }
        let Some(value) = evaluate(text) else {
            return Vec::new();
        };
        vec![row(
            text,
            format!("= {value}"),
            vec![NamedAction::new(
                funke_core::t("action.copy_result"),
                Action::CopyText { text: value },
            )],
        )]
    }
}

fn row(text: &str, title: String, actions: Vec<NamedAction>) -> ResultItem {
    ResultItem {
        id: format!("calc:{text}"),
        provider: "calc".into(),
        title,
        subtitle: Some(funke_core::tf("calc.subtitle", &[("expression", text)])),
        icon: Some(glyph_data_url(CALC_GLYPH)),
        score: CALC_SCORE,
        actions,
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
    let value = fasteval::ez_eval(text, &mut fasteval::EmptyNamespace).ok()?;
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

    #[test]
    fn a_conversion_answers_in_the_same_row_the_calculator_would_have() {
        let items = CalcProvider.query(&Query::new("100 mb in gb"));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "= 0.1 GB");
        assert!(
            matches!(items[0].primary_action(), Some(Action::CopyText { text }) if text == "0.1"),
            "Enter copies the number — the unit is one key further, on the second action"
        );
        assert!(matches!(&items[0].actions[1].action, Action::CopyText { text } if text == "0.1 GB"));
    }

    /// The gate widened to admit letters, and that is the thing most likely to go wrong: an
    /// ordinary word must still cost nothing and answer nothing.
    #[test]
    fn words_still_reach_neither_the_converter_nor_the_calculator() {
        for text in ["firefox", "report q3", "go to work", "settings", "in the morning"] {
            assert!(
                CalcProvider.query(&Query::new(text)).is_empty(),
                "`{text}` produced a calculator row"
            );
        }
    }
}
