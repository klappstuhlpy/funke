//! Windows Hello unlock support: DPAPI-protected persistence of a `bw` session key,
//! gated behind a Hello consent prompt (opt-in via `Settings::vault_hello`).
//!
//! The stored blob lets repeat unlocks skip the master password: after Hello verifies
//! the user, the decrypted session key boots `bw serve` pre-unlocked. DPAPI binds the
//! blob to the signed-in Windows account; the Hello prompt is a user-presence gate on
//! top, not an extra encryption layer — the tradeoff is documented in SECURITY.md.

use std::fs;
use std::path::PathBuf;

use windows::core::{factory, HSTRING, PCWSTR};
use windows::Security::Credentials::UI::{
    UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
};
use windows::Win32::Foundation::{LocalFree, HLOCAL, HWND};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};
use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
use windows_future::IAsyncOperation;
use zeroize::Zeroize;

use crate::secret_buf::SecretBuf;

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

pub fn save_session(session: &str) -> Result<(), String> {
    let blob = protect(session.as_bytes())?;
    let path = session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, blob).map_err(|e| e.to_string())
}

/// Decrypt the stored session key. Call only after [`verify`] succeeded — DPAPI itself
/// doesn't prompt. Returned page-locked so the decrypted key can't reach the pagefile
/// while it waits for `bw serve` to boot; the DPAPI output it was copied from is
/// zeroized here (it existed unlocked for that moment — see [`crate::secret_buf`]).
pub fn load_session() -> Result<SecretBuf, String> {
    let raw = fs::read(session_path()).map_err(|_| "no stored vault session")?;
    let mut plain = unprotect(&raw)?;
    if std::str::from_utf8(&plain).is_err() {
        plain.zeroize();
        return Err("stored vault session is corrupt".into());
    }
    let session = SecretBuf::from_slice(&plain);
    plain.zeroize();
    session
}

pub fn forget_session() {
    let _ = fs::remove_file(session_path());
}

/// Show the Windows Hello consent prompt (face/fingerprint/PIN), parented to `hwnd`.
pub fn verify(hwnd: isize, message: &str) -> Result<(), String> {
    let availability = UserConsentVerifier::CheckAvailabilityAsync()
        .and_then(|op| op.get())
        .map_err(|e| format!("Windows Hello availability check failed: {e}"))?;
    if availability != UserConsentVerifierAvailability::Available {
        return Err("Windows Hello isn't set up on this device — use the master password".into());
    }
    // Desktop apps must go through the interop factory so the dialog gets a parent HWND.
    let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
        .map_err(|e| format!("Windows Hello is unavailable: {e}"))?;
    let operation: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        interop
            .RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &HSTRING::from(message))
            .map_err(|e| format!("Windows Hello prompt failed: {e}"))?
    };
    match operation.get() {
        Ok(UserConsentVerificationResult::Verified) => Ok(()),
        Ok(UserConsentVerificationResult::Canceled) => Err("Windows Hello was cancelled".into()),
        Ok(other) => Err(format!("Windows Hello did not verify (result {})", other.0)),
        Err(e) => Err(format!("Windows Hello verification failed: {e}")),
    }
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

    #[test]
    fn dpapi_round_trips_for_the_current_user() {
        let secret = b"BW_SESSION-test-value";
        let cipher = protect(secret).expect("protect");
        assert_ne!(cipher.as_slice(), secret, "blob must not be plaintext");
        assert_eq!(unprotect(&cipher).expect("unprotect"), secret);
    }
}
