//! The activity model: what a client pushes and what the pill shows.

use serde::{Deserialize, Serialize};

use crate::limits::*;
use crate::{ErrorCode, ProtocolError};

/// Something happening that a client wants on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    /// Unique within its source; pushing the same id again updates it.
    pub id: String,
    /// Who sent it, e.g. `claude-code`, `wrap`, `my-script`.
    pub source: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<Progress>,
    /// A built-in icon name such as `agent`, `build` or `download`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Seconds until the activity is removed. See [`Activity::lifetime_secs`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Running,
    NeedsInput,
    Succeeded,
    Failed,
    Info,
}

impl State {
    /// Finished activities leave the pill a few seconds after they end.
    pub fn is_finished(self) -> bool {
        matches!(self, State::Succeeded | State::Failed)
    }
}

/// A fraction from 0 to 1, or `"indeterminate"` for work with no known end.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Progress {
    Fraction(f64),
    Indeterminate(Indeterminate),
}

/// The literal string `"indeterminate"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Indeterminate {
    Indeterminate,
}

impl Progress {
    pub const INDETERMINATE: Progress = Progress::Indeterminate(Indeterminate::Indeterminate);
}

/// A button on an activity or pop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Unique within the activity; sent back in callbacks.
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: ActionKind,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub primary: bool,
}

/// What a button does. Pushed buttons never run commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionKind {
    /// Opens an http or https link in the default browser.
    OpenUrl { url: String },
    /// Brings forward the window owned by this process or its nearest ancestor.
    FocusWindow { pid: u32 },
    /// Sends an `action` message back to the client that pushed the activity.
    Callback,
    /// Removes the activity.
    Dismiss,
}

impl Activity {
    /// How long the activity lives, in seconds, when the client gave no `expires_in`.
    pub fn lifetime_secs(&self) -> u64 {
        self.expires_in.unwrap_or(if self.state.is_finished() {
            FINISHED_LIFETIME_SECS
        } else {
            DEFAULT_LIFETIME_SECS
        })
    }

    /// Checks every limit in protocol v1.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        check_key("id", &self.id)?;
        check_key("source", &self.source)?;
        check_text("title", &self.title, MAX_TITLE_CHARS, true)?;
        if let Some(subtitle) = &self.subtitle {
            check_text("subtitle", subtitle, MAX_SUBTITLE_CHARS, false)?;
        }
        if let Some(Progress::Fraction(f)) = self.progress
            && !(0.0..=1.0).contains(&f)
        {
            return Err(invalid(format!("progress must be from 0 to 1, got {f}")));
        }
        if let Some(icon) = &self.icon
            && (icon.is_empty()
                || icon.len() > MAX_ICON_CHARS
                || !icon
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b == b'_' || b == b'-'))
        {
            return Err(invalid("icon must be a short built-in name like `agent`"));
        }
        if let Some(secs) = self.expires_in
            && !(1..=MAX_EXPIRES_IN_SECS).contains(&secs)
        {
            return Err(invalid(format!(
                "expires_in must be from 1 to {MAX_EXPIRES_IN_SECS} seconds"
            )));
        }
        if self.actions.len() > MAX_ACTIONS {
            return Err(invalid(format!(
                "at most {MAX_ACTIONS} actions are allowed"
            )));
        }
        if self.actions.iter().filter(|a| a.primary).count() > 1 {
            return Err(invalid("only one action may be primary"));
        }
        for (i, action) in self.actions.iter().enumerate() {
            action.validate()?;
            if self.actions[..i].iter().any(|a| a.id == action.id) {
                return Err(invalid(format!("duplicate action id `{}`", action.id)));
            }
        }
        Ok(())
    }
}

impl Action {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        check_key("action id", &self.id)?;
        check_text("action label", &self.label, MAX_LABEL_CHARS, true)?;
        match &self.kind {
            ActionKind::OpenUrl { url } => {
                let lower = url.to_ascii_lowercase();
                if url.len() > MAX_URL_CHARS
                    || !(lower.starts_with("https://") || lower.starts_with("http://"))
                    || url.chars().any(char::is_control)
                {
                    return Err(invalid("open_url accepts only http and https links"));
                }
            }
            ActionKind::FocusWindow { pid } if *pid == 0 => {
                return Err(invalid("focus_window needs a process id"));
            }
            _ => {}
        }
        Ok(())
    }
}

fn invalid(message: impl Into<String>) -> ProtocolError {
    ProtocolError::new(ErrorCode::InvalidMessage, message)
}

/// Ids and source names: short, non-empty, no control characters.
pub(crate) fn check_key(field: &str, value: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.chars().count() > MAX_KEY_CHARS {
        return Err(invalid(format!(
            "{field} must be 1 to {MAX_KEY_CHARS} characters"
        )));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(invalid(format!(
            "{field} must not contain spaces or control characters"
        )));
    }
    Ok(())
}

pub(crate) fn check_text(
    field: &str,
    value: &str,
    max: usize,
    required: bool,
) -> Result<(), ProtocolError> {
    if required && value.trim().is_empty() {
        return Err(invalid(format!("{field} must not be empty")));
    }
    if value.chars().count() > max {
        return Err(invalid(format!("{field} must be at most {max} characters")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}
