//! The heart of Qapsule: the activity store, pop rules, visibility and the UI
//! update coalescer. Plain Rust with no window and no real clock: every method
//! that depends on time takes `now`, so tests can drive time by hand.

mod expanded;
mod media;
pub mod presence;
pub mod settings;
pub mod store;
pub mod timing;
pub mod view;

use std::time::{Duration, Instant};

use qapsule_protocol::{
    ActionKind, Activity, ClientMessage, ErrorCode, ProtocolError, ServerMessage, State,
    is_reserved_source,
};

pub use media::{MediaState, Timeline};
pub use presence::{Presence, PresenceEffect};
pub use settings::{ExpandOn, Hotkey, HotkeyKey, Screen, WidgetConfig, WidgetKind, WidgetSize};
pub use settings::{PopSetting, Settings, VisibilityMode};
pub use store::{Entry, Key, Store};
pub use timing::{Coalescer, RateLimiter};
pub use view::{
    ActionView, EventNext, EventNow, EventsReady, EventsView, ExpandedView, HourView, ItemView,
    MusicView, Peek, PeekItem, PopView, UiState, WeatherReady, WeatherView,
};

/// Identifies one pipe connection.
pub type ConnId = u64;

/// How long a pop shows before shrinking back to the peek.
pub const POP_DURATION: Duration = Duration::from_secs(5);
/// How long "Notification dismissed · Undo" stays.
pub const UNDO_DURATION: Duration = Duration::from_secs(4);
/// How long a hover-revealed peek stays after the pointer leaves.
pub const HOVER_GRACE: Duration = Duration::from_millis(600);
/// How long the pointer must rest on the peek before it expands.
pub const EXPAND_DWELL: Duration = Duration::from_millis(180);
/// How long after the pointer slows down over the peek it expands.
pub const EXPAND_INTENT_DELAY: Duration = Duration::from_millis(60);
/// How long the expanded view stays after the pointer leaves it.
pub const COLLAPSE_GRACE: Duration = Duration::from_millis(300);
/// How long touching the top edge brings a tucked capsule out for. Long enough for
/// a destroyed webview to come back and notice the pointer over it.
pub const EDGE_REVEAL: Duration = Duration::from_secs(2);
/// At most 4 UI updates per second, whatever clients send.
pub const UI_UPDATE_INTERVAL: Duration = Duration::from_millis(250);
/// Older pops waiting behind the current one are dropped past this.
const MAX_POPS: usize = 8;

/// Something the app shell must do after a button press.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    OpenUrl(String),
    FocusWindow(u32),
    /// A button on a provider activity: the engine hands it to the provider.
    Provider {
        source: String,
        id: String,
        action: String,
    },
    /// Send `message` to the pipe connection `conn`.
    Callback {
        conn: ConnId,
        message: ServerMessage,
    },
}

#[derive(Debug, Clone)]
struct Pop {
    key: Key,
    /// A copy, so a finished activity's pop outlives the activity by a second.
    activity: Activity,
    /// `None` for pops that stay until handled.
    until: Option<Instant>,
    seq: u64,
}

/// One thing the peek can rank.
enum Candidate<'a> {
    Activity(&'a Entry),
    Music(&'a MediaState),
}

#[derive(Debug)]
struct Undo {
    until: Instant,
    pop: Option<Pop>,
    removed: Option<(Key, Entry)>,
}

pub struct Core {
    store: Store,
    settings: Settings,
    /// Waiting pops; the last one is on screen.
    pops: Vec<Pop>,
    pop_seq: u64,
    /// The top two peek items, when the user has swapped them.
    swapped: Option<(Key, Key)>,
    undo: Option<Undo>,
    clock: String,
    date: String,
    fullscreen: bool,
    expanded: expanded::Expanded,
    weather: WeatherView,
    events: EventsView,
    /// What Windows is playing, as the media provider last said.
    media: Option<MediaState>,
    /// The last song seen (title, artist), kept while nothing plays, to spot a change.
    last_song: Option<(String, String)>,
    /// The music state the UI was last told about, to judge whether a new report is news.
    media_shown: Option<MediaState>,
    /// While set, the peek shows the full music pill: the song notice (decision 0009).
    notice_until: Option<Instant>,
    /// A new song waiting for the expanded view to close before it is announced.
    notice_pending: Option<(String, String)>,
    pointer_over: bool,
    hover_until: Option<Instant>,
    coalescer: Coalescer,
    presence: Presence,
}

impl Core {
    pub fn new(settings: Settings, now: Instant) -> Self {
        let weather = expanded::initial_weather(settings.weather_city());
        let mut core = Self {
            store: Store::default(),
            settings,
            pops: Vec::new(),
            pop_seq: 0,
            swapped: None,
            undo: None,
            clock: String::new(),
            date: String::new(),
            fullscreen: false,
            expanded: expanded::Expanded::default(),
            weather,
            // The calendar provider says `NoLink` when no link is set.
            events: EventsView::Loading,
            media: None,
            last_song: None,
            media_shown: None,
            notice_until: None,
            notice_pending: None,
            pointer_over: false,
            hover_until: None,
            coalescer: Coalescer::new(UI_UPDATE_INTERVAL),
            presence: Presence::new(),
        };
        core.coalescer.mark(now);
        core
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Handles one message from a client and returns the reply.
    pub fn handle(&mut self, conn: ConnId, message: ClientMessage, now: Instant) -> ServerMessage {
        match message {
            ClientMessage::Hello { .. } => ServerMessage::Ok,
            ClientMessage::List { source } => ServerMessage::ListResult {
                activities: self
                    .store
                    .list(source.as_deref(), now)
                    .into_iter()
                    .map(|a| self.hide_text(a))
                    .collect(),
            },
            ClientMessage::Upsert { activity } => {
                let key = store::key_of(&activity);
                match self.store.upsert(activity, Some(conn), now) {
                    Ok(previous) => {
                        self.changed(key, previous, now);
                        ServerMessage::Ok
                    }
                    Err(e) => e.into(),
                }
            }
            ClientMessage::End {
                source,
                id,
                state,
                title,
                subtitle,
            } => {
                let key = (source, id);
                match self.store.end(&key, state, title, subtitle, now) {
                    Ok(previous) => {
                        self.changed(key, Some(previous), now);
                        ServerMessage::Ok
                    }
                    Err(e) => e.into(),
                }
            }
            ClientMessage::Clear { source, id } => {
                let removed = self.store.clear(source.as_deref(), id.as_deref());
                self.pops.retain(|p| !removed.contains(&p.key));
                self.coalescer.mark(now);
                ServerMessage::Ok
            }
        }
    }

    /// Adds or updates an activity from an in-process provider. Only reserved
    /// sources are accepted. At the per-source limit the oldest activity of the
    /// source makes room.
    pub fn provider_upsert(
        &mut self,
        activity: Activity,
        now: Instant,
    ) -> Result<(), ProtocolError> {
        if !is_reserved_source(&activity.source) {
            return Err(ProtocolError::new(
                ErrorCode::InvalidMessage,
                "providers may only use reserved sources",
            ));
        }
        activity.validate()?;
        let key = store::key_of(&activity);
        let previous = match self.store.upsert(activity.clone(), None, now) {
            Ok(previous) => previous,
            Err(e) if e.code == ErrorCode::TooManyActivities => {
                if let Some(oldest) = self.store.oldest_in(&key.0) {
                    self.store.remove(&oldest);
                    self.pops.retain(|p| p.key != oldest);
                    // Mark now, so a failed retry still leaves a consistent UI.
                    self.coalescer.mark(now);
                }
                self.store.upsert(activity, None, now)?
            }
            Err(e) => return Err(e),
        };
        self.changed(key, previous, now);
        Ok(())
    }

    /// Removes a provider's activity and its pops. A missing activity is a no-op.
    pub fn provider_remove(&mut self, source: &str, id: &str, now: Instant) {
        let key = (source.to_string(), id.to_string());
        if self.store.remove(&key).is_some() {
            self.pops.retain(|p| p.key != key);
        }
        if let Some(undo) = &mut self.undo {
            if undo.removed.as_ref().is_some_and(|(k, _)| *k == key) {
                undo.removed = None;
            }
            if undo.pop.as_ref().is_some_and(|p| p.key == key) {
                undo.pop = None;
            }
            if undo.removed.is_none() && undo.pop.is_none() {
                self.undo = None;
            }
        }
        self.coalescer.mark(now);
    }

    /// Whether a source's text must be hidden everywhere: a provider source with details off.
    /// Music is not in the store; its peek view and pop apply the same setting themselves.
    fn text_hidden(&self, source: &str) -> bool {
        is_reserved_source(source) && !self.settings.pop_setting(source).details
    }

    /// Replaces a provider activity's text with the generic line when details are off.
    fn hide_text(&self, mut activity: Activity) -> Activity {
        if self.text_hidden(&activity.source) {
            activity.title = view::generic_title(&activity.source, activity.state).into();
            activity.subtitle = None;
        }
        activity
    }

    /// Applies the pop rules after an activity was added or changed.
    fn changed(&mut self, key: Key, previous: Option<State>, now: Instant) {
        self.coalescer.mark(now);
        let Some(entry) = self.store.get(&key) else {
            return;
        };
        let activity = entry.activity.clone();
        let state = activity.state;

        // Keep a showing pop current, and end a waiting pop once it is answered.
        for pop in self.pops.iter_mut().filter(|p| p.key == key) {
            pop.activity = activity.clone();
        }
        if state != State::NeedsInput {
            self.pops.retain(|p| p.key != key || p.until.is_some());
        }

        let pops = match state {
            State::NeedsInput | State::Failed | State::Succeeded => previous != Some(state),
            State::Info => previous.is_none(),
            State::Running => false,
        };
        if !pops || self.settings.do_not_disturb || !self.settings.pop_setting(&key.0).pop {
            return;
        }
        let until = (state != State::NeedsInput).then_some(now + POP_DURATION);
        self.push_pop(key, activity, until);
    }

    /// Shows `activity` as a pop in place of any earlier pop for `key`.
    fn push_pop(&mut self, key: Key, activity: Activity, until: Option<Instant>) {
        self.pops.retain(|p| p.key != key);
        self.pop_seq += 1;
        self.pops.push(Pop {
            key,
            activity,
            until,
            seq: self.pop_seq,
        });
        if self.pops.len() > MAX_POPS {
            // Drop the oldest timed pop; a pop waiting for input goes only if all are.
            let victim = self.pops.iter().position(|p| p.until.is_some());
            self.pops.remove(victim.unwrap_or(0));
        }
    }

    /// A client disconnected. Its activities stay, but can no longer call it back.
    pub fn connection_closed(&mut self, conn: ConnId) {
        self.store.forget_owner(conn);
    }

    /// A button was pressed on an activity or its pop.
    pub fn press(
        &mut self,
        source: &str,
        id: &str,
        action_id: &str,
        now: Instant,
    ) -> Option<Effect> {
        let key = (source.to_string(), id.to_string());
        if media::is_media_key(&key) {
            return self.press_media(action_id);
        }
        let entry = self.store.get(&key);
        let activity = entry
            .map(|e| &e.activity)
            .or_else(|| self.pops.iter().find(|p| p.key == key).map(|p| &p.activity))?;
        let action = activity.actions.iter().find(|a| a.id == action_id)?;
        let effect = match &action.kind {
            ActionKind::OpenUrl { url } => Some(Effect::OpenUrl(url.clone())),
            ActionKind::FocusWindow { pid } => Some(Effect::FocusWindow(*pid)),
            ActionKind::Callback if is_reserved_source(source) => Some(Effect::Provider {
                source: key.0.clone(),
                id: key.1.clone(),
                action: action.id.clone(),
            }),
            ActionKind::Callback => entry.and_then(|e| e.owner).map(|conn| Effect::Callback {
                conn,
                message: ServerMessage::Action {
                    source: key.0.clone(),
                    id: key.1.clone(),
                    action: action.id.clone(),
                },
            }),
            ActionKind::Dismiss => {
                let removed = self.store.remove(&key).map(|e| (key.clone(), e));
                let pop = self.take_pop(&key);
                self.undo = Some(Undo {
                    until: now + UNDO_DURATION,
                    pop,
                    removed,
                });
                self.coalescer.mark(now);
                return None;
            }
        };
        // Any other button counts as handling the pop.
        self.take_pop(&key);
        self.coalescer.mark(now);
        effect
    }

    fn take_pop(&mut self, key: &Key) -> Option<Pop> {
        let index = self.pops.iter().rposition(|p| &p.key == key)?;
        Some(self.pops.remove(index))
    }

    /// Hides the current pop, leaving its activity in place. Can be undone.
    pub fn dismiss_pop(&mut self, now: Instant) {
        if self.expanded.is_open() {
            return;
        }
        if let Some(pop) = self.pops.pop() {
            self.undo = Some(Undo {
                until: now + UNDO_DURATION,
                pop: Some(pop),
                removed: None,
            });
            self.coalescer.mark(now);
        }
    }

    /// Brings back whatever the last dismissal removed.
    pub fn undo(&mut self, now: Instant) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        if let Some((key, entry)) = undo.removed {
            self.store.restore(key, entry);
        }
        if let Some(mut pop) = undo.pop {
            // A pop that came after the dismissal is the newer one; never show two.
            self.pops.retain(|p| p.key != pop.key);
            if pop.until.is_some() {
                pop.until = Some(now + POP_DURATION);
            }
            self.pops.push(pop);
        }
        self.coalescer.mark(now);
    }

    /// Swaps the ¾ and ¼ items of the dual peek, or swaps them back.
    pub fn swap_peek(&mut self, now: Instant) {
        let ranked = self.peek_ranked();
        // With music the ¼ is always the music and the ¾ the top activity; the time has
        // nothing to swap with.
        let pair = if self.media.is_some() && self.settings.dual_status {
            ranked
                .iter()
                .find(|(k, _)| !media::is_media_key(k))
                .map(|(k, _)| (k.clone(), media::media_key()))
        } else if let [(a, _), (b, _), ..] = ranked.as_slice() {
            Some((a.clone(), b.clone()))
        } else {
            None
        };
        if let Some(pair) = pair {
            self.swapped = if self.swapped.as_ref() == Some(&pair) {
                None
            } else {
                Some(pair)
            };
            self.coalescer.mark(now);
        }
    }

    /// The pointer entered or left the capsule (or the strip it hides in).
    pub fn set_pointer(&mut self, over: bool, now: Instant) {
        if over {
            self.pointer_over = true;
            self.hover_until = None;
            self.expanded_pointer_entered(now);
        } else if self.pointer_over {
            self.pointer_over = false;
            self.hover_until = Some(now + HOVER_GRACE);
            self.expanded_pointer_left(now);
        }
        self.coalescer.mark(now);
    }

    /// The pointer touched the strip at the top edge where a tucked capsule hides.
    /// The capsule comes out for [`EDGE_REVEAL`], or for as long as the pointer is then over it.
    pub fn reveal_from_edge(&mut self, now: Instant) {
        let until = now + EDGE_REVEAL;
        self.hover_until = Some(self.hover_until.map_or(until, |u| u.max(until)));
        self.coalescer.mark(now);
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, now: Instant) {
        if self.fullscreen != fullscreen {
            self.fullscreen = fullscreen;
            self.coalescer.mark(now);
        }
    }

    pub fn set_clock(&mut self, clock: String, date: String, now: Instant) {
        self.clock = clock;
        self.date = date;
        // Also refreshes elapsed minutes.
        self.coalescer.mark(now);
    }

    pub fn set_settings(&mut self, settings: Settings, now: Instant) {
        let dnd_ended = self.settings.do_not_disturb && !settings.do_not_disturb;
        let city_changed = self.settings.weather_city() != settings.weather_city();
        self.settings = settings;
        if city_changed {
            self.weather = expanded::initial_weather(self.settings.weather_city());
        }
        if self.settings.expand_on != ExpandOn::Hover {
            self.expanded.cancel_dwell();
        }
        if self.settings.do_not_disturb {
            self.pops.clear();
        } else if dnd_ended {
            self.pop_held(now);
        }
        self.coalescer.mark(now);
    }

    /// After Do Not Disturb, pop whatever is still waiting for input.
    fn pop_held(&mut self, now: Instant) {
        let waiting: Vec<Key> = self
            .store
            .ranked()
            .into_iter()
            .filter(|(k, e)| {
                e.activity.state == State::NeedsInput && self.settings.pop_setting(&k.0).pop
            })
            .map(|(k, _)| k.clone())
            .collect();
        for key in waiting.into_iter().rev() {
            self.changed(key, None, now);
        }
    }

    /// Removes whatever has run out of time.
    pub fn tick(&mut self, now: Instant) {
        let expired = self.store.expire(now);
        let mut changed = !expired.is_empty();
        let before = self.pops.len();
        // A finished activity's pop keeps its own timer; a waiting one goes with its activity.
        // While the view is open nobody sees the pops, so timed ones wait.
        let open = self.expanded.is_open();
        self.pops.retain(|p| {
            p.until
                .map_or(!expired.contains(&p.key), |until| open || until > now)
        });
        changed |= self.pops.len() != before;
        if !open && self.undo.as_ref().is_some_and(|u| u.until <= now) {
            self.undo = None;
            changed = true;
        }
        if self.notice_until.is_some_and(|until| until <= now) {
            self.notice_until = None;
            changed = true;
        }
        if !open && self.notice_pending.is_some() {
            changed = true;
            self.start_pending_notice(now);
        }
        if self.hover_until.is_some_and(|until| until <= now) {
            self.hover_until = None;
            changed = true;
        }
        changed |= self.tick_expanded(now);
        if changed {
            self.coalescer.mark(now);
        }
    }

    /// The next moment [`Core::tick`] or [`Core::take_update`] has work to do.
    pub fn next_deadline(&self) -> Option<Instant> {
        [
            self.store.next_expiry(),
            // Timed pops are paused while the view is open.
            self.pops
                .iter()
                .filter_map(|p| p.until)
                .filter(|_| !self.expanded.is_open())
                .min(),
            self.undo
                .as_ref()
                .map(|u| u.until)
                .filter(|_| !self.expanded.is_open()),
            self.hover_until,
            self.notice_until,
            self.coalescer.due(),
            self.presence.next_deadline(),
        ]
        .into_iter()
        .chain(self.expanded.deadlines())
        .flatten()
        .min()
    }

    /// The next UI update, if one is owed and the rate limit allows it.
    pub fn take_update(&mut self, now: Instant) -> Option<UiState> {
        self.coalescer.take(now).then(|| self.snapshot(now))
    }

    /// What the window must do now that time has moved on and commands have run.
    /// Window commands, not UI updates: they skip the coalescer. Call after
    /// [`Core::tick`], which clears the timers that `shown` depends on.
    pub fn take_presence(&mut self, now: Instant) -> Vec<PresenceEffect> {
        let shown = self.shown();
        let mut effects = self.presence.observe(shown, now);
        effects.extend(self.presence.tick(now));
        effects
    }

    /// The window reports that the webview is gone: asks for a new one if the
    /// capsule should be on screen.
    pub fn webview_gone(&mut self, now: Instant) -> Vec<PresenceEffect> {
        // Its pointer events are lost with it: the pointer has left.
        if self.pointer_over {
            self.pointer_over = false;
            self.expanded_pointer_left(now);
        }
        self.presence.webview_gone(now)
    }

    /// Whether the capsule is on screen.
    fn shown(&self) -> bool {
        match self.settings.visibility {
            VisibilityMode::Always => true,
            VisibilityMode::Auto => !self.fullscreen || self.brought_out(),
            VisibilityMode::OnDemand => self.brought_out(),
        }
    }

    fn brought_out(&self) -> bool {
        let hovering = self.pointer_over || self.hover_until.is_some();
        !self.pops.is_empty()
            || self.undo.is_some()
            || self.notice_until.is_some()
            || self.expanded.is_open()
            || hovering
    }

    /// The full UI state right now, for a UI that has just started.
    pub fn snapshot(&self, now: Instant) -> UiState {
        // Pops and the undo bar wait behind the expanded view.
        let open = self.expanded.is_open();
        let pop = self.pops.last().filter(|_| !open).map(|p| self.pop_view(p));
        let undo = self.undo.is_some() && !open;
        UiState {
            clock: self.clock.clone(),
            mode: self.settings.visibility,
            shown: self.shown(),
            peek: self.peek(now),
            pop,
            undo,
            expanded: open.then(|| ExpandedView {
                date: self.date.clone(),
                weather: self.weather.clone(),
                events: self.events.clone(),
            }),
            focus: self.expanded.is_pinned(),
        }
    }

    /// What the peek ranks: pushed activities and the music. Provider notifications
    /// are pop-only, though they stay in the store (and in `list`). Music sits
    /// between running activities with progress and those without.
    fn peek_ranked(&self) -> Vec<(Key, Candidate<'_>)> {
        let mut ranked: Vec<(Key, Candidate)> = self
            .store
            .ranked()
            .into_iter()
            .filter(|(k, _)| !is_reserved_source(&k.0))
            .map(|(k, e)| (k.clone(), Candidate::Activity(e)))
            .collect();
        if let Some(media) = &self.media {
            let at = ranked
                .iter()
                .position(
                    |(_, c)| matches!(c, Candidate::Activity(e) if store::rank(&e.activity) >= 3),
                )
                .unwrap_or(ranked.len());
            ranked.insert(at, (media::media_key(), Candidate::Music(media)));
        }
        ranked
    }

    /// Whether music would be the peek's top item: nothing needs input, has failed or is
    /// running with progress.
    fn music_on_top(&self) -> bool {
        self.store
            .ranked()
            .into_iter()
            .filter(|(k, _)| !is_reserved_source(&k.0))
            .all(|(_, e)| store::rank(&e.activity) >= 3)
    }

    fn peek(&self, now: Instant) -> Peek {
        let details = self.settings.pop_setting(media::SOURCE).details;
        let activity_item = |entry: &Entry| {
            let elapsed = matches!(entry.activity.state, State::Running | State::NeedsInput)
                .then(|| now.saturating_duration_since(entry.started).as_secs() / 60);
            PeekItem::Activity(view::item_view(&entry.activity, elapsed))
        };
        // The hover form is allowed on a peek whose only item is music, with nothing on
        // top of it. The pointer is the UI's to watch: it shows the form at once.
        let closed = !self.expanded.is_open() && self.pops.is_empty() && self.undo.is_none();
        let ranked = self.peek_ranked();

        if let Some(media) = self.media.as_ref().filter(|_| self.settings.dual_status) {
            let music =
                |controls| PeekItem::Music(media::music_view(media, controls, details, now));
            // The song notice: the full pill while it lasts, if music is still on top.
            if self.notice_until.is_some() && self.music_on_top() {
                return Peek::One {
                    main: music(closed),
                };
            }
            // Otherwise music is the ¼, beside the top activity or the time.
            return match ranked.iter().find(|(k, _)| !media::is_media_key(k)) {
                None => Peek::Dual {
                    main: PeekItem::Time,
                    side: music(closed),
                },
                Some((key, Candidate::Activity(entry))) => {
                    let swapped = self.swapped.as_ref() == Some(&(key.clone(), media::media_key()));
                    let (main, side) = if swapped {
                        (music(false), activity_item(entry))
                    } else {
                        (activity_item(entry), music(false))
                    };
                    Peek::Dual { main, side }
                }
                Some(_) => unreachable!("only activities are not the music key"),
            };
        }

        let item = |candidate: &Candidate, controls: bool| match candidate {
            Candidate::Activity(entry) => activity_item(entry),
            Candidate::Music(media) => {
                PeekItem::Music(media::music_view(media, controls, details, now))
            }
        };
        match ranked.as_slice() {
            [] => Peek::Idle,
            [(_, only)] => Peek::One {
                main: item(only, closed),
            },
            [(_, first), ..] if !self.settings.dual_status => Peek::One {
                main: item(first, false),
            },
            [(a, first), (b, second), ..] => {
                let swapped = self.swapped.as_ref() == Some(&(a.clone(), b.clone()));
                let (main, side) = if swapped {
                    (second, first)
                } else {
                    (first, second)
                };
                Peek::Dual {
                    main: item(main, false),
                    side: item(side, false),
                }
            }
        }
    }

    fn pop_view(&self, pop: &Pop) -> PopView {
        let activity = &pop.activity;
        let details = self.settings.pop_setting(&activity.source).details;
        let primary = view::primary_action(activity).map(|a| a.id.as_str());
        PopView {
            source: activity.source.clone(),
            id: activity.id.clone(),
            source_label: view::source_label(&activity.source),
            title: if details {
                activity.title.clone()
            } else {
                view::generic_title(&activity.source, activity.state).into()
            },
            subtitle: if details {
                activity.subtitle.clone()
            } else {
                None
            },
            state: activity.state,
            icon: view::icon_for(activity),
            actions: activity
                .actions
                .iter()
                .map(|a| view::action_view(a, Some(a.id.as_str()) == primary))
                .collect(),
            sticky: pop.until.is_none(),
            seq: pop.seq,
        }
    }
}

#[cfg(test)]
mod tests;
