//! Secrets in Windows Credential Manager. A secret never appears in an error,
//! a log or a config file.

/// Where the calendar's secret iCal link is stored.
pub const CALENDAR_LINK: &str = "Qapsule/calendar-ical";

#[cfg(windows)]
mod imp;
#[cfg(windows)]
pub use imp::{clear, get, set};

#[cfg(not(windows))]
pub fn set(_target: &str, _secret: &str) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(not(windows))]
pub fn get(_target: &str) -> std::io::Result<Option<String>> {
    Err(std::io::ErrorKind::Unsupported.into())
}

#[cfg(not(windows))]
pub fn clear(_target: &str) -> std::io::Result<bool> {
    Err(std::io::ErrorKind::Unsupported.into())
}
