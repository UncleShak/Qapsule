//! The media provider's logic, free of Windows: which state to report, when to
//! read the cover, and how a burst of player events becomes one update. The
//! Windows session sits behind [`Source`], so tests drive it with a fake.
//! Titles, artists and app names are never logged here.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, sync_channel};
use std::time::{Duration, Instant};

use qapsule_core::{MediaState, Timeline};

use super::art::{ArtStore, Cover};

/// Longest text the provider passes on; players can send anything.
const MAX_TEXT_CHARS: usize = 200;
/// A timeline last updated longer ago than this is not trusted to have moved on.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// The longest a burst of events is held back.
const MAX_HOLD: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    Playing,
    Paused,
    /// Stopped or closed: nothing to show.
    Stopped,
    /// Between songs or opening: wait for the next event.
    Changing,
}

/// What a session says about the position, before it is brought up to date.
#[derive(Debug, Clone, PartialEq)]
pub struct RawTimeline {
    pub start: Duration,
    pub end: Duration,
    pub position: Duration,
    /// How long ago the player stamped `position`.
    pub age: Duration,
    pub rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playback: Playback,
    pub timeline: Option<RawTimeline>,
}

/// What reading the session gave.
#[derive(Debug, Clone, PartialEq)]
pub enum Read {
    Session(Reading),
    /// Windows has no current session.
    NoSession,
    /// The player didn't answer in time, or the read failed: keep what was reported.
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    PlayPause,
    Next,
    Previous,
}

/// The current media session, however it is reached.
pub trait Source {
    /// Windows' current session changed (or this is the start): follow the new one.
    fn session_changed(&mut self);
    /// The current session's state.
    fn read(&mut self) -> Read;
    /// The current session's cover as PNG, at most 256 x 256; `None` if it has none or can't be read.
    fn read_art(&mut self) -> Option<Cover>;
    fn control(&mut self, control: Control);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    SessionChanged,
    /// Title, artist or cover changed.
    Properties,
    Playback,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    Event(Event),
    Control(Control),
}

const EVENTS: [Event; 4] = [
    Event::SessionChanged,
    Event::Properties,
    Event::Playback,
    Event::Timeline,
];

/// More messages than this wait for the worker only if buttons are pressed in a flood.
const QUEUE: usize = 16;

type Pending = [AtomicBool; 4];

fn slot(event: Event) -> usize {
    EVENTS.iter().position(|e| *e == event).unwrap_or(0)
}

/// The way Windows' event handlers (and the buttons) reach the worker without
/// growing memory: at most one wake per kind of event is ever queued, because
/// the worker re-reads the whole state on a wake anyway.
#[derive(Clone)]
pub struct Wakes {
    pending: Arc<Pending>,
    tx: SyncSender<Msg>,
}

/// The worker's end of [`Wakes`].
pub struct Inbox {
    pending: Arc<Pending>,
    rx: Receiver<Msg>,
}

impl Wakes {
    pub fn new() -> (Wakes, Inbox) {
        let (tx, rx) = sync_channel(QUEUE);
        let pending = Arc::new(Pending::default());
        (
            Wakes {
                pending: pending.clone(),
                tx,
            },
            Inbox { pending, rx },
        )
    }

    /// An event happened. Queues a wake unless one of this kind is already waiting.
    pub fn raise(&self, event: Event) {
        if !self.pending[slot(event)].swap(true, Ordering::AcqRel)
            && self.tx.try_send(Msg::Event(event)).is_err()
        {
            // The queue is full of button presses; the next event will wake the worker.
            self.pending[slot(event)].store(false, Ordering::Release);
        }
    }

    /// A button was pressed. Dropped if a flood of presses has filled the queue.
    pub fn control(&self, control: Control) {
        let _ = self.tx.try_send(Msg::Control(control));
    }
}

impl Inbox {
    /// The worker is about to read the state: the next event of this kind queues a new wake.
    fn taken(&self, event: Event) {
        self.pending[slot(event)].store(false, Ordering::Release);
    }
}

/// What a burst of events held.
#[derive(Debug, Default, Clone, Copy)]
pub struct Batch {
    pub session: bool,
    pub properties: bool,
}

impl Batch {
    fn add(&mut self, event: Event) {
        match event {
            Event::SessionChanged => self.session = true,
            Event::Properties => self.properties = true,
            Event::Playback | Event::Timeline => {}
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Tell the core: the state, or nothing playing.
    Set(Option<MediaState>),
    /// A transition; the next event decides.
    Keep,
}

pub struct Tracker<S> {
    source: S,
    art: Arc<ArtStore>,
    /// Title, artist and app of the song the cover belongs to.
    song: Option<(String, String, String)>,
    art_url: Option<String>,
    art_tint: Option<[u8; 3]>,
}

fn clip(text: String) -> String {
    let text = text.trim();
    match text.char_indices().nth(MAX_TEXT_CHARS) {
        Some((end, _)) => text[..end].to_string(),
        None => text.to_string(),
    }
}

/// The playback rate to pass on: a finite number from 0 to 8, or 1.0 if the player sent nonsense.
fn speed(rate: f64) -> f64 {
    if rate.is_finite() {
        rate.clamp(0.0, 8.0)
    } else {
        1.0
    }
}

/// The position now, from what the player stamped some time ago.
fn timeline(raw: &RawTimeline, playing: bool, now: Instant) -> Option<Timeline> {
    let length = raw.end.checked_sub(raw.start)?;
    if length.is_zero() {
        return None;
    }
    let mut position = raw.position.saturating_sub(raw.start);
    if playing && raw.age <= MAX_AGE {
        position += raw.age.mul_f64(speed(raw.rate));
    }
    Some(Timeline {
        position: position.min(length),
        length,
        read_at: now,
        rate: speed(raw.rate),
    })
}

impl<S: Source> Tracker<S> {
    pub fn new(source: S, art: Arc<ArtStore>) -> Self {
        Self {
            source,
            art,
            song: None,
            art_url: None,
            art_tint: None,
        }
    }

    pub fn control(&mut self, control: Control) {
        self.source.control(control);
    }

    fn nothing(&mut self) -> Outcome {
        self.song = None;
        self.art_url = None;
        self.art_tint = None;
        self.art.clear();
        Outcome::Set(None)
    }

    /// Reads the session after a burst of events and says what to report.
    pub fn refresh(&mut self, batch: Batch, now: Instant) -> Outcome {
        if batch.session {
            self.source.session_changed();
            // The new session's cover is not the old one's.
            self.song = None;
        }
        let reading = match self.source.read() {
            Read::Session(reading) => reading,
            Read::NoSession => return self.nothing(),
            Read::Unavailable => return Outcome::Keep,
        };
        let playing = match reading.playback {
            Playback::Stopped => return self.nothing(),
            Playback::Changing => return Outcome::Keep,
            Playback::Playing => true,
            Playback::Paused => false,
        };
        let (title, artist, app) = (clip(reading.title), clip(reading.artist), clip(reading.app));
        if title.is_empty() && artist.is_empty() {
            return self.nothing();
        }

        let song = (title.clone(), artist.clone(), app.clone());
        let same_song = self.song.as_ref() == Some(&song);
        if batch.properties || !same_song {
            match self.source.read_art() {
                Some(cover) => {
                    self.art_url = Some(self.art.set(cover.png));
                    self.art_tint = cover.tint;
                }
                // A cover that can't be read right now stays for the same song.
                None if !same_song => {
                    self.art.clear();
                    self.art_url = None;
                    self.art_tint = None;
                }
                None => {}
            }
        }
        self.song = Some(song);

        Outcome::Set(Some(MediaState {
            app,
            title,
            artist,
            art: self.art_url.clone(),
            art_tint: self.art_url.as_ref().and(self.art_tint),
            playing,
            timeline: reading
                .timeline
                .as_ref()
                .and_then(|raw| timeline(raw, playing, now)),
        }))
    }
}

/// Follows the session until the channel closes. A burst of events (a song
/// change fires several) waits until `quiet` passes without another, then is
/// read once. Controls run at once. `emit` gets each state to report.
pub fn run<S: Source>(
    source: S,
    inbox: Inbox,
    art: Arc<ArtStore>,
    quiet: Duration,
    mut emit: impl FnMut(Option<MediaState>),
) {
    let mut tracker = Tracker::new(source, art);
    let mut report = |tracker: &mut Tracker<S>, batch: Batch| {
        if let Outcome::Set(state) = tracker.refresh(batch, Instant::now()) {
            emit(state);
        }
    };
    report(
        &mut tracker,
        Batch {
            session: true,
            properties: true,
        },
    );
    loop {
        let Ok(first) = inbox.rx.recv() else {
            return;
        };
        let mut batch = Batch::default();
        let mut any = false;
        let mut open = true;
        let started = Instant::now();
        let mut next = Some(first);
        while let Some(msg) = next.take() {
            match msg {
                Msg::Event(event) => {
                    inbox.taken(event);
                    batch.add(event);
                    any = true;
                }
                Msg::Control(control) => tracker.control(control),
            }
            let Some(left) = MAX_HOLD.checked_sub(started.elapsed()) else {
                break;
            };
            match inbox.rx.recv_timeout(quiet.min(left)) {
                Ok(msg) => next = Some(msg),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => open = false,
            }
        }
        if any {
            report(&mut tracker, batch);
        }
        if !open {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct Fake {
        reading: Option<Reading>,
        /// The player doesn't answer.
        hung: bool,
        art: Option<Vec<u8>>,
        tint: Option<[u8; 3]>,
        art_reads: usize,
        attaches: usize,
        controls: Vec<Control>,
    }

    #[derive(Clone, Default)]
    struct FakeSource(Rc<RefCell<Fake>>);

    impl Source for FakeSource {
        fn session_changed(&mut self) {
            self.0.borrow_mut().attaches += 1;
        }
        fn read(&mut self) -> Read {
            let fake = self.0.borrow();
            if fake.hung {
                return Read::Unavailable;
            }
            match fake.reading.clone() {
                Some(reading) => Read::Session(reading),
                None => Read::NoSession,
            }
        }
        fn read_art(&mut self) -> Option<Cover> {
            let mut fake = self.0.borrow_mut();
            fake.art_reads += 1;
            let tint = fake.tint;
            fake.art.clone().map(|png| Cover { png, tint })
        }
        fn control(&mut self, control: Control) {
            self.0.borrow_mut().controls.push(control);
        }
    }

    fn reading(title: &str, playback: Playback) -> Reading {
        Reading {
            app: "Spotify".into(),
            title: title.into(),
            artist: "Band".into(),
            playback,
            timeline: Some(RawTimeline {
                start: Duration::ZERO,
                end: Duration::from_secs(200),
                position: Duration::from_secs(10),
                age: Duration::from_secs(3),
                rate: 1.0,
            }),
        }
    }

    fn setup() -> (Tracker<FakeSource>, FakeSource, Arc<ArtStore>) {
        let source = FakeSource::default();
        let art = Arc::new(ArtStore::default());
        (Tracker::new(source.clone(), art.clone()), source, art)
    }

    const BOTH: Batch = Batch {
        session: true,
        properties: true,
    };
    const PLAIN: Batch = Batch {
        session: false,
        properties: false,
    };

    fn state(outcome: Outcome) -> MediaState {
        match outcome {
            Outcome::Set(Some(state)) => state,
            other => panic!("expected a state, got {other:?}"),
        }
    }

    #[test]
    fn no_session_stopped_or_untitled_is_nothing() {
        let (mut tracker, source, _) = setup();
        assert_eq!(tracker.refresh(BOTH, Instant::now()), Outcome::Set(None));
        source.0.borrow_mut().reading = Some(reading("Song", Playback::Stopped));
        assert_eq!(tracker.refresh(PLAIN, Instant::now()), Outcome::Set(None));
        let mut untitled = reading("", Playback::Playing);
        untitled.artist.clear();
        source.0.borrow_mut().reading = Some(untitled);
        assert_eq!(tracker.refresh(PLAIN, Instant::now()), Outcome::Set(None));
    }

    #[test]
    fn a_transition_keeps_what_was_reported() {
        let (mut tracker, source, _) = setup();
        source.0.borrow_mut().reading = Some(reading("Song", Playback::Changing));
        assert_eq!(tracker.refresh(BOTH, Instant::now()), Outcome::Keep);
    }

    #[test]
    fn playing_and_paused_states_carry_the_timeline_brought_up_to_date() {
        let (mut tracker, source, _) = setup();
        let now = Instant::now();
        source.0.borrow_mut().reading = Some(reading("Song", Playback::Playing));
        let playing = state(tracker.refresh(BOTH, now));
        assert!(playing.playing);
        assert_eq!(
            (playing.title.as_str(), playing.app.as_str()),
            ("Song", "Spotify")
        );
        let t = playing.timeline.unwrap();
        assert_eq!(t.position, Duration::from_secs(13), "10 s plus 3 s of age");
        assert_eq!(t.length, Duration::from_secs(200));
        assert_eq!(t.read_at, now);
        assert_eq!(t.rate, 1.0);

        source.0.borrow_mut().reading = Some(reading("Song", Playback::Paused));
        let paused = state(tracker.refresh(PLAIN, now));
        assert!(!paused.playing);
        assert_eq!(paused.timeline.unwrap().position, Duration::from_secs(10));
    }

    #[test]
    fn timelines_without_a_length_or_with_odd_numbers_are_safe() {
        let now = Instant::now();
        let mut raw = reading("x", Playback::Playing).timeline.unwrap();
        raw.end = raw.start;
        assert!(timeline(&raw, true, now).is_none(), "a live stream");

        let mut raw = reading("x", Playback::Playing).timeline.unwrap();
        raw.position = Duration::from_secs(500);
        assert_eq!(
            timeline(&raw, true, now).unwrap().position,
            Duration::from_secs(200),
            "clamped"
        );

        let mut raw = reading("x", Playback::Playing).timeline.unwrap();
        raw.rate = f64::NAN;
        raw.age = Duration::from_secs(2);
        assert_eq!(
            timeline(&raw, true, now).unwrap().position,
            Duration::from_secs(12)
        );
        raw.age = Duration::from_secs(10 * 24 * 3600);
        assert_eq!(
            timeline(&raw, true, now).unwrap().position,
            Duration::from_secs(10),
            "a stale stamp is ignored"
        );
        raw.end = Duration::from_secs(5);
        raw.start = Duration::from_secs(9);
        assert!(timeline(&raw, true, now).is_none(), "end before start");
    }

    #[test]
    fn long_text_is_cut() {
        let (mut tracker, source, _) = setup();
        source.0.borrow_mut().reading = Some(reading(&"é".repeat(500), Playback::Playing));
        let state = state(tracker.refresh(BOTH, Instant::now()));
        assert_eq!(state.title.chars().count(), MAX_TEXT_CHARS);
    }

    #[test]
    fn the_cover_is_read_on_property_changes_and_kept_when_it_cannot_be_read() {
        let (mut tracker, source, art) = setup();
        {
            let mut fake = source.0.borrow_mut();
            fake.reading = Some(reading("One", Playback::Playing));
            fake.art = Some(vec![1, 2, 3]);
        }
        let first = state(tracker.refresh(BOTH, Instant::now()));
        let url = first.art.clone().unwrap();
        assert!(url.starts_with("http://qapsule-art.localhost/"));
        assert!(art.get(url.rsplit('/').next().unwrap()).is_some());
        assert_eq!(source.0.borrow().art_reads, 1);

        // Playback and timeline events don't read the cover again.
        let again = state(tracker.refresh(PLAIN, Instant::now()));
        assert_eq!(again.art.as_deref(), Some(url.as_str()));
        assert_eq!(source.0.borrow().art_reads, 1);

        // The same song, a property event, and the cover can't be read: it stays.
        source.0.borrow_mut().art = None;
        let props = Batch {
            session: false,
            properties: true,
        };
        let kept = state(tracker.refresh(props, Instant::now()));
        assert_eq!(kept.art.as_deref(), Some(url.as_str()));
        assert!(art.get(url.rsplit('/').next().unwrap()).is_some());

        // A new song without a readable cover has none, and the old image is gone.
        source.0.borrow_mut().reading = Some(reading("Two", Playback::Playing));
        let two = state(tracker.refresh(props, Instant::now()));
        assert_eq!(two.art, None);
        assert!(art.get(url.rsplit('/').next().unwrap()).is_none());

        // The cover arrives later with another property event.
        source.0.borrow_mut().art = Some(vec![7, 7]);
        let later = state(tracker.refresh(props, Instant::now()));
        assert!(later.art.is_some());
        assert_ne!(later.art.as_deref(), Some(url.as_str()));
    }

    #[test]
    fn the_tint_goes_with_the_cover() {
        let (mut tracker, source, _) = setup();
        {
            let mut fake = source.0.borrow_mut();
            fake.reading = Some(reading("One", Playback::Playing));
            fake.art = Some(vec![1]);
            fake.tint = Some([200, 40, 40]);
        }
        let first = state(tracker.refresh(BOTH, Instant::now()));
        assert_eq!(first.art_tint, Some([200, 40, 40]));
        // A property event that can't read the cover keeps both for the same song.
        source.0.borrow_mut().art = None;
        let props = Batch {
            session: false,
            properties: true,
        };
        assert_eq!(
            state(tracker.refresh(props, Instant::now())).art_tint,
            Some([200, 40, 40])
        );
        // A grey cover has a cover and no tint.
        {
            let mut fake = source.0.borrow_mut();
            fake.art = Some(vec![2]);
            fake.tint = None;
        }
        let grey = state(tracker.refresh(props, Instant::now()));
        assert!(grey.art.is_some());
        assert_eq!(grey.art_tint, None);
        // A new song without a cover has no tint.
        {
            let mut fake = source.0.borrow_mut();
            fake.reading = Some(reading("Two", Playback::Playing));
            fake.art = None;
            fake.tint = Some([1, 2, 3]);
        }
        let two = state(tracker.refresh(props, Instant::now()));
        assert_eq!((two.art, two.art_tint), (None, None));
    }

    #[test]
    fn a_new_session_is_followed_and_drops_the_old_cover() {
        let (mut tracker, source, art) = setup();
        {
            let mut fake = source.0.borrow_mut();
            fake.reading = Some(reading("One", Playback::Playing));
            fake.art = Some(vec![1]);
        }
        let first = state(tracker.refresh(BOTH, Instant::now()));
        assert_eq!(source.0.borrow().attaches, 1);
        source.0.borrow_mut().art = None;
        let second = state(tracker.refresh(BOTH, Instant::now()));
        assert_eq!(source.0.borrow().attaches, 2);
        assert_eq!(
            second.art, None,
            "the same song in another session has its own cover"
        );
        assert!(
            art.get(first.art.unwrap().rsplit('/').next().unwrap())
                .is_none()
        );

        source.0.borrow_mut().reading = None;
        assert_eq!(tracker.refresh(BOTH, Instant::now()), Outcome::Set(None));
    }

    #[test]
    fn a_hung_player_keeps_the_last_state_and_the_cover() {
        let (mut tracker, source, art) = setup();
        {
            let mut fake = source.0.borrow_mut();
            fake.reading = Some(reading("Song", Playback::Playing));
            fake.art = Some(vec![1]);
        }
        let first = state(tracker.refresh(BOTH, Instant::now()));
        source.0.borrow_mut().hung = true;
        assert_eq!(tracker.refresh(BOTH, Instant::now()), Outcome::Keep);
        assert!(
            art.get(first.art.unwrap().rsplit('/').next().unwrap())
                .is_some()
        );
        // It answers again: reported as usual.
        source.0.borrow_mut().hung = false;
        assert!(state(tracker.refresh(PLAIN, Instant::now())).playing);
    }

    #[test]
    fn a_flood_of_events_queues_one_wake_per_kind() {
        let (wakes, inbox) = Wakes::new();
        let flood = {
            let wakes = wakes.clone();
            std::thread::spawn(move || {
                for _ in 0..10_000 {
                    for event in EVENTS {
                        wakes.raise(event);
                    }
                }
            })
        };
        flood.join().unwrap();
        let queued: Vec<Msg> = inbox.rx.try_iter().collect();
        assert_eq!(queued.len(), 4, "one per kind, however many events came");

        // Once the worker has taken a wake, the next event of that kind queues again.
        inbox.taken(Event::Playback);
        wakes.raise(Event::Playback);
        wakes.raise(Event::Playback);
        assert_eq!(inbox.rx.try_iter().count(), 1);
    }

    #[test]
    fn buttons_cannot_fill_the_queue_forever_or_lose_the_next_event() {
        let (wakes, inbox) = Wakes::new();
        for _ in 0..1_000 {
            wakes.control(Control::Next);
        }
        assert_eq!(inbox.rx.try_iter().count(), QUEUE);
        for _ in 0..QUEUE {
            wakes.control(Control::Next);
        }
        // The queue is full: the event is dropped and not marked pending, so a later one gets through.
        wakes.raise(Event::Timeline);
        assert_eq!(inbox.rx.try_iter().count(), QUEUE);
        wakes.raise(Event::Timeline);
        assert_eq!(inbox.rx.try_iter().count(), 1);
    }

    #[test]
    fn a_burst_of_events_becomes_one_update_and_controls_run_at_once() {
        let source = FakeSource::default();
        source.0.borrow_mut().reading = Some(reading("Song", Playback::Playing));
        let (wakes, inbox) = Wakes::new();
        for event in [
            Event::Properties,
            Event::Playback,
            Event::Timeline,
            Event::Properties,
            Event::Timeline,
        ] {
            wakes.raise(event);
        }
        wakes.control(Control::Next);
        drop(wakes);
        let mut emitted = Vec::new();
        run(
            source.clone(),
            inbox,
            Arc::new(ArtStore::default()),
            Duration::from_millis(5),
            |state| emitted.push(state),
        );
        assert_eq!(emitted.len(), 2, "the start, then the whole burst");
        assert_eq!(source.0.borrow().controls, [Control::Next]);
        assert_eq!(source.0.borrow().attaches, 1, "only the start attached");
    }

    #[test]
    fn events_far_apart_are_separate_updates() {
        let source = FakeSource::default();
        source.0.borrow_mut().reading = Some(reading("Song", Playback::Playing));
        let (wakes, inbox) = Wakes::new();
        let sender = std::thread::spawn(move || {
            for _ in 0..3 {
                wakes.raise(Event::Playback);
                std::thread::sleep(Duration::from_millis(80));
            }
        });
        let mut emitted = 0;
        run(
            source,
            inbox,
            Arc::new(ArtStore::default()),
            Duration::from_millis(10),
            |_| emitted += 1,
        );
        sender.join().unwrap();
        assert_eq!(emitted, 4);
    }
}
