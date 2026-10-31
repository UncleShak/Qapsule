//! The public contract between Qapsule and its clients.
//!
//! Clients send newline-delimited JSON over the named pipe [`PIPE_NAME`]. Every
//! message carries `"v": 1`. Unknown fields are ignored, so v1 clients keep
//! working as v1 grows; a breaking change means protocol v2 on a new pipe name.

mod activity;
mod message;

use std::fmt;

use serde::{Deserialize, Serialize};

pub use activity::{Action, ActionKind, Activity, Indeterminate, Progress, State};
pub use message::{ClientMessage, ServerMessage, decode_client, decode_server, encode};

/// The protocol version spoken over the pipe.
pub const PROTOCOL_VERSION: u32 = 1;

/// The named pipe Qapsule listens on.
pub const PIPE_NAME: &str = r"\\.\pipe\qapsule";

/// Limits every client must respect. Breaking one gets an `error` reply.
pub mod limits {
    /// Bytes per message, including the JSON but not the newline.
    pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
    pub const MAX_ACTIVITIES_PER_SOURCE: usize = 50;
    pub const MAX_MESSAGES_PER_SECOND: u32 = 20;
    pub const MAX_TITLE_CHARS: usize = 80;
    pub const MAX_SUBTITLE_CHARS: usize = 120;
    /// Ids and source names.
    pub const MAX_KEY_CHARS: usize = 64;
    pub const MAX_LABEL_CHARS: usize = 24;
    pub const MAX_ICON_CHARS: usize = 32;
    pub const MAX_URL_CHARS: usize = 2048;
    pub const MAX_ACTIONS: usize = 2;
    /// One day.
    pub const MAX_EXPIRES_IN_SECS: u64 = 24 * 60 * 60;
    /// Lifetime of a running, waiting or info activity with no `expires_in`.
    pub const DEFAULT_LIFETIME_SECS: u64 = 60 * 60;
    /// Lifetime of a succeeded or failed activity with no `expires_in`.
    pub const FINISHED_LIFETIME_SECS: u64 = 4;
}

/// Source prefixes that belong to Qapsule's own providers. Pipe clients may not
/// `upsert` or `end` activities in them.
pub const RESERVED_SOURCE_PREFIXES: &[&str] = &["windows:", "calendar:", "media:"];

/// Whether `source` starts with a reserved prefix.
pub fn is_reserved_source(source: &str) -> bool {
    reserved_prefix(source).is_some()
}

/// Icons only Qapsule's own providers may use.
pub(crate) const RESERVED_ICONS: &[&str] = &["app"];

/// The reserved prefix `source` starts with, ignoring ASCII case, if any.
pub(crate) fn reserved_prefix(source: &str) -> Option<&'static str> {
    RESERVED_SOURCE_PREFIXES.iter().copied().find(|p| {
        source
            .get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    })
}

/// Why a message was refused. Sent to the client as an `error` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// Not valid JSON, or not a JSON object.
    InvalidJson,
    /// Valid JSON that breaks the protocol: an unknown type, a missing field, a broken limit.
    InvalidMessage,
    /// Missing `v`, or a version this pipe doesn't speak.
    UnsupportedVersion,
    /// Over [`limits::MAX_MESSAGE_BYTES`].
    TooLarge,
    /// Over [`limits::MAX_MESSAGES_PER_SECOND`] on one connection.
    RateLimited,
    /// Over [`limits::MAX_ACTIVITIES_PER_SOURCE`].
    TooManyActivities,
    /// `end` named an activity that doesn't exist.
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: ErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests;
