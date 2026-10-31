//! The expanded view: when it opens and closes, and the widget state it shows.
//! The pointer, the click and the hotkey only report facts; every timer lives
//! here and runs on the `now` that callers pass in.

use std::time::Instant;

use qapsule_protocol::State;

use crate::settings::ExpandOn;
use crate::view::{EventsView, WeatherView};
use crate::{COLLAPSE_GRACE, Core, EXPAND_DWELL, EXPAND_INTENT_DELAY, POP_DURATION, UNDO_DURATION};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Open {
    #[default]
    Closed,
    /// Opened by hover or click: the pointer leaving closes it.
    Pointer,
    /// Opened by the hotkey: stays until Esc or the hotkey again.
    Pinned,
}

#[derive(Debug, Default)]
pub(crate) struct Expanded {
    open: Open,
    expand_at: Option<Instant>,
    collapse_at: Option<Instant>,
    /// What providers were last told by [`Core::take_main_screen`].
    reported: bool,
    /// Esc or the hotkey closed the view with the pointer over it: hovering doesn't
    /// open it again until the pointer has left.
    suppressed: bool,
}

impl Expanded {
    pub(crate) fn is_open(&self) -> bool {
        self.open != Open::Closed
    }

    pub(crate) fn is_pinned(&self) -> bool {
        self.open == Open::Pinned
    }

    /// A switch to click mode ends a dwell that was already counting.
    pub(crate) fn cancel_dwell(&mut self) {
        self.expand_at = None;
    }

    pub(crate) fn deadlines(&self) -> [Option<Instant>; 2] {
        [self.expand_at, self.collapse_at]
    }
}

/// The weather widget before any provider has spoken.
pub(crate) fn initial_weather(city: Option<&str>) -> WeatherView {
    if city.is_some() {
        WeatherView::Loading
    } else {
        WeatherView::NoCity
    }
}

impl Core {
    /// A pop or the undo bar is on screen (the user is about to press it).
    pub(crate) fn pop_or_undo_showing(&self) -> bool {
        !self.pops.is_empty() || self.undo.is_some()
    }

    /// Whether the pointer may start opening the view by hovering.
    pub(crate) fn can_hover_open(&self) -> bool {
        self.settings.expand_on == ExpandOn::Hover
            && !self.expanded.is_open()
            && !self.expanded.suppressed
            && !self.pop_or_undo_showing()
    }

    /// The pointer came in: start the dwell timer, and cancel a pending collapse.
    pub(crate) fn expanded_pointer_entered(&mut self, now: Instant) {
        self.expanded.collapse_at = None;
        if self.can_hover_open() {
            let at = now + EXPAND_DWELL;
            self.expanded.expand_at = Some(self.expanded.expand_at.map_or(at, |e| e.min(at)));
        }
    }

    /// The pointer left: no more dwell, and a view opened by the pointer closes soon.
    pub(crate) fn expanded_pointer_left(&mut self, now: Instant) {
        self.expanded.expand_at = None;
        self.expanded.suppressed = false;
        if self.expanded.open == Open::Pointer {
            self.expanded.collapse_at = Some(now + COLLAPSE_GRACE);
        }
    }

    /// The pointer slowed down over the capsule: open sooner than the dwell would.
    pub fn pointer_slowed(&mut self, now: Instant) {
        if self.pointer_over && self.can_hover_open() {
            let at = now + EXPAND_INTENT_DELAY;
            self.expanded.expand_at = Some(self.expanded.expand_at.map_or(at, |e| e.min(at)));
        }
    }

    /// The peek was clicked outside any button. Opens the view in click mode only.
    pub fn expand_click(&mut self, now: Instant) {
        if self.settings.expand_on == ExpandOn::Click
            && !self.expanded.is_open()
            && !self.pop_or_undo_showing()
        {
            self.open(Open::Pointer, now);
        }
    }

    /// The hotkey: open the view and keep it open, or close it.
    pub fn toggle_expanded(&mut self, now: Instant) {
        if self.expanded.is_pinned() {
            self.close_by_keyboard(now);
        } else {
            self.open(Open::Pinned, now);
        }
    }

    /// Esc: close the view whichever way it was opened.
    pub fn close_expanded(&mut self, now: Instant) {
        self.close_by_keyboard(now);
    }

    fn close_by_keyboard(&mut self, now: Instant) {
        let was_open = self.expanded.is_open();
        self.close(now);
        // The pointer is probably still resting there; it must not undo the close.
        self.expanded.suppressed = was_open && self.pointer_over;
    }

    fn open(&mut self, how: Open, now: Instant) {
        // The media view gives way to the expanded view.
        self.media_open = false;
        self.media_close_at = None;
        self.expanded.open = how;
        self.expanded.expand_at = None;
        self.expanded.collapse_at = None;
        self.coalescer.mark(now);
    }

    fn close(&mut self, now: Instant) {
        self.expanded.expand_at = None;
        self.expanded.collapse_at = None;
        if !self.expanded.is_open() {
            return;
        }
        self.expanded.open = Open::Closed;
        // Pops that arrived meanwhile have not been seen yet: give them their full time.
        for pop in &mut self.pops {
            if pop.until.is_some() {
                pop.until = Some(now + POP_DURATION);
            }
        }
        // A pop whose activity ran out while nobody could see it is stale; a finished
        // activity's pop is its own copy and still shows.
        let stale: Vec<_> = self
            .pops
            .iter()
            .filter(|pop| {
                !(pop.until.is_none()
                    || crate::media::is_media_key(&pop.key)
                    || matches!(pop.activity.state, State::Succeeded | State::Failed)
                    || self.store.get(&pop.key).is_some())
            })
            .map(|pop| pop.key.clone())
            .collect();
        self.retire_where(|pop| stale.contains(&pop.key));
        // What is left shows now.
        for pop in &mut self.pops {
            pop.shown = true;
        }
        if let Some(undo) = &mut self.undo {
            undo.until = now + UNDO_DURATION;
        }
        self.coalescer.mark(now);
    }

    /// Runs the dwell and collapse timers. Returns whether anything changed.
    pub(crate) fn tick_expanded(&mut self, now: Instant) -> bool {
        if self.expanded.collapse_at.is_some_and(|at| at <= now) {
            self.close(now);
            return true;
        }
        if self.expanded.expand_at.is_some_and(|at| at <= now) {
            self.expanded.expand_at = None;
            // The conditions may have changed since the timer started.
            if self.pointer_over && self.can_hover_open() {
                self.open(Open::Pointer, now);
                return true;
            }
        }
        false
    }

    /// Whether the main screen opened or closed since the last call: `Some(open)`
    /// once per change, so providers can refresh on open (decision 0007).
    pub fn take_main_screen(&mut self) -> Option<bool> {
        let open = self.expanded.is_open();
        (open != self.expanded.reported).then(|| {
            self.expanded.reported = open;
            open
        })
    }

    /// A provider has new weather. Only an open view needs to hear about it.
    pub fn set_weather(&mut self, weather: WeatherView, now: Instant) {
        self.weather = weather;
        if self.expanded.is_open() {
            self.coalescer.mark(now);
        }
    }

    /// A provider has new calendar events. Only an open view needs to hear about it.
    pub fn set_events(&mut self, events: EventsView, now: Instant) {
        self.events = events;
        if self.expanded.is_open() {
            self.coalescer.mark(now);
        }
    }
}
