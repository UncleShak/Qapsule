//! Messages sent over the pipe, one JSON object per line.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::{Activity, State, check_key, check_text};
use crate::limits::*;
use crate::{ErrorCode, PROTOCOL_VERSION, ProtocolError};

/// Client to Qapsule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Optional greeting; answered with `ok`.
    Hello {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client: Option<String>,
    },
    /// Adds an activity, or replaces the one with the same source and id.
    Upsert { activity: Activity },
    /// Marks an activity finished. It stays briefly, then leaves.
    End {
        source: String,
        id: String,
        /// `succeeded` (the default) or `failed`.
        #[serde(default = "succeeded")]
        state: State,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subtitle: Option<String>,
    },
    /// Removes activities at once: one (source and id), a whole source, or everything.
    Clear {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// Asks for the live activities, optionally from one source.
    List {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
}

fn succeeded() -> State {
    State::Succeeded
}

/// Qapsule to client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Ok,
    Error {
        code: ErrorCode,
        message: String,
    },
    ListResult {
        activities: Vec<Activity>,
    },
    /// A `callback` button was pressed on one of this client's activities.
    Action {
        source: String,
        id: String,
        action: String,
    },
}

impl From<ProtocolError> for ServerMessage {
    fn from(e: ProtocolError) -> Self {
        ServerMessage::Error {
            code: e.code,
            message: e.message,
        }
    }
}

fn check_not_reserved(source: &str) -> Result<(), ProtocolError> {
    match crate::reserved_prefix(source) {
        Some(prefix) => Err(ProtocolError::new(
            ErrorCode::InvalidMessage,
            format!("the {prefix} source prefix is reserved for Qapsule"),
        )),
        None => Ok(()),
    }
}

impl ClientMessage {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            ClientMessage::Hello { .. } | ClientMessage::List { .. } => Ok(()),
            ClientMessage::Upsert { activity } => {
                check_not_reserved(&activity.source)?;
                if let Some(icon) = activity.icon.as_deref()
                    && crate::RESERVED_ICONS.contains(&icon)
                {
                    return Err(ProtocolError::new(
                        ErrorCode::InvalidMessage,
                        format!("the {icon} icon is reserved for Qapsule"),
                    ));
                }
                activity.validate()
            }
            ClientMessage::End {
                source,
                id,
                state,
                title,
                subtitle,
            } => {
                check_not_reserved(source)?;
                check_key("source", source)?;
                check_key("id", id)?;
                if !state.is_finished() {
                    return Err(ProtocolError::new(
                        ErrorCode::InvalidMessage,
                        "end takes the state `succeeded` or `failed`",
                    ));
                }
                if let Some(title) = title {
                    check_text("title", title, MAX_TITLE_CHARS, true)?;
                }
                if let Some(subtitle) = subtitle {
                    check_text("subtitle", subtitle, MAX_SUBTITLE_CHARS, false)?;
                }
                Ok(())
            }
            ClientMessage::Clear { source, id } => {
                if id.is_some() && source.is_none() {
                    return Err(ProtocolError::new(
                        ErrorCode::InvalidMessage,
                        "clear with an id also needs its source",
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Serialize)]
struct Envelope<'a, T> {
    v: u32,
    #[serde(flatten)]
    body: &'a T,
}

/// Encodes a message as one line of JSON, without the trailing newline.
pub fn encode<T: Serialize>(message: &T) -> String {
    serde_json::to_string(&Envelope {
        v: PROTOCOL_VERSION,
        body: message,
    })
    .expect("protocol messages always serialise")
}

/// Decodes and validates one line from a client.
pub fn decode_client(line: &str) -> Result<ClientMessage, ProtocolError> {
    let message: ClientMessage = decode(line)?;
    message.validate()?;
    Ok(message)
}

/// Decodes one line from Qapsule.
pub fn decode_server(line: &str) -> Result<ServerMessage, ProtocolError> {
    decode(line)
}

fn decode<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, ProtocolError> {
    if line.len() > MAX_MESSAGE_BYTES {
        return Err(ProtocolError::new(
            ErrorCode::TooLarge,
            format!("messages are limited to {MAX_MESSAGE_BYTES} bytes"),
        ));
    }
    let value: Value = serde_json::from_str(line)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidJson, e.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(ProtocolError::new(
            ErrorCode::InvalidJson,
            "a message must be a JSON object",
        ));
    };
    match object.get("v").and_then(Value::as_u64) {
        Some(v) if v == u64::from(PROTOCOL_VERSION) => {}
        Some(v) => {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedVersion,
                format!("this pipe speaks protocol v{PROTOCOL_VERSION}, not v{v}"),
            ));
        }
        None => {
            return Err(ProtocolError::new(
                ErrorCode::UnsupportedVersion,
                format!("every message needs \"v\":{PROTOCOL_VERSION}"),
            ));
        }
    }
    serde_json::from_value(value)
        .map_err(|e| ProtocolError::new(ErrorCode::InvalidMessage, e.to_string()))
}
