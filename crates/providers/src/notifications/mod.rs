//! Other apps' Windows toasts, shown as activities.

#[cfg(windows)]
mod listener;
mod toast;
mod tracked;

#[cfg(windows)]
pub use listener::Notifications;
pub use toast::*;
