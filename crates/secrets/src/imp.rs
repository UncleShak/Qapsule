use std::io;

use windows::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{PCWSTR, PWSTR};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// The last OS error, as a plain `io::Error` holding only the code.
fn last_error() -> io::Error {
    io::Error::from_raw_os_error(unsafe { GetLastError() }.0 as i32)
}

fn is_not_found(error: &io::Error) -> bool {
    error.raw_os_error() == Some(ERROR_NOT_FOUND.0 as i32)
}

/// Stores `secret` under `target`, replacing any earlier one.
pub fn set(target: &str, secret: &str) -> io::Result<()> {
    let mut name = wide(target);
    let mut blob: Vec<u8> = secret.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let blob_len = u32::try_from(blob.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let credential = CREDENTIALW {
        Flags: CRED_FLAGS(0),
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(name.as_mut_ptr()),
        CredentialBlobSize: blob_len,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: `credential` points at `name` and `blob`, which outlive the call.
    let result = unsafe { CredWriteW(&credential, 0) };
    result.map_err(|_| last_error())
}

/// The secret under `target`, `None` if there is none.
pub fn get(target: &str) -> io::Result<Option<String>> {
    let name = wide(target);
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated; on success `found` is set and freed below.
    let read = unsafe { CredReadW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None, &mut found) };
    if read.is_err() {
        let error = last_error();
        return if is_not_found(&error) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    // SAFETY: a successful read gives a valid credential whose blob holds
    // `CredentialBlobSize` bytes; it is copied before `CredFree`.
    let text = unsafe {
        let credential = &*found;
        let bytes = if credential.CredentialBlob.is_null() {
            &[][..]
        } else {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        };
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        let text = String::from_utf16(&units);
        CredFree(found as *const _);
        text
    };
    text.map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "stored secret is not text"))
}

/// Removes the secret under `target`; whether there was one.
pub fn clear(target: &str) -> io::Result<bool> {
    let name = wide(target);
    // SAFETY: `name` is NUL-terminated.
    let result = unsafe { CredDeleteW(PCWSTR(name.as_ptr()), CRED_TYPE_GENERIC, None) };
    if result.is_ok() {
        return Ok(true);
    }
    let error = last_error();
    if is_not_found(&error) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_round_trips_and_clears() {
        let target = format!("Qapsule/test-{}", std::process::id());
        assert_eq!(get(&target).unwrap(), None);
        set(&target, "https://example.com/ünï?x=1").unwrap();
        assert_eq!(
            get(&target).unwrap().as_deref(),
            Some("https://example.com/ünï?x=1")
        );
        set(&target, "second").unwrap();
        assert_eq!(get(&target).unwrap().as_deref(), Some("second"));
        assert!(clear(&target).unwrap());
        assert!(!clear(&target).unwrap());
        assert_eq!(get(&target).unwrap(), None);
    }
}
