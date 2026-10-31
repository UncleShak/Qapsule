//! Start with Windows: one value under the current user's `Run` key. HKCU only,
//! so no admin is needed, and nothing outside that one value is touched.

use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::{PCWSTR, w};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("Qapsule");

/// The running exe, quoted as a command line.
fn wanted() -> Result<String, &'static str> {
    let exe = std::env::current_exe().map_err(|_| "couldn't find the app's path")?;
    Ok(quoted(&exe.to_string_lossy()))
}

fn quoted(path: &str) -> String {
    format!("\"{path}\"")
}

/// Whether the value exists and points at this exe.
pub fn is_enabled() -> bool {
    let Ok(wanted) = wanted() else {
        return false;
    };
    read(RUN_KEY, VALUE).is_some_and(|v| v.eq_ignore_ascii_case(&wanted))
}

/// Turns Start with Windows on (writes the value) or off (deletes it).
pub fn set_enabled(on: bool) -> Result<(), &'static str> {
    if on {
        write(RUN_KEY, VALUE, &wanted()?)
    } else {
        remove(RUN_KEY, VALUE)
    }
}

fn read(key: PCWSTR, name: PCWSTR) -> Option<String> {
    let mut buf = vec![0u16; 1024];
    loop {
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` is valid for `bytes` bytes and outlives the call; the key
        // and name are NUL-terminated wide strings.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if status == ERROR_MORE_DATA {
            buf = vec![0u16; (bytes as usize).div_ceil(2) + 1];
            continue;
        }
        if status != ERROR_SUCCESS {
            return None;
        }
        let units = (bytes as usize / 2).min(buf.len());
        let text = &buf[..units];
        let end = text.iter().position(|&c| c == 0).unwrap_or(units);
        return Some(String::from_utf16_lossy(&text[..end]));
    }
}

fn write(key: PCWSTR, name: PCWSTR, text: &str) -> Result<(), &'static str> {
    let data: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `data` is valid for the given byte length (including the NUL) for the call.
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            REG_SZ.0,
            Some(data.as_ptr().cast()),
            (data.len() * 2) as u32,
        )
    };
    if status == ERROR_SUCCESS {
        Ok(())
    } else {
        Err("couldn't write the startup entry")
    }
}

fn remove(key: PCWSTR, name: PCWSTR) -> Result<(), &'static str> {
    // SAFETY: the key and name are NUL-terminated wide strings.
    let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name) };
    // Already gone is the state we wanted.
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        Ok(())
    } else {
        Err("couldn't remove the startup entry")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Registry::RegDeleteKeyW;

    #[test]
    fn the_command_is_the_quoted_path() {
        assert_eq!(
            quoted(r"C:\Program Files\Q\q.exe"),
            r#""C:\Program Files\Q\q.exe""#
        );
    }

    #[test]
    fn a_value_round_trips_and_is_removed() {
        let key_name =
            windows::core::HSTRING::from(format!(r"Software\Qapsule-test-{}", std::process::id()));
        let key = PCWSTR(key_name.as_ptr());
        let name = w!("Probe");
        let long = format!("\"{}\"", "x".repeat(3000));

        remove(key, name).unwrap();
        assert_eq!(read(key, name), None);
        write(key, name, r#""C:\a b\c.exe""#).unwrap();
        assert_eq!(read(key, name).as_deref(), Some(r#""C:\a b\c.exe""#));
        write(key, name, &long).unwrap();
        assert_eq!(read(key, name).as_deref(), Some(long.as_str()));
        remove(key, name).unwrap();
        assert_eq!(read(key, name), None);
        remove(key, name).unwrap();
        // SAFETY: a NUL-terminated key name.
        let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, key) };
    }
}
