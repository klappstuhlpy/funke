//! Behavioral pin for the `secretshape` migration — these are the four test blocks that
//! lived in `src/secret.rs` before the heuristic moved to the `secretshape` crate. They
//! encode funke's product decisions (what must be caught, what must be kept, the
//! documented password blind spot) and stay here for one release as insurance that the
//! dependency behaves exactly like the module it replaced. Delete after 0.8.

use secretshape::is_probably_secret;

#[test]
fn vendor_tokens_and_keys_are_caught_outright() {
    assert!(is_probably_secret("ghp_16C7e42F292c6912E7710c838347Ae178B4a"));
    assert!(is_probably_secret("sk-ant-api03-abcDEF123456"));
    assert!(is_probably_secret("AKIAIOSFODNN7EXAMPLE"));
    assert!(is_probably_secret("xoxb-1234-5678-abcdefghijklmnop"));
    assert!(is_probably_secret(
        "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1r\n-----END"
    ));
    assert!(is_probably_secret(
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk"
    ));
}

#[test]
fn random_looking_tokens_are_caught_by_shape() {
    assert!(is_probably_secret("Xk7pQm2Rv9Ls4Tz8Wn3Yb6Hd"));
    assert!(is_probably_secret("aG9sZG9udGhpc2lzYmFzZTY0ISE=Zm9v"));
}

/// The failure that would make the feature worse than useless: forgetting the things
/// people actually copy all day.
#[test]
fn ordinary_copies_are_kept() {
    assert!(!is_probably_secret(
        "https://github.com/klappstuhlpy/funke/releases/tag/v0.3.1"
    ));
    assert!(!is_probably_secret(r"C:\Users\bened\Documents\Coding\funke\README.md"));
    assert!(!is_probably_secret(
        "cargo clippy --workspace --all-targets -- -D warnings"
    ));
    assert!(!is_probably_secret("bigbenwashere@gmail.com"));
    assert!(!is_probably_secret("Guten Morgen, wie geht es dir?"));
    assert!(!is_probably_secret("supercalifragilistic"));
    assert!(!is_probably_secret("42"));
    assert!(!is_probably_secret(""));
}

/// Documented blind spot: a short human-chosen password is a word with a number on
/// the end, and nothing about its shape gives it away. The exclusion markers, not
/// this heuristic, are what keep managed passwords out of the history.
#[test]
fn a_short_human_password_is_not_detectable_by_shape() {
    assert!(!is_probably_secret("Sommer2024!"));
}
