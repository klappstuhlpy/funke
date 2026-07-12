//! Page-locked storage for a secret at rest: memory that cannot be swapped to the
//! pagefile while the secret lives, zeroized and freed when it drops.
//!
//! `zeroize` bounds a secret's *lifetime*; it says nothing about where the bytes sat in
//! the meantime — a page holding the Windows Hello session key could be swapped out
//! minutes before the zeroize runs, and a pagefile is an artifact at rest. `VirtualLock`
//! closes that path for the one place a vault secret genuinely *rests* in this process.
//!
//! Honest limits, so nobody mistakes this for a vault: bytes that passed through other
//! buffers on their way in (a DPAPI output, a serde parse) existed unlocked for that
//! moment, and the callers zeroize those transients but cannot lock them. A failing
//! `VirtualLock` (working-set quota) degrades to plain-but-zeroized memory with a logged
//! warning — the buffer is a hardening layer, never a gate.

use std::ffi::c_void;
use std::ptr::NonNull;

use windows::Win32::System::Memory::{
    VirtualAlloc, VirtualFree, VirtualLock, VirtualUnlock, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use zeroize::Zeroize;

pub struct SecretBuf {
    ptr: NonNull<u8>,
    len: usize,
    /// What was actually allocated (page-rounded by the kernel; freed whole).
    alloc: usize,
    locked: bool,
}

// The buffer is owned, never aliased, and the kernel doesn't care which thread frees it.
unsafe impl Send for SecretBuf {}

impl SecretBuf {
    /// Copy `bytes` into freshly allocated, page-locked memory. The caller zeroizes the
    /// source — this type can only vouch for its own pages.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, String> {
        let alloc = bytes.len().max(1);
        unsafe {
            let raw = VirtualAlloc(None, alloc, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
            let ptr = NonNull::new(raw as *mut u8).ok_or("VirtualAlloc failed")?;
            let locked = match VirtualLock(raw, alloc) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("warning: secret buffer is not page-locked (it may reach the pagefile): {e}");
                    false
                }
            };
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.as_ptr(), bytes.len());
            Ok(Self {
                ptr,
                len: bytes.len(),
                alloc,
                locked,
            })
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }

    /// The secret as text (`bw` session keys are printable ASCII).
    pub fn as_str(&self) -> Result<&str, String> {
        std::str::from_utf8(self.as_bytes()).map_err(|_| "secret is not valid UTF-8".into())
    }
}

impl Drop for SecretBuf {
    fn drop(&mut self) {
        unsafe {
            // Volatile writes the compiler can't elide, then unlock, then release.
            std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.len).zeroize();
            let raw = self.ptr.as_ptr() as *mut c_void;
            if self.locked {
                let _ = VirtualUnlock(raw, self.alloc);
            }
            let _ = VirtualFree(raw, 0, MEM_RELEASE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_reports_text() {
        let buf = SecretBuf::from_slice(b"BW_SESSION_abc123==").unwrap();
        assert_eq!(buf.as_bytes(), b"BW_SESSION_abc123==");
        assert_eq!(buf.as_str().unwrap(), "BW_SESSION_abc123==");
    }

    #[test]
    fn empty_secrets_are_representable() {
        let buf = SecretBuf::from_slice(b"").unwrap();
        assert_eq!(buf.as_bytes(), b"");
    }

    #[test]
    fn non_utf8_is_reported_not_lossy() {
        let buf = SecretBuf::from_slice(&[0xFF, 0xFE]).unwrap();
        assert!(buf.as_str().is_err());
    }
}
