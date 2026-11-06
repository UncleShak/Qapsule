//! Focus: a study timer with a module chip and a per-module study log (decision 0012).
//! Rust owns all of it: the UI only shows the [`FocusView`] it is given and sends presses
//! back.
//!
//! The timer counts down from 15, 25 or 50 minutes, or counts up. Every focused second
//! is credited to the day and the module, and the log is saved as `study.toml` beside
//! the config. While a session runs, pops other than "needs input" are held in the
//! history instead of popping.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use qapsule_protocol::{Action, ActionKind, Activity, State};
use serde::{Deserialize, Serialize};

use crate::view::{EventsView, ItemView};

/// The reserved source the focus timer's peek item and pops use.
pub const SOURCE: &str = "focus:";
/// The id of the focus timer's peek item and pops.
pub const ID: &str = "session";
/// The "Start another" button on the pop a finished session shows.
pub const AGAIN: &str = "again";
/// The countdown lengths offered, in minutes. 0 means counting up.
pub const LENGTHS: [u32; 3] = [15, 25, 50];
const DEFAULT_LENGTH: u32 = 25;
/// A module picked by hand wins over the calendar for this long.
const MANUAL_HOLD: Duration = Duration::from_secs(10 * 60);
/// While a session runs, the log is credited and saved this often.
const SAVE_EVERY: Duration = Duration::from_secs(60);
/// The log keeps this many days.
const MAX_DAYS: usize = 180;
/// A day counts towards the streak with at least this much focus.
const STREAK_SECS: u64 = 5 * 60;
/// A count-up session counts as a session once it reaches this.
const COUNT_UP_SESSION_SECS: u64 = 5 * 60;
/// Focus time with no module is filed under this name.
pub const NO_MODULE: &str = "Other";
/// Module colours, in first-seen order. Light enough to read on the dark capsule.
const COLOURS: [&str; 8] = [
    "#60cdff", "#c084fc", "#7ad99a", "#ffb27a", "#f48fb1", "#ffd966", "#7aa2f7", "#5eead4",
];

/// One day in the study log.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyDay {
    /// Sessions finished that day.
    #[serde(default)]
    pub sessions: u32,
    /// Focused seconds by module.
    #[serde(default)]
    pub modules: BTreeMap<String, u64>,
}

impl StudyDay {
    fn total(&self) -> u64 {
        self.modules.values().sum()
    }
}

/// The study log, as saved in `study.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudyLog {
    /// Modules in the order they were first seen; it fixes their colours.
    #[serde(default)]
    pub order: Vec<String>,
    /// Keyed by local date, `YYYY-MM-DD`.
    #[serde(default)]
    pub days: BTreeMap<String, StudyDay>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusPhase {
    Idle,
    Running,
    Paused,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Running { since: Instant, banked: Duration },
    Paused { banked: Duration },
    Done { banked: Duration },
}

/// A module as the UI shows it: its code and its colour.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModuleChip {
    pub code: String,
    pub color: String,
}

/// One bar of the week chart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DayBar {
    /// `M`, `T`, `W`...
    pub label: String,
    /// 0 to 1 of the busiest day this week.
    pub height: f64,
    pub today: bool,
    /// Like `1 h 20 min`, for the bar's tooltip and screen readers.
    pub time: String,
}

/// One module's share of this week.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModuleRow {
    pub code: String,
    pub color: String,
    pub time: String,
    /// 0 to 1 of the busiest module this week.
    pub share: f64,
}

/// The numbers under the timer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StudyStats {
    pub today: String,
    pub week: String,
    pub streak: u32,
    pub sessions_today: u32,
    pub bars: Vec<DayBar>,
    /// This week's modules, busiest first, at most four.
    pub modules: Vec<ModuleRow>,
}

/// Everything the Focus screen draws.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FocusView {
    pub phase: FocusPhase,
    pub count_up: bool,
    /// 0 when counting up.
    pub length_min: u32,
    pub lengths: Vec<u32>,
    /// `18:42`, or `1:02:05` past an hour.
    pub display: String,
    /// `of 25:00`, `counting up` or `done`.
    pub caption: String,
    /// The ring: 0 to 1 of the countdown, or of the current hour when counting up.
    pub progress: f64,
    pub module: Option<ModuleChip>,
    pub stats: StudyStats,
}

#[derive(Debug)]
pub(crate) struct Focus {
    length_min: u32,
    phase: Phase,
    module: Option<String>,
    picked_by_hand: Option<Instant>,
    /// Seconds up to here are in the log.
    credited_to: Option<Instant>,
    log: StudyLog,
    /// Today's local date, `YYYY-MM-DD`; empty until the engine says.
    day: String,
    dirty: bool,
    save_at: Option<Instant>,
    /// When the shown capsule next needs a new second drawn.
    next_second: Option<Instant>,
}

impl Default for Focus {
    fn default() -> Self {
        Self {
            length_min: DEFAULT_LENGTH,
            phase: Phase::Idle,
            module: None,
            picked_by_hand: None,
            credited_to: None,
            log: StudyLog::default(),
            day: String::new(),
            dirty: false,
            save_at: None,
            next_second: None,
        }
    }
}

/// What [`Focus::tick`] found.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct TickResult {
    /// A countdown reached zero.
    pub finished: bool,
    /// The UI should be told about a new second.
    pub redraw: bool,
}

impl Focus {
    pub(crate) fn is_running(&self) -> bool {
        matches!(self.phase, Phase::Running { .. })
    }

    /// Running or paused: the peek shows it.
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.phase, Phase::Running { .. } | Phase::Paused { .. })
    }

    fn elapsed(&self, now: Instant) -> Duration {
        match self.phase {
            Phase::Idle => Duration::ZERO,
            Phase::Running { since, banked } => banked + now.saturating_duration_since(since),
            Phase::Paused { banked } | Phase::Done { banked } => banked,
        }
    }

    fn length(&self) -> Option<Duration> {
        (self.length_min > 0).then(|| Duration::from_secs(u64::from(self.length_min) * 60))
    }

    /// When a running countdown reaches zero.
    fn finish_at(&self) -> Option<Instant> {
        match (self.phase, self.length()) {
            (Phase::Running { since, banked }, Some(length)) => {
                Some(since + length.saturating_sub(banked))
            }
            _ => None,
        }
    }

    pub(crate) fn deadlines(&self) -> [Option<Instant>; 3] {
        [self.finish_at(), self.save_at, self.next_second]
    }

    pub(crate) fn set_day(&mut self, day: String, now: Instant) {
        if day != self.day {
            // Seconds before midnight belong to the old day.
            self.credit(now);
            self.day = day;
        }
    }

    pub(crate) fn view_length(&self) -> u32 {
        self.length_min
    }

    pub(crate) fn module(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// Starts or resumes. `calendar` is the module of the class on now, if any.
    pub(crate) fn start(&mut self, calendar: Option<String>, now: Instant) {
        let banked = match self.phase {
            Phase::Running { .. } => return,
            Phase::Paused { banked } => banked,
            Phase::Idle | Phase::Done { .. } => {
                let hand_picked = self
                    .picked_by_hand
                    .is_some_and(|at| now.saturating_duration_since(at) < MANUAL_HOLD);
                if !hand_picked && calendar.is_some() {
                    self.set_module(calendar);
                }
                Duration::ZERO
            }
        };
        self.phase = Phase::Running { since: now, banked };
        self.credited_to = Some(now);
        self.save_at = Some(now + SAVE_EVERY);
    }

    pub(crate) fn pause(&mut self, now: Instant) {
        if let Phase::Running { .. } = self.phase {
            self.credit(now);
            self.phase = Phase::Paused {
                banked: self.elapsed(now),
            };
            self.stop_clocks();
        }
    }

    /// Back to the start. A count-up session of 5 minutes or more counts as a session.
    pub(crate) fn reset(&mut self, now: Instant) {
        self.credit(now);
        let elapsed = self.elapsed(now);
        if self.length_min == 0
            && !matches!(self.phase, Phase::Idle | Phase::Done { .. })
            && elapsed.as_secs() >= COUNT_UP_SESSION_SECS
        {
            self.add_session();
        }
        self.phase = Phase::Idle;
        self.stop_clocks();
    }

    /// Picks a countdown length (or 0 to count up). Only while nothing is running.
    pub(crate) fn set_length(&mut self, minutes: u32, now: Instant) {
        if self.is_active() || !(minutes == 0 || LENGTHS.contains(&minutes)) {
            return;
        }
        self.length_min = minutes;
        if let Phase::Done { .. } = self.phase {
            self.reset(now);
        }
    }

    /// The module chip was pressed: the next choice, ending with no module.
    pub(crate) fn cycle_module(&mut self, calendar: &[String], now: Instant) {
        let choices = self.choices(calendar);
        let next = match &self.module {
            None => choices.first().cloned(),
            Some(current) => {
                let at = choices.iter().position(|c| c == current);
                at.and_then(|i| choices.get(i + 1)).cloned()
            }
        };
        // Time so far goes to the module it was spent on.
        self.credit(now);
        self.set_module(next);
        self.picked_by_hand = Some(now);
    }

    fn set_module(&mut self, module: Option<String>) {
        if let Some(code) = &module
            && !self.log.order.contains(code)
        {
            self.log.order.push(code.clone());
            self.dirty = true;
        }
        self.module = module;
    }

    /// What the chip cycles through: modules from the log, then any new ones from the calendar.
    fn choices(&self, calendar: &[String]) -> Vec<String> {
        let mut all: Vec<String> = self.log.order.clone();
        for code in calendar {
            if !all.contains(code) {
                all.push(code.clone());
            }
        }
        all
    }

    fn stop_clocks(&mut self) {
        self.credited_to = None;
        self.save_at = None;
        self.next_second = None;
    }

    /// Moves the seconds since the last credit into today's log.
    fn credit(&mut self, now: Instant) {
        let (Some(from), Phase::Running { .. }) = (self.credited_to, self.phase) else {
            return;
        };
        let secs = now.saturating_duration_since(from).as_secs();
        if secs == 0 || self.day.is_empty() {
            return;
        }
        let module = self.module.clone().unwrap_or_else(|| NO_MODULE.to_string());
        *self
            .log
            .days
            .entry(self.day.clone())
            .or_default()
            .modules
            .entry(module)
            .or_default() += secs;
        // Keep the part second for next time.
        self.credited_to = Some(from + Duration::from_secs(secs));
        self.prune();
        self.dirty = true;
    }

    fn add_session(&mut self) {
        if self.day.is_empty() {
            return;
        }
        self.log.days.entry(self.day.clone()).or_default().sessions += 1;
        self.dirty = true;
    }

    fn prune(&mut self) {
        while self.log.days.len() > MAX_DAYS {
            let Some(oldest) = self.log.days.keys().next().cloned() else {
                break;
            };
            self.log.days.remove(&oldest);
        }
    }

    /// Runs the timer. `shown` is whether the capsule is on screen, so seconds are only
    /// drawn when someone can see them.
    pub(crate) fn tick(&mut self, shown: bool, now: Instant) -> TickResult {
        let mut result = TickResult::default();
        if self.finish_at().is_some_and(|at| at <= now) {
            let at = self.finish_at().unwrap_or(now);
            self.credit(at);
            self.phase = Phase::Done {
                banked: self.length().unwrap_or_default(),
            };
            self.add_session();
            self.stop_clocks();
            result.finished = true;
            result.redraw = true;
            return result;
        }
        if self.save_at.is_some_and(|at| at <= now) {
            self.credit(now);
            self.save_at = Some(now + SAVE_EVERY);
        }
        if let (true, Phase::Running { since, banked }) = (shown, self.phase) {
            if self.next_second.is_none_or(|at| at <= now) {
                result.redraw = true;
                // The next whole second of the session.
                let into = (banked + now.saturating_duration_since(since)).subsec_nanos();
                let left = Duration::from_nanos(1_000_000_000 - u64::from(into));
                self.next_second = Some(now + left);
            }
        } else {
            self.next_second = None;
        }
        result
    }

    /// The log to save, once per change.
    pub(crate) fn take_save(&mut self) -> Option<String> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        toml::to_string(&self.log).ok()
    }

    /// Reads a saved log. Today's seconds so far are kept if the text can't be read.
    pub(crate) fn load(&mut self, text: &str) -> Result<(), String> {
        let log: StudyLog =
            toml::from_str(text).map_err(|_| "study.toml isn't valid".to_string())?;
        self.log = log;
        self.prune();
        Ok(())
    }

    /// The minutes and seconds the peek shows.
    pub(crate) fn display(&self, now: Instant) -> String {
        let elapsed = self.elapsed(now);
        let secs = match self.length() {
            // Round up, so a countdown never shows 0:00 while it is still running.
            Some(length) => {
                let left = length.saturating_sub(elapsed);
                left.as_secs() + u64::from(left.subsec_nanos() > 0)
            }
            None => elapsed.as_secs(),
        };
        clock_text(secs)
    }

    /// 0 to 1: of the countdown, or of the current hour when counting up.
    pub(crate) fn progress(&self, now: Instant) -> f64 {
        let elapsed = self.elapsed(now).as_secs_f64();
        match self.length() {
            Some(length) => (elapsed / length.as_secs_f64()).clamp(0.0, 1.0),
            None => (elapsed % 3600.0) / 3600.0,
        }
    }

    pub(crate) fn chip(&self, code: &str) -> ModuleChip {
        ModuleChip {
            code: code.to_string(),
            color: self.colour(code),
        }
    }

    fn colour(&self, code: &str) -> String {
        if code == NO_MODULE {
            return "#a0a0a0".into();
        }
        let index = self
            .log
            .order
            .iter()
            .position(|c| c == code)
            .unwrap_or(self.log.order.len());
        COLOURS[index % COLOURS.len()].to_string()
    }

    pub(crate) fn view(&self, now: Instant) -> FocusView {
        let phase = match self.phase {
            Phase::Idle => FocusPhase::Idle,
            Phase::Running { .. } => FocusPhase::Running,
            Phase::Paused { .. } => FocusPhase::Paused,
            Phase::Done { .. } => FocusPhase::Done,
        };
        let caption = match (phase, self.length_min) {
            (FocusPhase::Done, _) => "done".to_string(),
            (_, 0) => "counting up".to_string(),
            (_, minutes) => format!("of {minutes}:00"),
        };
        FocusView {
            phase,
            count_up: self.length_min == 0,
            length_min: self.length_min,
            lengths: LENGTHS.to_vec(),
            display: self.display(now),
            caption,
            progress: self.progress(now),
            module: self.module.as_deref().map(|m| self.chip(m)),
            stats: self.stats(now),
        }
    }

    /// The log with the seconds not yet credited added in, so the numbers move live.
    fn live_log(&self, now: Instant) -> StudyLog {
        let mut log = self.log.clone();
        if let (Some(from), Phase::Running { .. }, false) =
            (self.credited_to, self.phase, self.day.is_empty())
        {
            let secs = now.saturating_duration_since(from).as_secs();
            if secs > 0 {
                let module = self.module.clone().unwrap_or_else(|| NO_MODULE.to_string());
                *log.days
                    .entry(self.day.clone())
                    .or_default()
                    .modules
                    .entry(module)
                    .or_default() += secs;
            }
        }
        log
    }

    fn stats(&self, now: Instant) -> StudyStats {
        let log = self.live_log(now);
        let today = log.days.get(&self.day).cloned().unwrap_or_default();
        let week = week_of(&self.day);
        let totals: Vec<u64> = week
            .iter()
            .map(|d| log.days.get(d).map_or(0, StudyDay::total))
            .collect();
        let busiest = totals.iter().copied().max().unwrap_or(0).max(1);
        const LETTERS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
        let bars = week
            .iter()
            .zip(&totals)
            .enumerate()
            .map(|(i, (day, secs))| DayBar {
                label: LETTERS[i].to_string(),
                height: *secs as f64 / busiest as f64,
                today: *day == self.day,
                time: duration_text(*secs),
            })
            .collect();
        let mut by_module: BTreeMap<String, u64> = BTreeMap::new();
        for day in &week {
            if let Some(entry) = log.days.get(day) {
                for (module, secs) in &entry.modules {
                    *by_module.entry(module.clone()).or_default() += secs;
                }
            }
        }
        let mut ranked: Vec<(String, u64)> =
            by_module.into_iter().filter(|(_, s)| *s > 0).collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let top = ranked.first().map_or(1, |(_, s)| (*s).max(1));
        let modules = ranked
            .iter()
            .take(4)
            .map(|(code, secs)| ModuleRow {
                code: code.clone(),
                color: self.colour(code),
                time: duration_text(*secs),
                share: *secs as f64 / top as f64,
            })
            .collect();
        StudyStats {
            today: duration_text(today.total()),
            week: duration_text(totals.iter().sum()),
            streak: streak(&log, &self.day),
            sessions_today: today.sessions,
            bars,
            modules,
        }
    }
}

impl crate::Core {
    /// The module of the class on now, or starting within 10 minutes, from the calendar.
    fn calendar_module(&self) -> Option<String> {
        let EventsView::Ready(ready) = &self.events else {
            return None;
        };
        let head = ready.head.as_ref()?;
        let soon = head.caption.starts_with("Now")
            || head
                .caption
                .strip_prefix("In ")
                .and_then(|rest| rest.split(' ').next())
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n <= 10);
        if soon { module_code(&head.title) } else { None }
    }

    /// Every module code in the calendar card, in order.
    fn calendar_modules(&self) -> Vec<String> {
        let EventsView::Ready(ready) = &self.events else {
            return Vec::new();
        };
        let titles = ready.head.iter().map(|h| h.title.as_str()).chain(
            ready
                .days
                .iter()
                .flat_map(|d| d.rows.iter().map(|r| r.title.as_str())),
        );
        let mut codes: Vec<String> = Vec::new();
        for code in titles.filter_map(module_code) {
            if !codes.contains(&code) {
                codes.push(code);
            }
        }
        codes
    }

    /// Start, or carry on after a pause.
    pub fn focus_start(&mut self, now: Instant) {
        let calendar = self.calendar_module();
        self.focus.start(calendar, now);
        self.coalescer.mark(now);
    }

    pub fn focus_pause(&mut self, now: Instant) {
        self.focus.pause(now);
        self.coalescer.mark(now);
    }

    /// The play/pause button.
    pub fn focus_toggle(&mut self, now: Instant) {
        if self.focus.is_running() {
            self.focus_pause(now);
        } else {
            self.focus_start(now);
        }
    }

    pub fn focus_reset(&mut self, now: Instant) {
        self.focus.reset(now);
        self.coalescer.mark(now);
    }

    /// 15, 25 or 50 minutes, or 0 to count up.
    pub fn focus_length(&mut self, minutes: u32, now: Instant) {
        self.focus.set_length(minutes, now);
        self.coalescer.mark(now);
    }

    /// The module chip was pressed.
    pub fn focus_cycle_module(&mut self, now: Instant) {
        let calendar = self.calendar_modules();
        self.focus.cycle_module(&calendar, now);
        self.coalescer.mark(now);
    }

    /// Today's local date as `YYYY-MM-DD`, from the engine once a minute.
    pub fn set_day(&mut self, day: String, now: Instant) {
        self.focus.set_day(day, now);
    }

    /// The saved study log, read at start.
    pub fn load_study(&mut self, text: &str) -> Result<(), String> {
        self.focus.load(text)
    }

    /// The study log as TOML when it has changed since the last call, for the engine to save.
    pub fn take_study_save(&mut self) -> Option<String> {
        self.focus.take_save()
    }

    /// A countdown reached zero: pop, with a button to start another.
    pub(crate) fn focus_finished(&mut self, now: Instant) {
        self.coalescer.mark(now);
        if self.settings.do_not_disturb || !self.settings.pop_setting(SOURCE).pop {
            return;
        }
        let minutes = self.focus.view(now).length_min;
        let detail = match self.focus.module() {
            Some(m) => format!("{m} · {minutes} min"),
            None => format!("{minutes} min"),
        };
        let activity = Activity {
            id: ID.into(),
            source: SOURCE.into(),
            title: "Focus session done".into(),
            subtitle: Some(detail),
            state: State::Succeeded,
            progress: None,
            icon: Some("timer".into()),
            expires_in: None,
            actions: vec![Action {
                id: AGAIN.into(),
                label: "Start another".into(),
                kind: ActionKind::Callback,
                primary: true,
            }],
        };
        let key = (SOURCE.to_string(), ID.to_string());
        self.push_pop(key, activity, Some(now + crate::POP_DURATION));
    }

    /// The focus timer as a peek item: its module, the time left and a progress line.
    pub(crate) fn focus_item(&self, now: Instant) -> ItemView {
        let running = self.focus.is_running();
        let display = self.focus.display(now);
        let count_up = self.focus.view_length() == 0;
        ItemView {
            source: SOURCE.into(),
            id: ID.into(),
            source_label: "Focus".into(),
            title: match self.focus.module() {
                Some(m) => format!("Focus · {m}"),
                None => "Focus".into(),
            },
            subtitle: None,
            state: if running { State::Running } else { State::Info },
            progress: (!count_up).then(|| self.focus.progress(now)),
            indeterminate: false,
            icon: "timer".into(),
            elapsed_min: None,
            primary_action: None,
            caption: Some(if running {
                display
            } else {
                format!("{display} · paused")
            }),
        }
    }
}

/// `18:42`, or `1:02:05` from an hour up.
pub fn clock_text(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs / 60) % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// `0 min`, `42 min`, `1 h 20 min`, `3 h`.
pub fn duration_text(secs: u64) -> String {
    let minutes = secs / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// The module code in an event title, like `COS 214` from "COS214 Lecture" or
/// "WTW-285 tutorial": 3 or 4 capital letters, an optional space or dash, 3 digits and
/// an optional capital letter, standing on its own.
pub fn module_code(title: &str) -> Option<String> {
    let chars: Vec<char> = title.chars().collect();
    let is_word = |c: char| c.is_alphanumeric();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_uppercase() || (i > 0 && is_word(chars[i - 1])) {
            i += 1;
            continue;
        }
        let letters = chars[i..]
            .iter()
            .take_while(|c| c.is_ascii_uppercase())
            .count();
        if (3..=4).contains(&letters) {
            let mut j = i + letters;
            if j < chars.len() && (chars[j] == ' ' || chars[j] == '-') {
                j += 1;
            }
            let digits = chars[j..].iter().take_while(|c| c.is_ascii_digit()).count();
            if digits == 3 {
                let mut end = j + 3;
                if end < chars.len() && chars[end].is_ascii_uppercase() {
                    end += 1;
                }
                if end >= chars.len() || !is_word(chars[end]) {
                    let code: String = chars[i..i + letters].iter().collect();
                    let number: String = chars[j..end].iter().collect();
                    return Some(format!("{code} {number}"));
                }
            }
        }
        i += letters.max(1);
    }
    None
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn parse_day(day: &str) -> Option<i64> {
    let mut parts = day.splitn(3, '-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

fn day_key(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The local date as `YYYY-MM-DD`.
pub fn day_text(year: i64, month: u32, day: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

/// Monday to Sunday of the week holding `day`; empty if `day` isn't a date.
fn week_of(day: &str) -> Vec<String> {
    let Some(days) = parse_day(day) else {
        return Vec::new();
    };
    // 1970-01-01 was a Thursday: Monday is 0.
    let weekday = (days + 3).rem_euclid(7);
    let monday = days - weekday;
    (0..7).map(|i| day_key(monday + i)).collect()
}

/// Days in a row with at least 5 minutes, ending today, or yesterday if today has less.
fn streak(log: &StudyLog, today: &str) -> u32 {
    let Some(mut days) = parse_day(today) else {
        return 0;
    };
    let busy = |d: i64| {
        log.days
            .get(&day_key(d))
            .is_some_and(|e| e.total() >= STREAK_SECS)
    };
    if !busy(days) {
        days -= 1;
    }
    let mut count = 0;
    while busy(days) && count < 366 {
        count += 1;
        days -= 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_module_codes_in_event_titles() {
        assert_eq!(module_code("COS 214 Lecture").as_deref(), Some("COS 214"));
        assert_eq!(module_code("COS214 practical").as_deref(), Some("COS 214"));
        assert_eq!(module_code("Tut: WTW-285").as_deref(), Some("WTW 285"));
        assert_eq!(module_code("FRN 104Y oral").as_deref(), Some("FRN 104Y"));
        assert_eq!(module_code("APPM 1004"), None);
        assert_eq!(module_code("Standup with ABCD"), None);
        assert_eq!(module_code("xCOS 214"), None);
        assert_eq!(module_code("COS 2140"), None);
    }

    #[test]
    fn dates_round_trip_and_weeks_start_on_monday() {
        assert_eq!(day_key(parse_day("2026-11-04").unwrap()), "2026-11-04");
        assert_eq!(day_key(parse_day("2024-02-29").unwrap()), "2024-02-29");
        let week = week_of("2026-11-04"); // a Wednesday
        assert_eq!(week.first().map(String::as_str), Some("2026-11-02"));
        assert_eq!(week.last().map(String::as_str), Some("2026-11-08"));
        assert!(week_of("not a date").is_empty());
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_text(59), "0 min");
        assert_eq!(duration_text(42 * 60), "42 min");
        assert_eq!(duration_text(80 * 60), "1 h 20 min");
        assert_eq!(duration_text(3 * 3600), "3 h");
        assert_eq!(clock_text(18 * 60 + 42), "18:42");
        assert_eq!(clock_text(3725), "1:02:05");
    }

    #[test]
    fn the_log_round_trips_through_toml() {
        let mut log = StudyLog::default();
        log.order.push("COS 214".into());
        let day = StudyDay {
            sessions: 2,
            modules: BTreeMap::from([("COS 214".to_string(), 1500)]),
        };
        log.days.insert("2026-11-04".into(), day);
        let text = toml::to_string(&log).unwrap();
        let back: StudyLog = toml::from_str(&text).unwrap();
        assert_eq!(back, log);
    }

    #[test]
    fn the_streak_forgives_a_quiet_today() {
        let mut log = StudyLog::default();
        for d in ["2026-11-01", "2026-11-02", "2026-11-03"] {
            let day = StudyDay {
                sessions: 0,
                modules: BTreeMap::from([("COS 214".to_string(), 600)]),
            };
            log.days.insert(d.into(), day);
        }
        assert_eq!(streak(&log, "2026-11-04"), 3);
        assert_eq!(streak(&log, "2026-11-03"), 3);
        assert_eq!(streak(&log, "2026-11-06"), 0);
    }
}
