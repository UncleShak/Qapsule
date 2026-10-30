//! A security descriptor that lets only the current Windows user open the pipe.

use std::ffi::c_void;
use std::io;

use windows::Win32::Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    TokenUser,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{HSTRING, PWSTR};

pub struct PipeSecurity {
    descriptor: PSECURITY_DESCRIPTOR,
}

// The descriptor is read-only after creation.
unsafe impl Send for PipeSecurity {}
unsafe impl Sync for PipeSecurity {}

impl PipeSecurity {
    /// Full access for the current user; nobody else, not even administrators.
    pub fn current_user_only() -> io::Result<Self> {
        let sddl = current_user_sddl()?;
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )?;
        }
        Ok(Self { descriptor })
    }

    /// Attributes to pass to `CreateNamedPipeW`. Valid while `self` lives.
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.descriptor.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for PipeSecurity {
    fn drop(&mut self) {
        unsafe {
            LocalFree(Some(HLOCAL(self.descriptor.0)));
        }
    }
}

/// `D:P(A;;GA;;;<sid>)`: a protected list with one entry granting the user everything.
pub fn current_user_sddl() -> io::Result<String> {
    Ok(format!("D:P(A;;GA;;;{})", current_user_sid()?))
}

/// The current user's SID as a string, like `S-1-5-21-…`.
pub fn current_user_sid() -> io::Result<String> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)?;
        let mut len = 0u32;
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut len);
        // u64s keep the buffer aligned for TOKEN_USER.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        let result = GetTokenInformation(
            token,
            TokenUser,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            len,
            &mut len,
        );
        let _ = CloseHandle(token);
        result?;
        let user = &*buf.as_ptr().cast::<TOKEN_USER>();
        let mut sid = PWSTR::null();
        ConvertSidToStringSidW(user.User.Sid, &mut sid)?;
        let text = sid.to_string();
        LocalFree(Some(HLOCAL(sid.0.cast())));
        text.map_err(io::Error::other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_only_the_current_user() {
        let sddl = current_user_sddl().unwrap();
        assert!(sddl.starts_with("D:P(A;;GA;;;S-1-"), "{sddl}");
        assert_eq!(sddl.matches("(A;").count(), 1, "exactly one allow entry");
        PipeSecurity::current_user_only().unwrap();
    }
}
