//! "Start with Windows": a value under HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
//! Only the current user is touched; no admin rights needed.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub const VALUE_NAME: &str = "Strom-Widget";

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<HKEY> {
    let mut key = HKEY::default();
    let sub = HSTRING::from(RUN_KEY);
    let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()), 0, access, &mut key) };
    if rc == ERROR_SUCCESS {
        Some(key)
    } else {
        None
    }
}

/// Opens the Run key for writing and creates it if it is missing, as on a fresh user profile.
fn open_for_write() -> Option<HKEY> {
    let mut key = HKEY::default();
    let sub = HSTRING::from(RUN_KEY);
    let rc = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
    };
    (rc == ERROR_SUCCESS).then_some(key)
}

/// True if a Run value with this name exists.
pub fn is_enabled_named(name: &str) -> bool {
    let Some(key) = open(KEY_READ) else {
        return false;
    };
    let n = HSTRING::from(name);
    let rc = unsafe { RegQueryValueExW(key, PCWSTR(n.as_ptr()), None, None, None, None) };
    unsafe {
        let _ = RegCloseKey(key);
    }
    rc == ERROR_SUCCESS
}

/// Writes (enable) or removes (disable) the Run value pointing at `exe`.
pub fn set_named(name: &str, enable: bool, exe: &str) -> bool {
    let Some(key) = open_for_write() else {
        return false;
    };
    let n = HSTRING::from(name);
    let ok = unsafe {
        if enable {
            let cmd = format!("\"{}\"", exe);
            let wide: Vec<u16> = cmd.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes = std::slice::from_raw_parts(wide.as_ptr() as *const u8, wide.len() * 2);
            RegSetValueExW(key, PCWSTR(n.as_ptr()), 0, REG_SZ, Some(bytes)) == ERROR_SUCCESS
        } else {
            let rc = RegDeleteValueW(key, PCWSTR(n.as_ptr()));
            rc == ERROR_SUCCESS || !is_enabled_named(name)
        }
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    ok
}

pub fn is_enabled() -> bool {
    is_enabled_named(VALUE_NAME)
}

/// Enable or disable autostart for the running executable.
pub fn set(enable: bool) -> bool {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    if enable && exe.is_empty() {
        return false;
    }
    set_named(VALUE_NAME, enable, &exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_test_value() {
        let name = "Strom-Widget-SelfTest";
        assert!(set_named(name, true, r"C:\nowhere\test.exe"));
        assert!(is_enabled_named(name));
        assert!(set_named(name, false, ""));
        assert!(!is_enabled_named(name));
    }
}
