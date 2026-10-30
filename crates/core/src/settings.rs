//! User settings, read from `config.toml`. A wrong or unknown value is reported
//! and replaced by its default, so a typo can't stop Qapsule from starting.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use toml::{Table, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibilityMode {
    /// The peek is always visible, except behind fullscreen apps.
    #[default]
    Auto,
    /// The peek is always visible, even over fullscreen apps.
    Always,
    /// Tucked into the edge; appears for pops, a top-centre hover or the hotkey.
    OnDemand,
}

impl VisibilityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            VisibilityMode::Auto => "auto",
            VisibilityMode::Always => "always",
            VisibilityMode::OnDemand => "on_demand",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(VisibilityMode::Auto),
            "always" => Some(VisibilityMode::Always),
            "on_demand" => Some(VisibilityMode::OnDemand),
            _ => None,
        }
    }
}

/// What opens the expanded view from the peek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpandOn {
    /// Hovering the peek (dwell or slowing down).
    #[default]
    Hover,
    /// Only a click on the peek.
    Click,
}

impl ExpandOn {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "hover" => Some(ExpandOn::Hover),
            "click" => Some(ExpandOn::Click),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetKind {
    Agents,
    Timer,
    Weather,
    Time,
    Events,
    Audio,
    Shortcuts,
}

impl WidgetKind {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "agents" => Some(WidgetKind::Agents),
            "timer" => Some(WidgetKind::Timer),
            "weather" => Some(WidgetKind::Weather),
            "time" => Some(WidgetKind::Time),
            "events" => Some(WidgetKind::Events),
            "audio" => Some(WidgetKind::Audio),
            "shortcuts" => Some(WidgetKind::Shortcuts),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetSize {
    Small,
    Wide,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WidgetConfig {
    pub kind: WidgetKind,
    pub size: WidgetSize,
    /// Weather only.
    pub city: Option<String>,
}

/// One of the three screens of the expanded view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    pub widgets: Vec<WidgetConfig>,
}

fn widget(kind: WidgetKind, size: WidgetSize) -> WidgetConfig {
    WidgetConfig {
        kind,
        size,
        city: None,
    }
}

/// Left: your widgets. Centre: main. Right: system.
pub fn default_screens() -> Vec<Screen> {
    use WidgetKind::*;
    use WidgetSize::*;
    vec![
        Screen {
            widgets: vec![widget(Agents, Wide), widget(Timer, Small)],
        },
        Screen {
            widgets: vec![
                widget(Weather, Small),
                widget(Time, Small),
                widget(Events, Small),
            ],
        },
        Screen {
            widgets: vec![widget(Audio, Wide), widget(Shortcuts, Small)],
        },
    ]
}

/// Whether a source may pop, and whether its pops show details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopSetting {
    pub pop: bool,
    /// When off, pops show only the source name, not the title or subtitle.
    pub details: bool,
}

impl Default for PopSetting {
    fn default() -> Self {
        Self {
            pop: true,
            details: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub visibility: VisibilityMode,
    pub dual_status: bool,
    pub do_not_disturb: bool,
    /// Keyed by source name (`claude-code`) or category (`agents`, `commands`).
    pub notifications: HashMap<String, PopSetting>,
    pub expand_on: ExpandOn,
    /// Left, centre and right screens.
    pub screens: Vec<Screen>,
    /// How long before an event its pop shows.
    pub calendar_minutes_before: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            visibility: VisibilityMode::Auto,
            dual_status: true,
            do_not_disturb: false,
            notifications: HashMap::new(),
            expand_on: ExpandOn::Hover,
            screens: default_screens(),
            calendar_minutes_before: 10,
        }
    }
}

/// The category a source belongs to in `[notifications]`.
pub fn category(source: &str) -> &'static str {
    if qapsule_protocol::is_reserved_source(source) {
        return "windows_apps";
    }
    match source {
        "claude-code" | "codex" => "agents",
        "wrap" => "commands",
        _ => "apps",
    }
}

impl Settings {
    /// The pop setting for a source: its own entry, then its category's, then the default.
    pub fn pop_setting(&self, source: &str) -> PopSetting {
        self.notifications
            .get(source)
            .or_else(|| self.notifications.get(category(source)))
            .copied()
            .unwrap_or_default()
    }

    /// The city of the first weather widget on the centre screen, if one is set.
    pub fn weather_city(&self) -> Option<&str> {
        self.screens
            .get(1)?
            .widgets
            .iter()
            .find(|w| w.kind == WidgetKind::Weather)?
            .city
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
    }

    /// Reads settings from TOML. Returns the settings and a warning for each value it ignored.
    pub fn from_toml(text: &str) -> (Settings, Vec<String>) {
        let mut settings = Settings::default();
        let mut warnings = Vec::new();
        let table: Table = match text.parse() {
            Ok(table) => table,
            Err(e) => {
                warnings.push(format!("config.toml isn't valid TOML, using defaults: {e}"));
                return (settings, warnings);
            }
        };

        if let Some(general) = section(&table, "general", &mut warnings) {
            if let Some(value) = general.get("visibility") {
                match value.as_str().and_then(VisibilityMode::parse) {
                    Some(mode) => settings.visibility = mode,
                    None => warnings.push(format!(
                        "general.visibility must be \"auto\", \"always\" or \"on_demand\", not {value}"
                    )),
                }
            }
            if let Some(value) = general.get("expand_on") {
                match value.as_str().and_then(ExpandOn::parse) {
                    Some(mode) => settings.expand_on = mode,
                    None => warnings.push(format!(
                        "general.expand_on must be \"hover\" or \"click\", not {value}"
                    )),
                }
            }
        }
        if let Some(peek) = section(&table, "peek", &mut warnings) {
            read_bool(
                peek,
                "peek.dual_status",
                "dual_status",
                &mut settings.dual_status,
                &mut warnings,
            );
        }
        if let Some(notifications) = section(&table, "notifications", &mut warnings) {
            for (name, value) in notifications {
                if name == "do_not_disturb" {
                    read_bool(
                        notifications,
                        "notifications.do_not_disturb",
                        name,
                        &mut settings.do_not_disturb,
                        &mut warnings,
                    );
                    continue;
                }
                let Some(entry) = value.as_table() else {
                    warnings.push(format!(
                        "notifications.{name} must be a table like {{ pop = true, details = true }}"
                    ));
                    continue;
                };
                let mut setting = PopSetting::default();
                let path = format!("notifications.{name}");
                if name == "calendar" {
                    read_minutes_before(entry, &path, &mut settings, &mut warnings);
                }
                read_bool(
                    entry,
                    &format!("{path}.pop"),
                    "pop",
                    &mut setting.pop,
                    &mut warnings,
                );
                read_bool(
                    entry,
                    &format!("{path}.details"),
                    "details",
                    &mut setting.details,
                    &mut warnings,
                );
                settings.notifications.insert(name.clone(), setting);
            }
        }
        if let Some(value) = table.get("screens") {
            match read_screens(value, &mut warnings) {
                Some(screens) => settings.screens = screens,
                None => warnings.push("using the default screens".into()),
            }
        }
        (settings, warnings)
    }
}

fn read_minutes_before(
    entry: &Table,
    path: &str,
    settings: &mut Settings,
    warnings: &mut Vec<String>,
) {
    match entry.get("minutes_before") {
        None => {}
        Some(Value::Integer(n)) if (1..=120).contains(n) => {
            settings.calendar_minutes_before = *n as u32;
        }
        Some(other) => warnings.push(format!(
            "{path}.minutes_before must be a whole number from 1 to 120, not {other}"
        )),
    }
}

/// The three screens, or `None` (after warning why) when the list can't be used.
fn read_screens(value: &Value, warnings: &mut Vec<String>) -> Option<Vec<Screen>> {
    let Some(list) = value.as_array() else {
        warnings.push("[[screens]] must be an array of tables".into());
        return None;
    };
    if list.len() != 3 {
        warnings.push(format!(
            "there must be exactly 3 [[screens]], not {}",
            list.len()
        ));
        return None;
    }
    let mut screens = Vec::new();
    for (index, screen) in list.iter().enumerate() {
        let path = format!("screens[{index}]");
        let Some(widgets) = screen.get("widgets").and_then(Value::as_array) else {
            warnings.push(format!("{path}.widgets must be a list of widgets"));
            return None;
        };
        let widgets: Vec<WidgetConfig> = widgets
            .iter()
            .filter_map(|w| read_widget(w, &path, warnings))
            .collect();
        if widgets.is_empty() || widgets.len() > 3 {
            warnings.push(format!(
                "{path} must have 1 to 3 widgets, not {}",
                widgets.len()
            ));
            return None;
        }
        screens.push(Screen { widgets });
    }
    Some(screens)
}

fn read_widget(value: &Value, path: &str, warnings: &mut Vec<String>) -> Option<WidgetConfig> {
    let Some(table) = value.as_table() else {
        warnings.push(format!(
            "{path}: a widget must be a table, so it was dropped"
        ));
        return None;
    };
    let Some(kind) = table
        .get("type")
        .and_then(Value::as_str)
        .and_then(WidgetKind::parse)
    else {
        warnings.push(format!("{path}: unknown widget type, so it was dropped"));
        return None;
    };
    let size = match table.get("size") {
        None => WidgetSize::Small,
        Some(Value::String(s)) if s == "small" => WidgetSize::Small,
        Some(Value::String(s)) if s == "wide" => WidgetSize::Wide,
        Some(other) => {
            warnings.push(format!(
                "{path}: size must be \"small\" or \"wide\", not {other}"
            ));
            WidgetSize::Small
        }
    };
    let city = match table.get("city") {
        None => None,
        Some(Value::String(s)) if s.chars().count() <= 100 => Some(s.clone()),
        Some(Value::String(_)) => {
            warnings.push(format!("{path}: city must be at most 100 characters"));
            None
        }
        Some(other) => {
            warnings.push(format!("{path}: city must be text, not {other}"));
            None
        }
    };
    Some(WidgetConfig { kind, size, city })
}

fn section<'a>(table: &'a Table, name: &str, warnings: &mut Vec<String>) -> Option<&'a Table> {
    match table.get(name) {
        None => None,
        Some(Value::Table(t)) => Some(t),
        Some(_) => {
            warnings.push(format!("[{name}] must be a table"));
            None
        }
    }
}

fn read_bool(table: &Table, path: &str, name: &str, out: &mut bool, warnings: &mut Vec<String>) {
    match table.get(name) {
        None => {}
        Some(Value::Boolean(b)) => *out = *b,
        Some(other) => warnings.push(format!("{path} must be true or false, not {other}")),
    }
}
