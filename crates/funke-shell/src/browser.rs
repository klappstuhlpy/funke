//! Default-browser lookup: which executable handles https, and its icon.
//!
//! The user's pick lives in the registry (`UrlAssociations\https\UserChoice` → ProgId);
//! that ProgId's `shell\open\command` names the executable, whose icon
//! [`icon_data_url`](crate::icon_data_url) extracts like any other shell item.

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

/// Icon of the default https handler (the user's browser) as a data URL.
pub fn default_browser_icon() -> Option<String> {
    crate::icon_data_url(&default_browser_exe()?)
}

/// Absolute path of the executable registered for https URLs.
pub fn default_browser_exe() -> Option<String> {
    let progid = read_reg_sz(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\https\UserChoice",
        "ProgId",
    )?;
    let command = read_reg_sz(HKEY_CLASSES_ROOT, &format!(r"{progid}\shell\open\command"), "")?;
    command_exe(&command)
}

/// First token of a shell command line: a quoted path, or everything up to a space.
fn command_exe(command: &str) -> Option<String> {
    let command = command.trim();
    let exe = match command.strip_prefix('"') {
        Some(rest) => rest.split('"').next()?,
        None => command.split_whitespace().next()?,
    };
    (!exe.is_empty()).then(|| exe.to_string())
}

fn read_reg_sz(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let subkey_w: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let value_w: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut bytes: u32 = 0;
        RegGetValueW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut bytes),
        )
        .ok()
        .ok()?;
        let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
        RegGetValueW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            PCWSTR(value_w.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
        .ok()
        .ok()?;
        Some(String::from_utf16_lossy(&buf).trim_end_matches('\0').to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_lines_yield_their_executable() {
        assert_eq!(
            command_exe(r#""C:\Program Files\Firefox\firefox.exe" -osint -url "%1""#).as_deref(),
            Some(r"C:\Program Files\Firefox\firefox.exe")
        );
        assert_eq!(
            command_exe(r"C:\Windows\explorer.exe %1").as_deref(),
            Some(r"C:\Windows\explorer.exe")
        );
        assert_eq!(command_exe(""), None);
    }

    #[test]
    fn this_machine_resolves_a_default_browser() {
        // Fresh profiles (CI images) may have no UserChoice key — None is fine there;
        // when a handler is registered, the resolved token must be an executable path.
        if let Some(exe) = default_browser_exe() {
            assert!(exe.to_ascii_lowercase().ends_with(".exe"), "unexpected handler: {exe}");
        }
    }
}
