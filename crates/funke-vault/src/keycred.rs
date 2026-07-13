//! Windows Hello as *key material* — a TPM-held key that only signs after Hello verifies
//! the user.
//!
//! What this replaces is worth spelling out, because the two look identical on screen.
//! `UserConsentVerifier` (the old path) is a **presence check**: it shows a Hello prompt
//! and reports whether a human passed it. The answer is advice. The program is free to
//! ignore it, and so is anything that never asks — the secret it guarded was protected by
//! DPAPI alone, so any code running as the user could simply decrypt it and skip the
//! prompt entirely.
//!
//! `KeyCredentialManager` mints a key pair whose private half lives in the TPM and will
//! only sign after Hello succeeds. Derive the encryption key from that signature and the
//! prompt stops being a question the program may ignore: no Hello, no signature; no
//! signature, no key. That is the whole point of S2 — the prompt becomes load-bearing.
//!
//! It works because RSA PKCS#1 v1.5 signatures are **deterministic**: the same challenge
//! always produces the same signature, so a stored challenge reproduces the same key
//! forever (until Hello is reset, which is exactly when the old key *should* die). The
//! scheme is KeePassXC's; the reasoning is theirs and it is sound.

use windows::core::HSTRING;
use windows::Security::Credentials::{
    KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Storage::Streams::{DataReader, DataWriter};

use crate::hello::HelloError;

/// Funke's key in the user's credential store. Stable — it is what a v2 blob is sealed to.
const CREDENTIAL: &str = "funke-vault-hello";

fn failed(context: &str, e: windows::core::Error) -> HelloError {
    HelloError::Failed(format!("{context}: {e}"))
}

/// Can this device hold a Hello-gated key at all? False on a machine with no PIN or
/// biometric enrolled — where the whole feature is unavailable, not merely inconvenient.
pub fn supported() -> bool {
    KeyCredentialManager::IsSupportedAsync()
        .and_then(|op| op.get())
        .unwrap_or(false)
}

/// Sign `challenge` with Funke's Hello key — **this is the Hello prompt**. `create` opens
/// the enable path (mint the key if it isn't there yet); without it, a missing key is
/// [`HelloError::Gone`] rather than a fresh key that could never open the old blob.
pub fn sign(challenge: &[u8], create: bool) -> Result<Vec<u8>, HelloError> {
    if !supported() {
        return Err(HelloError::Unsupported);
    }
    let credential = open(create)?;
    let data = to_buffer(challenge)?;
    let operation = credential
        .RequestSignAsync(&data)
        .map_err(|e| failed("Windows Hello could not sign", e))?
        .get()
        .map_err(|e| failed("Windows Hello could not sign", e))?;
    match operation.Status() {
        Ok(KeyCredentialStatus::Success) => {}
        Ok(other) => return Err(status_error(other)),
        Err(e) => return Err(failed("Windows Hello could not sign", e)),
    }
    let signature = operation
        .Result()
        .map_err(|e| failed("Windows Hello returned no signature", e))?;
    from_buffer(&signature)
}

/// Delete Funke's key. Best-effort: it is the tail of switching the setting off, and a
/// key we cannot delete is a key nothing is sealed to any more anyway.
pub fn forget() {
    if let Ok(operation) = KeyCredentialManager::DeleteAsync(&HSTRING::from(CREDENTIAL)) {
        let _ = operation.get();
    }
}

fn open(create: bool) -> Result<KeyCredential, HelloError> {
    let name = HSTRING::from(CREDENTIAL);
    let result = if create {
        let created = KeyCredentialManager::RequestCreateAsync(&name, KeyCredentialCreationOption::FailIfExists)
            .map_err(|e| failed("Windows Hello key creation failed", e))?
            .get()
            .map_err(|e| failed("Windows Hello key creation failed", e))?;
        // Already minted by an earlier unlock — the expected case after the first one.
        // Re-creating would replace the key and orphan every blob sealed to it.
        match created.Status() {
            Ok(KeyCredentialStatus::CredentialAlreadyExists) => open_existing(&name)?,
            _ => created,
        }
    } else {
        open_existing(&name)?
    };

    match result.Status() {
        Ok(KeyCredentialStatus::Success) => result
            .Credential()
            .map_err(|e| failed("Windows Hello key is unreadable", e)),
        Ok(other) => Err(status_error(other)),
        Err(e) => Err(failed("Windows Hello key is unreadable", e)),
    }
}

fn open_existing(name: &HSTRING) -> Result<windows::Security::Credentials::KeyCredentialRetrievalResult, HelloError> {
    KeyCredentialManager::OpenAsync(name)
        .map_err(|e| failed("Windows Hello key could not be opened", e))?
        .get()
        .map_err(|e| failed("Windows Hello key could not be opened", e))
}

fn status_error(status: KeyCredentialStatus) -> HelloError {
    match status {
        KeyCredentialStatus::NotFound => HelloError::Gone,
        KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => HelloError::Cancelled,
        other => HelloError::Failed(format!("Windows Hello reported status {}", other.0)),
    }
}

fn to_buffer(bytes: &[u8]) -> Result<windows::Storage::Streams::IBuffer, HelloError> {
    let writer = DataWriter::new().map_err(|e| failed("buffer", e))?;
    writer.WriteBytes(bytes).map_err(|e| failed("buffer", e))?;
    writer.DetachBuffer().map_err(|e| failed("buffer", e))
}

fn from_buffer(buffer: &windows::Storage::Streams::IBuffer) -> Result<Vec<u8>, HelloError> {
    let length = buffer.Length().map_err(|e| failed("buffer", e))? as usize;
    let reader = DataReader::FromBuffer(buffer).map_err(|e| failed("buffer", e))?;
    let mut bytes = vec![0u8; length];
    reader.ReadBytes(&mut bytes).map_err(|e| failed("buffer", e))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_is_not_a_reset() {
        // The one distinction the caller acts on: `Gone` deletes the stored session,
        // everything else leaves it alone. Getting this backwards would wipe a working
        // Hello setup every time somebody hit Escape on the prompt.
        assert_eq!(status_error(KeyCredentialStatus::UserCanceled), HelloError::Cancelled);
        assert_eq!(
            status_error(KeyCredentialStatus::UserPrefersPassword),
            HelloError::Cancelled
        );
        assert_eq!(status_error(KeyCredentialStatus::NotFound), HelloError::Gone);
        assert!(matches!(
            status_error(KeyCredentialStatus::SecurityDeviceLocked),
            HelloError::Failed(_)
        ));
    }

    /// Does this machine have a Hello credential store? Shows no prompt — it is the one
    /// piece of the WinRT path that can be exercised without a human, and it proves the
    /// activation factory resolves at all (a binding that compiles is not a binding that
    /// runs). `cargo test -p funke-vault hello_support_here -- --ignored --nocapture`
    #[test]
    #[ignore = "machine-dependent; run by hand"]
    fn hello_support_here() {
        println!("KeyCredentialManager::IsSupportedAsync -> {}", supported());
    }

    /// The real thing, on a machine with Hello set up:
    /// `cargo test -p funke-vault hello_on_this_machine -- --ignored --nocapture`
    /// Prompts twice on first run (create, then sign) — that is the flow, not a bug.
    #[test]
    #[ignore = "shows a Windows Hello prompt; run by hand"]
    fn hello_on_this_machine() {
        assert!(supported(), "no Hello credential store on this device");
        let challenge = [7u8; 32];
        let first = sign(&challenge, true).expect("sign");
        let second = sign(&challenge, false).expect("sign again");
        // Determinism is the whole scheme: a different signature each time would mean a
        // different key each time, and the stored session could never be reopened.
        assert_eq!(first, second, "Hello signatures must be deterministic");
        println!("signed {} bytes with the Hello key", first.len());
    }
}
