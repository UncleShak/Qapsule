//! What the UI is told to draw. The UI holds a copy of this and nothing else.

use qapsule_protocol::{Action, ActionKind, Activity, Progress, State};
use serde::Serialize;

use crate::settings::VisibilityMode;

/// A full description of the screen. Sent whole on every update.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiState {
    /// Local time as `HH:MM`, updated once a minute.
    pub clock: String,
    pub mode: VisibilityMode,
    /// False when the capsule is tucked into the top edge.
    pub shown: bool,
    pub peek: Peek,
    pub pop: Option<PopView>,
    /// "Notification dismissed · Undo" is showing.
    pub undo: bool,
    /// The expanded view, only while it is open.
    pub expanded: Option<ExpandedView>,
    /// The window should hold keyboard focus (the hotkey opened the view).
    pub focus: bool,
}

/// The main screen of the expanded view.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExpandedView {
    /// Local date like `Sat 31 Oct`.
    pub date: String,
    pub weather: WeatherView,
    pub events: EventsView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeatherView {
    /// No city in the config.
    NoCity,
    Loading,
    Ready(WeatherReady),
    /// Never loaded and the last try failed.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WeatherReady {
    pub city: String,
    pub temp_c: i32,
    pub condition: String,
    pub glyph: String,
    pub high_c: i32,
    pub low_c: i32,
    pub hourly: Vec<HourView>,
    /// The last refresh failed; this is older data.
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HourView {
    pub label: String,
    pub temp_c: i32,
    pub glyph: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventsView {
    /// No calendar link is set.
    NoLink,
    Loading,
    Ready(EventsReady),
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventsReady {
    pub now: Option<EventNow>,
    pub next: Vec<EventNext>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventNow {
    pub title: String,
    /// End time as `HH:MM`.
    pub until: String,
    pub location: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EventNext {
    pub title: String,
    /// `09:00`, `Tomorrow 09:00` or `Thu 07:30`.
    pub when: String,
}

// Built a few times a second at most and serialised at once, so boxing would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Peek {
    /// Nothing happening: the time.
    Idle,
    /// One thing happening: it takes the whole pill.
    One { main: ItemView },
    /// Two or more: the top item takes ¾, the next ¼.
    Dual { main: ItemView, side: ItemView },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemView {
    pub source: String,
    pub id: String,
    pub source_label: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub state: State,
    /// 0 to 1 when the client reports a fraction.
    pub progress: Option<f64>,
    pub indeterminate: bool,
    pub icon: String,
    /// Whole minutes since a running activity started.
    pub elapsed_min: Option<u64>,
    /// The action clicking the item runs, if any.
    pub primary_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PopView {
    pub source: String,
    pub id: String,
    pub source_label: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub state: State,
    pub icon: String,
    pub actions: Vec<ActionView>,
    /// Stays until handled, instead of shrinking back after a few seconds.
    pub sticky: bool,
    /// Changes whenever a new pop replaces the current one.
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActionView {
    pub id: String,
    pub label: String,
    pub kind: &'static str,
    pub primary: bool,
}

/// The friendly name for a source.
pub fn source_label(source: &str) -> String {
    match source {
        "claude-code" => "Claude Code".into(),
        "codex" => "Codex".into(),
        "wrap" => "Command".into(),
        other => match other.split_once(':') {
            Some((_, name)) if qapsule_protocol::is_reserved_source(other) => {
                name.replace('-', " ")
            }
            _ => other.into(),
        },
    }
}

/// The built-in icon to draw: the activity's own, or one picked by its source.
pub fn icon_for(activity: &Activity) -> String {
    if let Some(icon) = &activity.icon {
        return icon.clone();
    }
    match activity.source.as_str() {
        "claude-code" | "codex" => "agent",
        "wrap" => "build",
        s if qapsule_protocol::is_reserved_source(s) => "app",
        _ => "info",
    }
    .into()
}

/// The line a pop shows when details are off for its source.
pub fn generic_title(state: State) -> &'static str {
    match state {
        State::Running => "Working",
        State::NeedsInput => "Needs your input",
        State::Succeeded => "Done",
        State::Failed => "Stopped with an error",
        State::Info => "New notification",
    }
}

/// The action that runs when the whole item or card is clicked.
pub fn primary_action(activity: &Activity) -> Option<&Action> {
    activity
        .actions
        .iter()
        .find(|a| a.primary)
        .or_else(|| match activity.actions.as_slice() {
            [only] if only.kind != ActionKind::Dismiss => Some(only),
            _ => None,
        })
}

pub fn action_view(action: &Action, primary: bool) -> ActionView {
    ActionView {
        id: action.id.clone(),
        label: action.label.clone(),
        kind: match action.kind {
            ActionKind::OpenUrl { .. } => "open_url",
            ActionKind::FocusWindow { .. } => "focus_window",
            ActionKind::Callback => "callback",
            ActionKind::Dismiss => "dismiss",
        },
        primary,
    }
}

pub fn item_view(activity: &Activity, elapsed_min: Option<u64>) -> ItemView {
    ItemView {
        source: activity.source.clone(),
        id: activity.id.clone(),
        source_label: source_label(&activity.source),
        title: activity.title.clone(),
        subtitle: activity.subtitle.clone(),
        state: activity.state,
        progress: match activity.progress {
            Some(Progress::Fraction(f)) => Some(f),
            _ => None,
        },
        indeterminate: matches!(activity.progress, Some(Progress::Indeterminate(_))),
        icon: icon_for(activity),
        elapsed_min,
        primary_action: primary_action(activity).map(|a| a.id.clone()),
    }
}
