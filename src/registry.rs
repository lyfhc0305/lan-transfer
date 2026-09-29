//! Values under HKEY_CURRENT_USER (Windows).
use std::os::windows::ffi::OsStrExt;

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// Set a string value under HKEY_CURRENT_USER.
pub fn set(key: &str, name: &str, value: &str) -> bool {
    let (key, name, value) = (wide(key), wide(name), wide(value));
    // SAFETY: all strings are NUL-terminated UTF-16 and live for the call.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        ) == 0
    }
}

/// Remove a value under HKEY_CURRENT_USER.
pub fn delete(key: &str, name: &str) {
    let (key, name) = (wide(key), wide(name));
    // SAFETY: both strings are NUL-terminated UTF-16 and live for the call.
    unsafe {
        RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
    }
}

const HKEY_CURRENT_USER: isize = 0x8000_0001_u32 as i32 as isize;
const REG_SZ: u32 = 1;

#[link(name = "advapi32")]
extern "system" {
    fn RegSetKeyValueW(
        key: isize,
        sub_key: *const u16,
        value_name: *const u16,
        kind: u32,
        data: *const std::ffi::c_void,
        size: u32,
    ) -> i32;
    fn RegDeleteKeyValueW(key: isize, sub_key: *const u16, value_name: *const u16) -> i32;
}
