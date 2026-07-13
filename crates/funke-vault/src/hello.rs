//! Windows Hello unlock: the `bw` session key at rest, sealed under a key that only a
//! Hello prompt can reproduce (opt-in via `Settings::vault_hello`).
//!
//! The stored blob lets repeat unlocks skip the master password. What it is:
//!
//! ```text
//! DPAPI( [version: u8 = 2] [challenge: 32] [nonce: 12] [AES-256-GCM(session key)] )
//! key = HKDF-SHA256(ikm = hello_signature(challenge), salt = challenge, info = INFO)
//! ```
//!
//! Two walls, not one. **DPAPI** binds the file to the signed-in Windows account, so it is
//! useless copied to another machine. **The AES layer** binds it to a signature only the
//! TPM will produce, and only after Hello verifies the user ([`crate::keycred`]) — so it
//! is useless to code running *as* that account, too. Neither wall alone was enough: DPAPI
//! is transparent to anything running as you, and a Hello *consent* prompt (what v1 asked
//! for) is advice a program can decline to take.
//!
//! **v1 blobs are refused, not migrated.** A v1 file is a raw DPAPI-wrapped session key —
//! the weak shape this exists to retire. It cannot be re-sealed without the session it
//! holds, and reading it to re-seal it would mean keeping the old path alive to do the
//! very thing we stopped trusting. So an old blob is deleted on sight and the user types
//! their master password once; that unlock mints a v2 blob. One prompt, and nothing weaker
//! survives the update. (The version byte is unambiguous: a `bw` session key is base64
//! text, whose every byte is ≥ 0x2B, so a v1 blob can never begin with 0x02.)

use std::fs;
use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use zeroize::Zeroize;

use crate::keycred;
use crate::secret_buf::SecretBuf;

const VERSION: u8 = 2;
const CHALLENGE_LEN: usize = 32;
const NONCE_LEN: usize = 12;
/// HKDF domain separation — this key derivation is for this file and nothing else.
const INFO: &[u8] = b"funke vault.session v2";
/// Version + challenge + nonce + a non-empty GCM tag: anything shorter isn't a v2 blob.
const HEADER_LEN: usize = 1 + CHALLENGE_LEN + NONCE_LEN;

/// Why a Hello unlock didn't happen. Every one of these ends in the masked master-password
/// prompt, so every one has to tell the user what to do now — and the *reason* differs
/// enough to matter: a reset Hello, a stale format, and a cancelled prompt are three very
/// different things to read at 9am.
///
/// The distinction the code acts on is narrower: [`Gone`](HelloError::Gone) and
/// [`Outdated`](HelloError::Outdated) mean the stored session can never be opened again and
/// is deleted; everything else leaves it exactly where it is.
#[derive(Debug, Clone, PartialEq)]
pub enum HelloError {
    /// No Hello-capable credential store on this device (no PIN, no biometrics, no TPM).
    Unsupported,
    /// The user dismissed the prompt, or chose to use their password instead.
    Cancelled,
    /// The key isn't there any more — Hello was reset, the PIN removed, the TPM cleared.
    /// Nothing sealed to it can be opened again, by us or by anyone.
    Gone,
    /// A session stored by an older Funke, under the weaker v1 scheme. Not an error the
    /// user caused, and worth saying so rather than blaming their Hello setup.
    Outdated,
    Failed(String),
}

impl HelloError {
    /// What the overlay says when it drops back to the master-password prompt.
    pub fn message(&self) -> String {
        match self {
            HelloError::Unsupported => funke_core::t("vault.hello.unsupported").into(),
            HelloError::Cancelled => funke_core::t("vault.hello.cancelled").into(),
            HelloError::Gone => funke_core::t("vault.hello.reset").into(),
            HelloError::Outdated => funke_core::t("vault.hello.outdated").into(),
            HelloError::Failed(e) => funke_core::tf("vault.hello.failed", &[("error", e)]),
        }
    }

    /// Is the stored session beyond saving? Then it gets deleted rather than left to fail
    /// the same way tomorrow.
    fn unopenable(&self) -> bool {
        matches!(self, HelloError::Gone | HelloError::Outdated)
    }
}

/// Same directory as settings/frecency (`%APPDATA%/funke`).
fn session_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("funke")
        .join("vault.session")
}

pub fn has_session() -> bool {
    session_path().is_file()
}

/// Seal the session key and write it. Shows a Hello prompt (the signature *is* the
/// prompt), and on the very first run one more to mint the key.
///
/// Failure means no Hello shortcut — never a weaker one. If the device has no Hello, or
/// the user waves the prompt away, nothing is written and the next unlock asks for the
/// master password again, which is exactly what it did before the feature was enabled.
pub fn save_session(session: &str) -> Result<(), String> {
    let mut sealed = seal(session.as_bytes()).map_err(|e| e.message())?;
    let blob = protect(&sealed);
    sealed.zeroize();
    let blob = blob?;

    let path = session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, blob).map_err(|e| e.to_string())
}

/// Unwrap the stored session key: DPAPI, then a Hello prompt, then AES-GCM. Returned
/// page-locked so the decrypted key can't reach the pagefile while it waits for
/// `bw serve` to boot; every transient it was copied out of is zeroized here.
///
/// A blob that can never be opened again (v1, corrupt, or sealed to a key Hello no longer
/// has) is deleted on the way out and reported as [`HelloError::Gone`] — the caller falls
/// back to the master password, which mints a fresh one. A *cancelled* prompt deletes
/// nothing.
pub fn load_session() -> Result<SecretBuf, HelloError> {
    let raw = fs::read(session_path()).map_err(|_| HelloError::Gone)?;
    let mut blob = match unprotect(&raw) {
        Ok(blob) => blob,
        // Not this account's blob any more, or the file is damaged. Either way it is
        // firewood: DPAPI is the outer wrapper, so nothing inside is reachable.
        Err(_) => return Err(discard(HelloError::Gone)),
    };
    let opened = open(&blob);
    blob.zeroize();

    let mut plain = match opened {
        Ok(plain) => plain,
        Err(e) if e.unopenable() => return Err(discard(e)),
        Err(e) => return Err(e),
    };
    let session = SecretBuf::from_slice(&plain).map_err(HelloError::Failed);
    plain.zeroize();
    session
}

pub fn forget_session() {
    let _ = fs::remove_file(session_path());
}

/// Drop the session *and* the Hello key it was sealed to — the setting was switched off,
/// so leaving a TPM key behind would be litter with the user's name on it.
pub fn forget_all() {
    forget_session();
    keycred::forget();
}

/// Delete an unopenable blob, passing the reason through to the caller unchanged.
fn discard(reason: HelloError) -> HelloError {
    forget_session();
    reason
}

fn seal(plain: &[u8]) -> Result<Vec<u8>, HelloError> {
    let mut challenge = [0u8; CHALLENGE_LEN];
    random(&mut challenge)?;
    let mut nonce = [0u8; NONCE_LEN];
    random(&mut nonce)?;

    let mut key = derive(&challenge, true)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| HelloError::Failed(e.to_string()))?;
    key.zeroize();

    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plain)
        .map_err(|e| HelloError::Failed(format!("sealing the vault session failed: {e}")))?;

    let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    blob.push(VERSION);
    blob.extend_from_slice(&challenge);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

fn open(blob: &[u8]) -> Result<Vec<u8>, HelloError> {
    // A blob written by an older Funke: the session key itself under DPAPI and nothing
    // more. It is readable — that is precisely the problem — and we decline to read it.
    if blob.len() <= HEADER_LEN || blob[0] != VERSION {
        return Err(HelloError::Outdated);
    }
    let challenge = &blob[1..1 + CHALLENGE_LEN];
    let nonce = &blob[1 + CHALLENGE_LEN..HEADER_LEN];
    let ciphertext = &blob[HEADER_LEN..];

    let mut key = derive(challenge, false)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| HelloError::Failed(e.to_string()))?;
    key.zeroize();

    // GCM authenticates: a wrong key fails here rather than yielding garbage. The wrong
    // key means the Hello credential was re-enrolled — the blob is dead, not corrupt.
    cipher
        .decrypt(Nonce::from_slice(nonce), ciphertext)
        .map_err(|_| HelloError::Gone)
}

/// The Hello prompt, then HKDF over what the TPM signed.
fn derive(challenge: &[u8], create: bool) -> Result<[u8; 32], HelloError> {
    let mut signature = keycred::sign(challenge, create)?;
    let mut key = [0u8; 32];
    let result = Hkdf::<Sha256>::new(Some(challenge), &signature)
        .expand(INFO, &mut key)
        .map_err(|e| HelloError::Failed(format!("key derivation failed: {e}")));
    signature.zeroize();
    result.map(|()| key)
}

fn random(bytes: &mut [u8]) -> Result<(), HelloError> {
    getrandom::fill(bytes).map_err(|e| HelloError::Failed(format!("no randomness available: {e}")))
}

fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
        .map_err(|e| format!("DPAPI protect failed: {e}"))?;
        Ok(take_blob(output))
    }
}

fn unprotect(cipher: &[u8]) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: cipher.len() as u32,
        pbData: cipher.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output)
            .map_err(|e| format!("DPAPI unprotect failed: {e}"))?;
        Ok(take_blob(output))
    }
}

/// Copy a DPAPI output blob to owned memory, zeroize the original, and free it.
unsafe fn take_blob(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let slice = std::slice::from_raw_parts_mut(blob.pbData, blob.cbData as usize);
    let bytes = slice.to_vec();
    slice.zeroize();
    let _ = LocalFree(Some(HLOCAL(blob.pbData.cast())));
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything except the Hello prompt itself: the same HKDF + AES-GCM the real path
    /// runs, over a fixed stand-in for what the TPM would sign.
    fn seal_with(signature: &[u8], challenge: [u8; CHALLENGE_LEN], nonce: [u8; NONCE_LEN], plain: &[u8]) -> Vec<u8> {
        let key = key_from(signature, &challenge);
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plain).unwrap();
        let mut blob = vec![VERSION];
        blob.extend_from_slice(&challenge);
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&ciphertext);
        blob
    }

    fn open_with(signature: &[u8], blob: &[u8]) -> Result<Vec<u8>, HelloError> {
        if blob.len() <= HEADER_LEN || blob[0] != VERSION {
            return Err(HelloError::Gone);
        }
        let challenge = &blob[1..1 + CHALLENGE_LEN];
        let key = key_from(signature, challenge);
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        cipher
            .decrypt(
                Nonce::from_slice(&blob[1 + CHALLENGE_LEN..HEADER_LEN]),
                &blob[HEADER_LEN..],
            )
            .map_err(|_| HelloError::Gone)
    }

    fn key_from(signature: &[u8], challenge: &[u8]) -> [u8; 32] {
        let mut key = [0u8; 32];
        Hkdf::<Sha256>::new(Some(challenge), signature)
            .expand(INFO, &mut key)
            .unwrap();
        key
    }

    #[test]
    fn a_sealed_session_round_trips_under_the_same_signature() {
        let session = b"bw-session-key-Ab3+/xyz==";
        let blob = seal_with(b"pretend-tpm-signature", [9; 32], [3; 12], session);
        assert!(!blob.windows(session.len()).any(|w| w == session), "not plaintext");
        assert_eq!(open_with(b"pretend-tpm-signature", &blob).unwrap(), session);
    }

    #[test]
    fn a_different_signature_opens_nothing() {
        // Hello re-enrolled, TPM cleared: the key changed, so the blob is dead. GCM's tag
        // is what turns that into a clean refusal instead of garbage handed to `bw serve`.
        let blob = seal_with(b"the-original-signature", [9; 32], [3; 12], b"bw-session");
        assert_eq!(open_with(b"a-brand-new-signature", &blob), Err(HelloError::Gone));
    }

    #[test]
    fn the_challenge_is_what_makes_two_seals_differ() {
        // Same signature, same secret — the stored challenge (and nonce) still make the
        // blobs unequal, so the file never leaks that the session was unchanged.
        let one = seal_with(b"sig", [1; 32], [1; 12], b"bw-session");
        let two = seal_with(b"sig", [2; 32], [2; 12], b"bw-session");
        assert_ne!(one, two);
        assert_eq!(open_with(b"sig", &one).unwrap(), b"bw-session");
        assert_eq!(open_with(b"sig", &two).unwrap(), b"bw-session");
    }

    #[test]
    fn a_v1_blob_is_refused_rather_than_read() {
        // The old format: the session key itself, DPAPI-wrapped and nothing more. Every
        // byte of base64 is ≥ 0x2B, so it can never be mistaken for a v2 header — and it
        // must not be, because reading it would be the weakness we just removed.
        let v1 = b"4kx9AbCdEf0123456789+/==".to_vec();
        assert_ne!(v1[0], VERSION);
        assert_eq!(open(&v1), Err(HelloError::Outdated));
    }

    #[test]
    fn a_truncated_blob_does_not_panic() {
        for length in 0..=HEADER_LEN {
            let blob = vec![VERSION; length];
            assert_eq!(open(&blob), Err(HelloError::Outdated), "length {length}");
        }
    }

    #[test]
    fn only_a_dead_session_gets_deleted() {
        // The cancel case is the one that would hurt: waving away the prompt must not
        // wipe a working Hello setup and send the user back to their master password
        // forever after.
        assert!(HelloError::Gone.unopenable());
        assert!(HelloError::Outdated.unopenable());
        assert!(!HelloError::Cancelled.unopenable());
        assert!(!HelloError::Unsupported.unopenable());
        assert!(!HelloError::Failed("device is on fire".into()).unopenable());
    }

    #[test]
    fn every_error_says_something_a_person_can_act_on() {
        for error in [
            HelloError::Unsupported,
            HelloError::Cancelled,
            HelloError::Gone,
            HelloError::Outdated,
            HelloError::Failed("device is on fire".into()),
        ] {
            let message = error.message();
            assert!(!message.is_empty());
            // A catalogue miss leaks the key itself — that would ship "vault.hello.reset"
            // to the overlay as the explanation.
            assert!(!message.starts_with("vault."), "{message}");
        }
    }

    #[test]
    fn dpapi_round_trips_for_the_current_user() {
        let secret = b"BW_SESSION-test-value";
        let cipher = protect(secret).expect("protect");
        assert_ne!(cipher.as_slice(), secret, "blob must not be plaintext");
        assert_eq!(unprotect(&cipher).expect("unprotect"), secret);
    }
}
