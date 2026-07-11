//! "Does this look like a secret?" — the last line of defence for clipboard history.
//!
//! The first two lines are exact: Funke's own vault copies are written with the exclusion
//! markers ([`crate::win::write_secret`]), and other password managers set the same
//! markers, so neither is ever offered to this module. What is left is the accident — a
//! token pasted out of a terminal, an API key out of a dashboard, a private key out of an
//! editor — where nothing marked anything and the text is simply *shaped* like a credential.
//!
//! This is a heuristic and it is honest about that: it will miss secrets (a short human
//! password is indistinguishable from a word) and it will occasionally drop something
//! harmless (a long random-looking id). It errs toward dropping, because a clipboard
//! history that quietly keeps an API key is worse than one that quietly forgets a hash.

/// Vendor prefixes that are never anything but a credential.
const TOKEN_PREFIXES: &[&str] = &[
    "ghp_",        // GitHub personal access token
    "gho_",        // GitHub OAuth
    "ghu_",        // GitHub user-to-server
    "ghs_",        // GitHub server-to-server
    "ghr_",        // GitHub refresh
    "github_pat_", // GitHub fine-grained PAT
    "glpat-",      // GitLab
    "sk-",         // OpenAI & friends
    "sk_live_",    // Stripe secret
    "sk_test_",    // Stripe test secret
    "rk_live_",    // Stripe restricted
    "pk_live_",    // Stripe publishable (not secret, but nobody wants it in history)
    "xox",         // Slack (xoxb-, xoxp-, xoxa-, …)
    "AKIA",        // AWS access key id
    "ASIA",        // AWS temporary access key id
    "AIza",        // Google API key
    "ya29.",       // Google OAuth access token
    "npm_",        // npm automation token
    "dop_v1_",     // DigitalOcean
    "shpat_",      // Shopify
    "SG.",         // SendGrid
    "hf_",         // Hugging Face
    "sk-ant-",     // Anthropic
    "Bearer ",     // an Authorization header, pasted whole
    "-----BEGIN",  // PEM: private keys, certificates
];

/// The shortest opaque token we will judge by shape alone. Below this, false positives
/// (commit hashes are 40 chars, but so is nothing else you copy) outweigh the catch.
const MIN_ENTROPY_LEN: usize = 20;
/// Shannon entropy per character. English prose sits near 2–3; base64/hex key material
/// sits above 4. The bar is deliberately above prose and below the theoretical max.
const MIN_ENTROPY_BITS: f64 = 3.2;

/// Is this text shaped like a credential? See the module docs for what that buys.
pub fn looks_like_secret(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    // A PEM block is multi-line; check it before the single-token gate below.
    if text.starts_with("-----BEGIN") {
        return true;
    }
    // Prose is not a secret, and anything with a space in it is prose — except the
    // vendor prefixes, which are matched first.
    if TOKEN_PREFIXES.iter().any(|prefix| text.starts_with(prefix)) {
        return true;
    }
    if text.contains(char::is_whitespace) {
        return false;
    }
    // Things that *look* random but are yours to keep: links and paths. A URL is high
    // entropy and full of symbols; forgetting the one you just copied would be maddening.
    if text.contains("://") || text.starts_with("www.") || text.contains('\\') || text.starts_with('/') {
        return false;
    }
    if is_jwt(text) {
        return true;
    }

    text.chars().count() >= MIN_ENTROPY_LEN && character_classes(text) >= 3 && entropy_bits(text) >= MIN_ENTROPY_BITS
}

/// `header.payload.signature`, all base64url — a JSON Web Token, i.e. a bearer credential.
fn is_jwt(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    parts.len() == 3
        && parts[0].starts_with("ey")
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '=')
        })
}

/// How many of {lowercase, uppercase, digit, symbol} appear. A single class is a word,
/// a number, or a hash — not the mixed alphabet key material tends to have.
fn character_classes(text: &str) -> u8 {
    let (mut lower, mut upper, mut digit, mut symbol) = (false, false, false, false);
    for c in text.chars() {
        match c {
            c if c.is_lowercase() => lower = true,
            c if c.is_uppercase() => upper = true,
            c if c.is_ascii_digit() => digit = true,
            _ => symbol = true,
        }
    }
    u8::from(lower) + u8::from(upper) + u8::from(digit) + u8::from(symbol)
}

/// Shannon entropy of the character distribution, in bits per character.
fn entropy_bits(text: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0f64;
    for c in text.chars() {
        *counts.entry(c).or_insert(0f64) += 1.0;
        total += 1.0;
    }
    counts
        .values()
        .map(|count| {
            let p = count / total;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_tokens_and_keys_are_caught_outright() {
        assert!(looks_like_secret("ghp_16C7e42F292c6912E7710c838347Ae178B4a"));
        assert!(looks_like_secret("sk-ant-api03-abcDEF123456"));
        assert!(looks_like_secret("AKIAIOSFODNN7EXAMPLE"));
        assert!(looks_like_secret("xoxb-1234-5678-abcdefghijklmnop"));
        assert!(looks_like_secret(
            "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1r\n-----END"
        ));
        assert!(looks_like_secret(
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.dBjftJeZ4CVPmB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        ));
    }

    #[test]
    fn random_looking_tokens_are_caught_by_shape() {
        assert!(looks_like_secret("Xk7pQm2Rv9Ls4Tz8Wn3Yb6Hd"));
        assert!(looks_like_secret("aG9sZG9udGhpc2lzYmFzZTY0ISE=Zm9v"));
    }

    /// The failure that would make the feature worse than useless: forgetting the things
    /// people actually copy all day.
    #[test]
    fn ordinary_copies_are_kept() {
        assert!(!looks_like_secret(
            "https://github.com/klappstuhlpy/funke/releases/tag/v0.3.1"
        ));
        assert!(!looks_like_secret(r"C:\Users\bened\Documents\Coding\funke\README.md"));
        assert!(!looks_like_secret(
            "cargo clippy --workspace --all-targets -- -D warnings"
        ));
        assert!(!looks_like_secret("bigbenwashere@gmail.com"));
        assert!(!looks_like_secret("Guten Morgen, wie geht es dir?"));
        assert!(!looks_like_secret("supercalifragilistic"));
        assert!(!looks_like_secret("42"));
        assert!(!looks_like_secret(""));
    }

    /// Documented blind spot: a short human-chosen password is a word with a number on
    /// the end, and nothing about its shape gives it away. The exclusion markers, not
    /// this function, are what keep managed passwords out of the history.
    #[test]
    fn a_short_human_password_is_not_detectable_by_shape() {
        assert!(!looks_like_secret("Sommer2024!"));
    }
}
