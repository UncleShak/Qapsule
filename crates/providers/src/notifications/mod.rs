//! Other apps' Windows toasts, shown as activities.

#[cfg(windows)]
mod listener;
mod status;
mod toast;
mod tracked;

#[cfg(windows)]
pub use listener::Notifications;
pub use status::Status;
pub use toast::*;
