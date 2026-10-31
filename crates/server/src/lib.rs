//! Everything in the Qapsule app that runs without a window: the named-pipe
//! server, the engine loop around the core, the action router and the config
//! file. The Tauri shell plugs a window into [`Shell`].

pub mod config;
pub mod config_watch;
pub mod engine;
mod framing;
pub mod pipe;
pub mod security;
pub mod system;

pub use engine::{Engine, Shell, run_effect};
pub use pipe::Listener;

/// The pipe to serve: `QAPSULE_PIPE` if set (for tests and side-by-side dev builds), else the real one.
pub fn pipe_name() -> String {
    std::env::var("QAPSULE_PIPE").unwrap_or_else(|_| qapsule_protocol::PIPE_NAME.to_string())
}
