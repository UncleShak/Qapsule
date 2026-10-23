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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            visibility: VisibilityMode::Auto,
            dual_status: true,
            do_not_disturb: false,
            notifications: HashMap::new(),
        }
    }
}

/// The category a source belongs to in `[notifications]`.
pub fn category(source: &str) -> &'static str {
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

        if let Some(general) = section(&table, "general", &mut warnings)
            && let Some(value) = general.get("visibility")
        {
            match value.as_str().and_then(VisibilityMode::parse) {
                Some(mode) => settings.visibility = mode,
                None => warnings.push(format!(
                    "general.visibility must be \"auto\", \"always\" or \"on_demand\", not {value}"
                )),
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
        (settings, warnings)
    }
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
