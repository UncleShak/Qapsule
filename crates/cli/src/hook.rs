//! Agent hook events to activities. Hook names change between agent versions;
//! keep this mapping in step with `integrations/` and the agent's docs.

use qapsule_protocol::limits::{MAX_KEY_CHARS, MAX_SUBTITLE_CHARS};
use qapsule_protocol::{Activity, ClientMessage, Progress, State};
use serde_json::Value;

use crate::{dismiss_action, fit, focus_action};

pub const CLAUDE_CODE: &str = "claude-code";

/// A subagent that never reports its stop leaves the pill after this long.
const SUBAGENT_MAX_SECS: u64 = 30 * 60;

/// Maps a Claude Code hook event (its JSON input) to a message, or `None` to ignore it.
/// `focus_pid` is a process inside the terminal running Claude, usually Claude itself.
///
/// | Event | State |
/// | --- | --- |
/// | `UserPromptSubmit`, `PostToolUse` | running |
/// | `Notification` asking for permission or input | needs input |
/// | `Stop` | succeeded |
/// | `StopFailure` | failed |
/// | `SubagentStart` | running, as its own activity `cc-<session>-<agent_id>` |
/// | `SubagentStop` | succeeded, same activity |
/// | `SessionEnd` | removed |
pub fn claude_code(input: &str, focus_pid: Option<u32>) -> Option<ClientMessage> {
    let event: Value = serde_json::from_str(input).ok()?;
    let field = |name: &str| event.get(name).and_then(Value::as_str);
    let session = field("session_id").filter(|s| !s.is_empty())?;
    let sanitise = |raw: String| -> String {
        raw.chars()
            .map(|c| {
                if c.is_whitespace() || c.is_control() {
                    '-'
                } else {
                    c
                }
            })
            .take(MAX_KEY_CHARS)
            .collect()
    };
    let mut id = sanitise(format!("cc-{session}"));
    let mut expires_in = None;
    let folder = field("cwd")
        .and_then(|cwd| cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next())
        .filter(|f| !f.is_empty());

    let (state, title, detail) = match field("hook_event_name")? {
        "UserPromptSubmit" | "PostToolUse" => (State::Running, "Claude is working".into(), None),
        "Notification" => match field("notification_type") {
            Some("permission_prompt" | "elicitation_dialog" | "agent_needs_input") => (
                State::NeedsInput,
                "Claude needs you".into(),
                field("message"),
            ),
            _ => return None,
        },
        event @ ("SubagentStart" | "SubagentStop") => {
            let agent_id = field("agent_id").filter(|s| !s.is_empty())?;
            id = sanitise(format!("cc-{session}-{agent_id}"));
            let name = field("agent_type")
                .filter(|s| !s.is_empty())
                .unwrap_or("Subagent");
            if event == "SubagentStart" {
                // No SessionEnd-time clear can reach these (a clear needs the exact id), so
                // bound a subagent that never reports its stop.
                expires_in = Some(SUBAGENT_MAX_SECS);
                (State::Running, format!("{name} is working"), None)
            } else {
                (State::Succeeded, format!("{name} is done"), None)
            }
        }
        "Stop" => (State::Succeeded, "Claude is done".into(), None),
        "StopFailure" => (State::Failed, "Claude stopped with an error".into(), None),
        "SessionEnd" => {
            return Some(ClientMessage::Clear {
                source: Some(CLAUDE_CODE.into()),
                id: Some(id),
            });
        }
        _ => return None,
    };

    let subtitle = match (detail, folder) {
        (Some(d), Some(f)) => Some(format!("{d} · {f}")),
        (Some(d), None) => Some(d.to_string()),
        (None, f) => f.map(str::to_string),
    }
    .map(|s| fit(&s.replace(['\r', '\n'], " "), MAX_SUBTITLE_CHARS));

    let mut actions = Vec::new();
    if let Some(pid) = focus_pid {
        actions.push(focus_action(pid, "Open terminal"));
    }
    actions.push(dismiss_action());

    Some(ClientMessage::Upsert {
        activity: Activity {
            id,
            source: CLAUDE_CODE.into(),
            title,
            subtitle,
            state,
            // Agents report no percentage, so running shows an indeterminate bar.
            progress: (state == State::Running).then_some(Progress::INDETERMINATE),
            icon: Some("agent".into()),
            expires_in,
            actions,
        },
    })
}
