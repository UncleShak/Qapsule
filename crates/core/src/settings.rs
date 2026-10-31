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
    Hover,
    /// Only a click on the peek.
    #[default]
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

/// The key of a [`Hotkey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKey {
    /// `A`-`Z` (upper case), `0`-`9` or one of the punctuation keys.
    Char(char),
    /// `F1`-`F24`.
    F(u8),
    Space,
    Tab,
    Enter,
    Esc,
}

impl HotkeyKey {
    /// Backtick, minus, equals, brackets, semicolon, quote, comma, period, slash, backslash.
    const PUNCTUATION: &'static str = "`-=[];',./\\";

    fn parse(text: &str) -> Option<Self> {
        let lower = text.to_ascii_lowercase();
        match lower.as_str() {
            "space" => return Some(Self::Space),
            "tab" => return Some(Self::Tab),
            "enter" => return Some(Self::Enter),
            "esc" => return Some(Self::Esc),
            _ => {}
        }
        if let Some(n) = lower.strip_prefix('f')
            && !n.is_empty()
            && n.bytes().all(|b| b.is_ascii_digit())
        {
            return n
                .parse::<u8>()
                .ok()
                .filter(|n| (1..=24).contains(n))
                .map(Self::F);
        }
        let mut chars = text.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return None;
        };
        let c = c.to_ascii_uppercase();
        (c.is_ascii_alphanumeric() || Self::PUNCTUATION.contains(c)).then_some(Self::Char(c))
    }
}

impl std::fmt::Display for HotkeyKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Char(c) => write!(f, "{c}"),
            Self::F(n) => write!(f, "F{n}"),
            Self::Space => f.write_str("Space"),
            Self::Tab => f.write_str("Tab"),
            Self::Enter => f.write_str("Enter"),
            Self::Esc => f.write_str("Esc"),
        }
    }
}

/// A global shortcut: modifiers and one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub key: HotkeyKey,
}

impl Hotkey {
    /// Ctrl+Alt+Q.
    pub const DEFAULT: Hotkey = Hotkey {
        ctrl: true,
        alt: true,
        shift: false,
        win: false,
        key: HotkeyKey::Char('Q'),
    };

    /// Reads `Ctrl+Alt+Q`-style text (case-insensitive). `Ok(None)` means off
    /// (`"off"` or an empty string). At least one of Ctrl, Alt or Win is required,
    /// so a shortcut cannot take over ordinary typing.
    pub fn parse(text: &str) -> Result<Option<Hotkey>, String> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("off") {
            return Ok(None);
        }
        // The key is whatever follows the last `+`; a `+` key itself is not supported.
        let mut parts: Vec<&str> = text.split('+').map(str::trim).collect();
        let key = parts.pop().unwrap_or_default();
        let key = HotkeyKey::parse(key).ok_or_else(|| {
            String::from(
                "the key must be A-Z, 0-9, F1-F24, Space, Tab, Enter, Esc or one of ` - = [ ] ; ' , . / \\",
            )
        })?;
        let mut hotkey = Hotkey {
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            key,
        };
        for part in parts {
            let flag = match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => &mut hotkey.ctrl,
                "alt" => &mut hotkey.alt,
                "shift" => &mut hotkey.shift,
                "win" => &mut hotkey.win,
                _ => return Err("modifiers are Ctrl, Alt, Shift and Win".into()),
            };
            if std::mem::replace(flag, true) {
                return Err("a modifier is repeated".into());
            }
        }
        if !(hotkey.ctrl || hotkey.alt || hotkey.win) {
            return Err("it needs Ctrl, Alt or Win".into());
        }
        Ok(Some(hotkey))
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
            (self.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
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
    /// The shortcut that opens the main screen; `None` when turned off.
    pub hotkey: Option<Hotkey>,
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
            expand_on: ExpandOn::Click,
            hotkey: Some(Hotkey::DEFAULT),
            screens: default_screens(),
            calendar_minutes_before: 10,
        }
    }
}

/// The category a source belongs to in `[notifications]`.
pub fn category(source: &str) -> &'static str {
    if source
        .get(..9)
        .is_some_and(|head| head.eq_ignore_ascii_case("calendar:"))
    {
        return "calendar";
    }
    if source == crate::media::SOURCE {
        return "music";
    }
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
            if let Some(value) = general.get("hotkey") {
                match value.as_str().map(Hotkey::parse) {
                    Some(Ok(hotkey)) => settings.hotkey = hotkey,
                    Some(Err(why)) => {
                        warnings.push(format!("general.hotkey: {why}; using {}", Hotkey::DEFAULT))
                    }
                    None => warnings.push(format!(
                        "general.hotkey: it must be text like \"Ctrl+Alt+Q\" or \"off\"; using {}",
                        Hotkey::DEFAULT
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
