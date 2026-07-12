//! Which `bw` gets run — and whether Bitwarden is the one who signed it.
//!
//! Every vault child used to be spawned as `Command::new("bw")`, which asks Windows to walk
//! `PATH` afresh on each spawn. The master password is handed to that binary in its
//! environment (`unlock_raw`), so anything that lands a `bw.exe` in a directory earlier in
//! `PATH` than the real one — a writable folder on a shared machine, a dropper with no
//! privileges at all — gets the password. Resolving *once*, to an absolute path, closes it:
//! whatever `bw` we found at startup is the `bw` we keep talking to.
//!
//! On top of the pin, an Authenticode check answers a different question — not "is this the
//! file we found" but "is this Bitwarden's file". Policy is deliberately **warn-first**: an
//! `npm -g install @bitwarden/cli` puts a `.cmd` wrapper around a Node script and nothing
//! there is signed at all, and that is a legitimate, common install that must keep working.
//! So an unverified CLI is used, said out loud on the vault's status row, and refused only
//! if the user asks for that (`Settings::vault_require_signed_cli`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What we found and what we make of it.
pub struct ResolvedCli {
    pub path: PathBuf,
    pub trust: Trust,
}

/// How much the binary at that path can prove about itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Trust {
    /// Authenticode verifies and the certificate says Bitwarden.
    Bitwarden,
    /// A signature that verifies, but somebody else's. The interesting failure: an attacker
    /// with any cheap code-signing certificate lands here, not in [`Trust::Unsigned`], which
    /// is exactly why the signer's *name* is checked and not merely the signature's validity.
    OtherSigner(String),
    /// No valid Authenticode signature.
    Unsigned,
    /// A `.cmd`/`.bat` wrapper — the npm shape. Authenticode does not apply to a text file,
    /// so there is nothing here to verify and nothing to hold against it.
    Shim,
}

impl Trust {
    /// Bitwarden signed this, and Windows agrees.
    pub fn verified(&self) -> bool {
        matches!(self, Trust::Bitwarden)
    }

    /// What the status row says about it (a catalogue key), or `None` when there is nothing
    /// worth saying.
    pub fn note(&self) -> Option<&'static str> {
        match self {
            Trust::Bitwarden => None,
            Trust::OtherSigner(_) => Some("vault.cli.other_signer"),
            Trust::Unsigned => Some("vault.cli.unsigned"),
            Trust::Shim => Some("vault.cli.shim"),
        }
    }
}

/// The pinned CLI, resolved on first use and never again.
///
/// Once per process is the point: re-resolving would hand a `PATH` edited mid-session the
/// very opening the pin exists to close.
pub fn cli() -> Option<&'static ResolvedCli> {
    static CLI: OnceLock<Option<ResolvedCli>> = OnceLock::new();
    CLI.get_or_init(|| {
        let resolved = resolve();
        match &resolved {
            Some(cli) => {
                if let Some(note) = cli.trust.note() {
                    eprintln!(
                        "warning: {} is not signature-verified as Bitwarden's ({note}) — it will be used anyway; \
                         enable \"require a signed CLI\" in Settings to refuse instead",
                        cli.path.display()
                    );
                }
            }
            None => eprintln!("no bw CLI found"),
        }
        resolved
    })
    .as_ref()
}

/// Where a Bitwarden CLI actually lives, most-specific installer first, `PATH` last.
///
/// The order is the point: the known install locations are checked *before* `PATH`, so a
/// planted `bw.exe` sitting early in `PATH` loses to a real winget/Program Files install.
fn resolve() -> Option<ResolvedCli> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let env = |key: &str| std::env::var_os(key).map(PathBuf::from);

    if let Some(dir) = env("LOCALAPPDATA") {
        candidates.push(dir.join(r"Microsoft\WinGet\Links\bw.exe"));
        // Where the official installer actually lands for a per-user install — the common
        // case, and the one this list first missed. It was being found by the PATH walk
        // below instead, which is the very lookup the pin exists to stop trusting.
        candidates.push(dir.join(r"Programs\Bitwarden CLI\bw.exe"));
    }
    if let Some(dir) = env("ProgramFiles") {
        candidates.push(dir.join(r"Bitwarden CLI\bw.exe"));
    }
    if let Some(dir) = env("USERPROFILE") {
        candidates.push(dir.join(r"scoop\shims\bw.exe"));
    }
    // npm's global install, read straight from the directory rather than by shelling out to
    // `npm prefix -g` — asking npm where it lives means running node to find out.
    if let Some(dir) = env("APPDATA") {
        candidates.push(dir.join(r"npm\bw.cmd"));
    }
    // The PATH walk, done here rather than by handing the name to `Command` — `where`-like
    // semantics, but we get to keep the absolute answer.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["bw.exe", "bw.cmd", "bw.bat"] {
                candidates.push(dir.join(name));
            }
        }
    }

    let path = candidates.into_iter().find(|path| path.is_file())?;
    let trust = trust_of(&path);
    Some(ResolvedCli { path, trust })
}

fn trust_of(path: &Path) -> Trust {
    let shim = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));
    if shim {
        return Trust::Shim;
    }
    signer_of(path)
}

/// Authenticode, via `WinVerifyTrust`, plus the signer's common name.
///
/// The name is the load-bearing half. `WinVerifyTrust` alone answers "did *somebody* Windows
/// trusts sign this", and somebody includes whoever bought a certificate this morning.
///
/// Only **embedded** signatures count: this is a file check, not a catalog lookup, so a
/// binary whose signature lives in a system catalog (as most of Windows' own do — a bare
/// `notepad.exe` reads as unsigned here) cannot pass. Bitwarden's `bw.exe` is
/// embedded-signed, and under the warn-first policy a wrong answer costs a note on a row,
/// never a working vault.
#[cfg(windows)]
fn signer_of(path: &Path) -> Trust {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_SAFER_FLAG, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut file },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_SAFER_FLAG,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;

    // SAFETY: `data` and `file` outlive both calls, and the VERIFY call is always paired with
    // the CLOSE below — leaving the state open leaks the provider's chain.
    let trust = unsafe {
        let status = WinVerifyTrust(
            HWND::default(),
            &mut action,
            &mut data as *mut _ as *mut std::ffi::c_void,
        );
        if status == 0 {
            match signer_name(data.hWVTStateData) {
                Some(name) if name.to_lowercase().contains("bitwarden") => Trust::Bitwarden,
                Some(name) => Trust::OtherSigner(name),
                None => Trust::Unsigned,
            }
        } else {
            Trust::Unsigned
        }
    };

    data.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            HWND::default(),
            &mut action,
            &mut data as *mut _ as *mut std::ffi::c_void,
        );
    }
    trust
}

/// The signing certificate's display name, out of the chain `WinVerifyTrust` just walked.
///
/// # Safety
/// `state` must be the `hWVTStateData` of a `WINTRUST_DATA` whose VERIFY call succeeded and
/// which has not been closed yet.
#[cfg(windows)]
unsafe fn signer_name(state: windows::Win32::Foundation::HANDLE) -> Option<String> {
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    use windows::Win32::Security::WinTrust::{
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData,
    };

    let provider = WTHelperProvDataFromStateData(state);
    if provider.is_null() {
        return None;
    }
    let signer = WTHelperGetProvSignerFromChain(provider, 0, false, 0);
    if signer.is_null() {
        return None;
    }
    let cert = WTHelperGetProvCertFromChain(signer, 0);
    if cert.is_null() {
        return None;
    }
    let context = (*cert).pCert;
    if context.is_null() {
        return None;
    }

    let len = CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, None);
    if len <= 1 {
        return None;
    }
    let mut buffer = vec![0u16; len as usize];
    let written = CertGetNameStringW(context, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buffer));
    if written <= 1 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..written as usize - 1]))
}

#[cfg(not(windows))]
fn signer_of(_path: &Path) -> Trust {
    Trust::Unsigned
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this machine actually resolves to, and what WinTrust makes of it.
    ///
    /// Machine-dependent, so it never runs in CI — but a chain walk through raw pointers is
    /// the kind of FFI that compiles perfectly and then quietly returns nothing, and this is
    /// the only thing that says otherwise. On a machine with the real CLI installed it must
    /// print `Bitwarden`:
    /// `cargo test -p funke-vault -- --ignored --nocapture bw_on_this_machine`
    #[test]
    #[ignore = "depends on what is installed on this machine"]
    fn bw_on_this_machine() {
        match resolve() {
            Some(cli) => println!("resolved {} → {:?}", cli.path.display(), cli.trust),
            None => println!("no bw CLI installed here"),
        }
    }

    #[test]
    fn shims_are_recognized_and_never_asked_to_prove_a_signature() {
        // A text wrapper has nothing to verify — classifying it by extension is what keeps
        // the Authenticode path from being asked an impossible question.
        let dir = std::env::temp_dir().join("funke-cli-kind");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, expected) in [
            ("bw.cmd", Trust::Shim),
            ("bw.bat", Trust::Shim),
            ("BW.CMD", Trust::Shim),
        ] {
            let path = dir.join(name);
            std::fs::write(&path, b"@echo off\n").unwrap();
            assert_eq!(trust_of(&path), expected, "{name}");
        }
        // Anything else goes to the signature check, and a file that is not a PE cannot pass.
        let exe = dir.join("bw.exe");
        std::fs::write(&exe, b"not a pe").unwrap();
        assert_eq!(trust_of(&exe), Trust::Unsigned);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_bitwardens_own_signature_counts_as_verified() {
        assert!(Trust::Bitwarden.verified());
        assert!(!Trust::Unsigned.verified());
        assert!(!Trust::Shim.verified());
        // The one that matters: a valid signature by someone else is *not* good enough.
        assert!(!Trust::OtherSigner("Contoso Ltd".into()).verified());

        assert!(Trust::Bitwarden.note().is_none(), "nothing to say about the good case");
        assert!(Trust::Shim.note().is_some());
    }
}
