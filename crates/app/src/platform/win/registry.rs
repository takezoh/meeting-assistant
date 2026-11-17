//! Reading and writing the current user's registry (HKEY_CURRENT_USER).

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW,
    RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR};

fn check(code: WIN32_ERROR) -> anyhow::Result<()> {
    if code == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(anyhow::Error::new(windows::core::Error::from_hresult(
            code.to_hresult(),
        )))
    }
}

fn check_ignoring_missing(code: WIN32_ERROR) -> anyhow::Result<()> {
    if code == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        check(code)
    }
}

fn name_or_default(name: Option<&str>) -> Option<HSTRING> {
    name.map(HSTRING::from)
}

/// Writes a string value, creating the key if needed. If `name` is `None`, writes the default value.
pub fn set_string(subkey: &str, name: Option<&str>, value: &str) -> anyhow::Result<()> {
    unsafe {
        let mut key = HKEY::default();
        check(RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(subkey),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut key,
            None,
        ))?;
        // Pass the string as bytes including the terminating NUL.
        let wide: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let bytes = std::slice::from_raw_parts(wide.as_ptr().cast::<u8>(), wide.len() * 2);
        let name = name_or_default(name);
        let result = RegSetValueExW(
            key,
            name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr())),
            None,
            REG_SZ,
            Some(bytes),
        );
        let _ = RegCloseKey(key);
        check(result)
    }
}

/// Reads a string value. `None` if absent.
pub fn get_string(subkey: &str, name: Option<&str>) -> Option<String> {
    unsafe {
        let subkey = HSTRING::from(subkey);
        let name = name_or_default(name);
        let name = name.as_ref().map_or(PCWSTR::null(), |n| PCWSTR(n.as_ptr()));
        let mut size = 0u32;
        check(RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            name,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        ))
        .ok()?;
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        check(RegGetValueW(
            HKEY_CURRENT_USER,
            &subkey,
            name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        ))
        .ok()?;
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

/// Deletes one value. Not an error if it does not exist.
pub fn delete_value(subkey: &str, name: &str) -> anyhow::Result<()> {
    check_ignoring_missing(unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(subkey), &HSTRING::from(name))
    })
}

/// Deletes a key together with everything under it. Not an error if it does not exist.
pub fn delete_tree(subkey: &str) -> anyhow::Result<()> {
    check_ignoring_missing(unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(subkey)) })
}
