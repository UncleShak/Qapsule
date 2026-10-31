//! What Windows is playing (decision 0008): follows the current media session
//! and reports it to the core. The state logic is in `tracker` and tested with
//! a fake session; `backend` is the only WinRT code.

mod answer;
mod art;
#[cfg(windows)]
mod backend;
mod name;
mod tracker;

pub use art::{ArtStore, SCHEME, url_for};
#[cfg(windows)]
pub use backend::Media;
