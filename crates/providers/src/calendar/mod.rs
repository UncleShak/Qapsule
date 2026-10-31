//! Calendar from a secret iCal link kept in Windows Credential Manager.
//!
//! Privacy: logs hold only a fixed reason or an HTTP status, never the link,
//! a URL, or an event's title or location. The task sleeps until one deadline
//! at a time (never polls) and, with pops off and the main screen closed,
//! sleeps until something opens it.

mod http;
mod parse;
mod view;

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::time::Duration;

use chrono::{DateTime, Utc};
use qapsule_core::view::EventsView;
use qapsule_protocol::limits::{MAX_EXPIRES_IN_SECS, MAX_SUBTITLE_CHARS};
use qapsule_protocol::{Activity, State};
use qapsule_server::Engine;
use rrule::Tz;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub use parse::{MAX_OCCURRENCES, Occurrence, ParseError, Span, parse_calendar};

/// Data older than this is fetched again when the main screen opens; with pops
/// on, it is also fetched this often in the background.
const REFRESH: Duration = Duration::from_secs(15 * 60);
/// A pop later than this after its time is skipped (the app was asleep or just started).
const POP_GRACE: chrono::Duration = chrono::Duration::seconds(60);
/// Where the pops go.
const SOURCE: &str = "calendar:events";

/// What the app's settings say about the calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalendarConfig {
    /// How long before an event it pops.
    pub minutes_before: u32,
    /// Whether events pop at all (and so whether the calendar refreshes in the background).
    pub pops: bool,
}

/// Why a fetch failed. Holds no URL, so it is safe to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchError {
    reason: &'static str,
    status: Option<u16>,
}

impl FetchError {
    pub fn new(reason: &'static str) -> Self {
        Self {
            reason,
            status: None,
        }
    }

    pub fn status(code: u16) -> Self {
        Self {
            reason: "http error",
            status: Some(code),
        }
    }
}

/// Where the calendar text comes from; swapped for a fake in tests.
pub trait Fetcher: Send + Sync + 'static {
    fn fetch(&self, link: &str) -> impl Future<Output = Result<String, FetchError>> + Send;
}

/// Where the link is kept; swapped for a fake in tests.
pub trait LinkSource: Send + Sync + 'static {
    fn link(&self) -> io::Result<Option<String>>;
}

/// The link in Windows Credential Manager, read afresh each time.
struct Credentials;

impl LinkSource for Credentials {
    fn link(&self) -> io::Result<Option<String>> {
        qapsule_secrets::get(qapsule_secrets::CALENDAR_LINK)
    }
}

type Clock = Box<dyn Fn() -> DateTime<Utc> + Send>;

/// A parse in flight: `None` if it stopped before finishing.
type Parsing = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<Result<Vec<Occurrence>, ParseError>>> + Send>,
>;

/// How the parse runs: `None` if it stopped before finishing.
type Runner = fn(String, Tz, DateTime<Utc>) -> Parsing;

/// Parses on a blocking thread, so a huge feed can't stall the runtime.
fn run_blocking(body: String, tz: Tz, now: DateTime<Utc>) -> Parsing {
    Box::pin(async move {
        tokio::task::spawn_blocking(move || parse_calendar(&body, tz, now))
            .await
            .ok()
    })
}

/// Where results go: the engine in the app, channels in tests.
struct Out {
    view: Box<dyn Fn(EventsView) + Send>,
    pop: Box<dyn Fn(Activity) + Send>,
}

/// The calendar provider. Dropping it stops the task.
pub struct Calendar {
    tx: mpsc::UnboundedSender<bool>,
}

impl Calendar {
    /// Starts the provider on the current tokio runtime.
    pub fn start(engine: Engine, config: CalendarConfig) -> Calendar {
        let popper = engine.clone();
        let out = Out {
            view: Box::new(move |view| engine.set_events(view)),
            pop: Box::new(move |activity| popper.provider_upsert(activity)),
        };
        spawn(
            http::Http::new(),
            Credentials,
            config,
            out,
            Box::new(Utc::now),
            Tz::LOCAL,
            run_blocking,
        )
        .0
    }

    /// The main screen opened (`true`) or closed (`false`).
    pub fn main_screen(&self, visible: bool) {
        let _ = self.tx.send(visible);
    }
}

fn spawn<F: Fetcher, L: LinkSource>(
    fetcher: F,
    links: L,
    config: CalendarConfig,
    out: Out,
    clock: Clock,
    tz: Tz,
    runner: Runner,
) -> (Calendar, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut state = Provider::new(fetcher, links, config, out, clock, tz, runner);
    let task = tokio::spawn(async move {
        if config.pops {
            state.tick().await;
        }
        loop {
            let deadline = state.deadline();
            tokio::select! {
                message = rx.recv() => match message {
                    Some(visible) => state.screen(visible).await,
                    None => break,
                },
                () = sleep_until(deadline) => state.tick().await,
            }
        }
    });
    (Calendar { tx }, task)
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// FNV-1a over the event's UID and start: the same occurrence always gets the same id.
fn pop_id(uid: &str, start: DateTime<Utc>) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let bytes = uid
        .bytes()
        .chain(std::iter::once(0xff))
        .chain(start.timestamp().to_le_bytes());
    for byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("ev-{hash:016x}")
}

struct Provider<F, L> {
    fetcher: F,
    links: L,
    config: CalendarConfig,
    out: Out,
    clock: Clock,
    tz: Tz,
    runner: Runner,
    visible: bool,
    /// The occurrences from the last good fetch.
    data: Option<Vec<Occurrence>>,
    no_link: bool,
    /// The last fetch failed (so `data`, if any, is stale).
    failed: bool,
    last_success: Option<Instant>,
    last_attempt: Option<Instant>,
    /// Pops already sent, by id, with the event's start (to forget them later).
    popped: HashMap<String, DateTime<Utc>>,
}

impl<F: Fetcher, L: LinkSource> Provider<F, L> {
    fn new(
        fetcher: F,
        links: L,
        config: CalendarConfig,
        out: Out,
        clock: Clock,
        tz: Tz,
        runner: Runner,
    ) -> Self {
        Self {
            fetcher,
            links,
            config,
            out,
            clock,
            tz,
            runner,
            visible: false,
            data: None,
            no_link: false,
            failed: false,
            last_success: None,
            last_attempt: None,
            popped: HashMap::new(),
        }
    }

    /// The main screen opened or closed.
    async fn screen(&mut self, visible: bool) {
        self.visible = visible;
        if !visible {
            return;
        }
        let now_i = Instant::now();
        let fresh = self
            .last_success
            .is_some_and(|at| now_i.duration_since(at) < REFRESH);
        if !fresh {
            // Don't show the old view while the refresh runs: rebuild it from what we have.
            if self.data.is_some() {
                self.send_view((self.clock)());
            }
            self.refresh(now_i).await;
        }
        let now = (self.clock)();
        self.fire_pops(now);
        self.send_view(now);
    }

    /// A deadline passed: refresh if due, pop what is due, update the screen if open.
    async fn tick(&mut self) {
        let now_i = Instant::now();
        let due = self.config.pops
            && self
                .last_attempt
                .is_none_or(|at| now_i.duration_since(at) >= REFRESH);
        if due {
            self.refresh(now_i).await;
        }
        let now = (self.clock)();
        self.fire_pops(now);
        if self.visible {
            self.send_view(now);
        }
    }

    async fn refresh(&mut self, now_i: Instant) {
        self.last_attempt = Some(now_i);
        let link = match self.links.link() {
            Ok(link) => link,
            Err(_) => {
                eprintln!("qapsule: calendar refresh failed (credential read)");
                return self.fail();
            }
        };
        let Some(link) = link else {
            self.data = None;
            self.no_link = true;
            self.failed = false;
            self.last_success = None;
            return;
        };
        self.no_link = false;
        if !link.starts_with("https://") {
            eprintln!("qapsule: calendar refresh failed (bad link)");
            return self.fail();
        }
        let body = match self.fetcher.fetch(&link).await {
            Ok(body) => body,
            Err(error) => {
                match error.status {
                    Some(code) => eprintln!("qapsule: calendar refresh failed (status {code})"),
                    None => eprintln!("qapsule: calendar refresh failed ({})", error.reason),
                }
                return self.fail();
            }
        };
        let (tz, now_utc) = (self.tz, (self.clock)());
        let parsed = (self.runner)(body, tz, now_utc).await;
        match parsed {
            None => {
                eprintln!("qapsule: calendar refresh failed (parser stopped)");
                self.fail();
            }
            Some(Ok(occurrences)) => {
                self.data = Some(occurrences);
                self.failed = false;
                self.last_success = Some(now_i);
            }
            Some(Err(error)) => {
                eprintln!("qapsule: calendar refresh failed ({})", error.0);
                self.fail();
            }
        }
    }

    fn fail(&mut self) {
        self.failed = true;
    }

    fn events_view(&self, now: DateTime<Utc>) -> EventsView {
        if self.no_link {
            return EventsView::NoLink;
        }
        match &self.data {
            Some(data) => EventsView::Ready(view::build(data, now, self.tz, self.failed)),
            None if self.failed => EventsView::Unavailable,
            None => EventsView::Loading,
        }
    }

    fn send_view(&self, now: DateTime<Utc>) {
        (self.out.view)(self.events_view(now));
    }

    fn pop_lead(&self) -> chrono::Duration {
        chrono::Duration::minutes(i64::from(self.config.minutes_before))
    }

    /// Sends the pop for every timed event whose pop time has just come.
    fn fire_pops(&mut self, now: DateTime<Utc>) {
        if !self.config.pops {
            return;
        }
        self.popped
            .retain(|_, start| *start + chrono::Duration::hours(1) > now);
        let lead = self.pop_lead();
        let mut due = Vec::new();
        for occurrence in self.data.iter().flatten() {
            let Span::Timed { start, .. } = &occurrence.span else {
                continue;
            };
            let pop_time = *start - lead;
            let id = pop_id(&occurrence.uid, *start);
            if *start > now
                && pop_time <= now
                && now - pop_time <= POP_GRACE
                && !self.popped.contains_key(&id)
            {
                due.push((id, *start, occurrence.clone()));
            }
        }
        for (id, start, occurrence) in due {
            self.popped.insert(id.clone(), start);
            (self.out.pop)(self.pop_activity(id, start, &occurrence, now));
        }
    }

    fn pop_activity(
        &self,
        id: String,
        start: DateTime<Utc>,
        occurrence: &Occurrence,
        now: DateTime<Utc>,
    ) -> Activity {
        let mut subtitle = format!(
            "Starts in {} min · {}",
            self.config.minutes_before,
            start.with_timezone(&self.tz).format("%H:%M")
        );
        if let Some(location) = view::location_of(occurrence) {
            subtitle.push_str(" · ");
            subtitle.push_str(&location);
        }
        Activity {
            id,
            source: SOURCE.into(),
            title: view::title_of(occurrence),
            subtitle: Some(view::clean(&subtitle, MAX_SUBTITLE_CHARS)),
            state: State::Info,
            progress: None,
            icon: Some("calendar".into()),
            expires_in: Some(
                u64::try_from((start - now).num_seconds())
                    .unwrap_or(0)
                    .clamp(1, MAX_EXPIRES_IN_SECS),
            ),
            actions: vec![],
        }
    }

    /// The earliest moment something needs doing, `None` if nothing does.
    fn deadline(&self) -> Option<Instant> {
        let now = (self.clock)();
        let now_i = Instant::now();
        let at = |t: DateTime<Utc>| now_i + (t - now).to_std().unwrap_or_default();
        let mut soonest: Vec<Instant> = Vec::new();
        if self.config.pops {
            soonest.push(self.last_attempt.map_or(now_i, |t| t + REFRESH));
        }
        if self.visible {
            soonest.push(at(view::next_midnight(now, self.tz)));
        }
        let lead = self.pop_lead();
        for occurrence in self.data.iter().flatten() {
            let Span::Timed { start, end } = &occurrence.span else {
                continue;
            };
            if self.config.pops {
                let pop_time = *start - lead;
                if pop_time > now && !self.popped.contains_key(&pop_id(&occurrence.uid, *start)) {
                    soonest.push(at(pop_time));
                }
            }
            if self.visible {
                for moment in [*start, *end] {
                    if moment > now {
                        soonest.push(at(moment));
                    }
                }
            }
        }
        soonest.into_iter().min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;
    use qapsule_core::view::EventsReady;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::mpsc::UnboundedReceiver;

    const JOBURG: Tz = Tz::Africa__Johannesburg;
    const LINK: &str = "https://calendar.example/secret/basic.ics";

    #[derive(Default)]
    struct Fake {
        body: Mutex<String>,
        fail: Mutex<Option<FetchError>>,
        fetches: AtomicUsize,
        links_seen: Mutex<Vec<String>>,
    }

    impl Fetcher for Arc<Fake> {
        fn fetch(&self, link: &str) -> impl Future<Output = Result<String, FetchError>> + Send {
            let this = self.clone();
            let link = link.to_string();
            async move {
                this.fetches.fetch_add(1, Ordering::SeqCst);
                this.links_seen.lock().unwrap().push(link);
                if let Some(error) = this.fail.lock().unwrap().clone() {
                    return Err(error);
                }
                Ok(this.body.lock().unwrap().clone())
            }
        }
    }

    struct Links(Arc<Mutex<Option<String>>>);

    impl LinkSource for Links {
        fn link(&self) -> io::Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }
    }

    struct Harness {
        calendar: Calendar,
        views: UnboundedReceiver<EventsView>,
        pops: UnboundedReceiver<Activity>,
        fake: Arc<Fake>,
        link: Arc<Mutex<Option<String>>>,
    }

    fn utc(s: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
            .unwrap()
            .and_utc()
    }

    /// Events as (uid, start, end) in iCal UTC form, e.g. `20250310T083000Z`.
    fn ics(events: &[(&str, &str, &str)]) -> String {
        let mut text = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n");
        for (uid, start, end) in events {
            text.push_str(&format!(
                "BEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART:{start}\r\nDTEND:{end}\r\nSUMMARY:{uid}\r\nLOCATION:Room 1\r\nEND:VEVENT\r\n"
            ));
        }
        text.push_str("END:VCALENDAR\r\n");
        text
    }

    /// The clock starts at 2025-03-10 08:00 UTC (10:00 in Johannesburg) and follows tokio's.
    fn harness(body: String, link: Option<&str>, pops: bool) -> Harness {
        let fake = Arc::new(Fake::default());
        *fake.body.lock().unwrap() = body;
        let link = Arc::new(Mutex::new(link.map(String::from)));
        let (view_tx, views) = mpsc::unbounded_channel();
        let (pop_tx, pops_rx) = mpsc::unbounded_channel();
        let out = Out {
            view: Box::new(move |v| view_tx.send(v).unwrap()),
            pop: Box::new(move |a| pop_tx.send(a).unwrap()),
        };
        let (base, started) = (utc("2025-03-10 08:00"), Instant::now());
        let clock: Clock =
            Box::new(move || base + chrono::Duration::from_std(Instant::now() - started).unwrap());
        let (calendar, _task) = spawn(
            fake.clone(),
            Links(link.clone()),
            CalendarConfig {
                minutes_before: 10,
                pops,
            },
            out,
            clock,
            JOBURG,
            run_inline,
        );
        Harness {
            calendar,
            views,
            pops: pops_rx,
            fake,
            link,
        }
    }

    /// Parses on the task itself, so paused-time tests stay deterministic.
    fn run_inline(body: String, tz: Tz, now: DateTime<Utc>) -> Parsing {
        Box::pin(async move { Some(parse_calendar(&body, tz, now)) })
    }

    #[tokio::test]
    async fn the_blocking_runner_parses_to_completion() {
        let text = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T100000Z\r\nSUMMARY:One\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";
        let parsed = run_blocking(text.into(), JOBURG, utc("2025-03-10 08:00")).await;
        assert_eq!(parsed.unwrap().unwrap().len(), 1);
        let junk = run_blocking("<html>".into(), JOBURG, utc("2025-03-10 08:00")).await;
        assert!(junk.unwrap().is_err());
    }

    async fn settle() {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }

    async fn advance(secs: u64) {
        tokio::time::advance(Duration::from_secs(secs)).await;
        settle().await;
    }

    fn last_view(rx: &mut UnboundedReceiver<EventsView>) -> Option<EventsView> {
        std::iter::from_fn(|| rx.try_recv().ok()).last()
    }

    fn ready(view: Option<EventsView>) -> EventsReady {
        match view {
            Some(EventsView::Ready(r)) => r,
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    impl Harness {
        fn fetches(&self) -> usize {
            self.fake.fetches.load(Ordering::SeqCst)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn no_link_says_so_and_fetches_nothing() {
        let mut h = harness(ics(&[]), None, false);
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(last_view(&mut h.views), Some(EventsView::NoLink));
        assert_eq!(h.fetches(), 0);

        // Setting the link applies at the next refresh.
        *h.link.lock().unwrap() = Some(LINK.into());
        h.calendar.main_screen(true);
        settle().await;
        assert!(matches!(
            last_view(&mut h.views),
            Some(EventsView::Ready(_))
        ));
        assert_eq!(h.fetches(), 1);
        assert_eq!(h.fake.links_seen.lock().unwrap()[0], LINK);
    }

    #[tokio::test(start_paused = true)]
    async fn opening_refreshes_after_fifteen_minutes_not_before() {
        let mut h = harness(ics(&[]), Some(LINK), false);
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(h.fetches(), 1);
        assert!(matches!(
            last_view(&mut h.views),
            Some(EventsView::Ready(_))
        ));

        advance(14 * 60).await;
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(h.fetches(), 1);
        // Still answers from what it has.
        assert!(matches!(
            last_view(&mut h.views),
            Some(EventsView::Ready(_))
        ));

        advance(60).await;
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(h.fetches(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn opening_first_shows_the_cached_events_as_of_now() {
        let mut h = harness(
            ics(&[("Review", "20250310T081000Z", "20250310T084000Z")]),
            Some(LINK),
            false,
        );
        h.calendar.main_screen(true);
        settle().await;
        let first = ready(last_view(&mut h.views));
        assert!(first.now.is_none() && first.next.len() == 1);
        h.calendar.main_screen(false);
        settle().await;

        // Twenty minutes on, the event is under way; the refresh will fail.
        *h.fake.fail.lock().unwrap() = Some(FetchError::status(503));
        advance(20 * 60).await;
        h.calendar.main_screen(true);
        settle().await;
        let views: Vec<_> = std::iter::from_fn(|| h.views.try_recv().ok()).collect();
        assert_eq!(views.len(), 2, "the cached view, then the refreshed one");
        let cached = ready(views.first().cloned());
        assert_eq!(
            cached.now.as_ref().map(|e| e.title.as_str()),
            Some("Review")
        );
        assert!(!cached.stale);
        assert!(ready(views.last().cloned()).stale);
    }

    #[tokio::test(start_paused = true)]
    async fn without_pops_nothing_runs_in_the_background() {
        let mut h = harness(
            ics(&[("a", "20250310T083000Z", "20250310T090000Z")]),
            Some(LINK),
            false,
        );
        settle().await;
        advance(60 * 60).await;
        assert_eq!(h.fetches(), 0);
        assert!(h.views.try_recv().is_err() && h.pops.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn with_pops_it_refreshes_in_the_background_every_fifteen_minutes() {
        let mut h = harness(ics(&[]), Some(LINK), true);
        settle().await;
        assert_eq!(h.fetches(), 1);
        advance(14 * 60 + 59).await;
        assert_eq!(h.fetches(), 1);
        advance(1).await;
        assert_eq!(h.fetches(), 2);
        advance(15 * 60).await;
        assert_eq!(h.fetches(), 3);
        // Closed: the screen is not touched.
        assert!(h.views.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn the_pop_fires_once_at_start_minus_minutes_before() {
        let mut h = harness(
            ics(&[("Standup", "20250310T083000Z", "20250310T090000Z")]),
            Some(LINK),
            true,
        );
        settle().await;
        advance(19 * 60 + 59).await;
        assert!(h.pops.try_recv().is_err());
        advance(1).await;
        let pop = h.pops.try_recv().unwrap();
        assert_eq!(pop.source, "calendar:events");
        assert_eq!(pop.title, "Standup");
        assert_eq!(
            pop.subtitle.as_deref(),
            Some("Starts in 10 min · 10:30 · Room 1")
        );
        assert_eq!(pop.icon.as_deref(), Some("calendar"));
        assert_eq!(pop.state, State::Info);
        assert_eq!(pop.expires_in, Some(600));
        assert!(pop.actions.is_empty());
        assert!(pop.id.starts_with("ev-") && pop.id.len() == 19);
        assert!(pop.validate().is_ok());

        // Later refreshes do not pop it again.
        advance(20 * 60).await;
        assert!(h.pops.try_recv().is_err());
        assert!(h.fetches() >= 3);
    }

    #[tokio::test(start_paused = true)]
    async fn an_event_past_its_pop_time_by_over_a_minute_never_pops() {
        // Starting in 5 minutes with a 10-minute lead: its pop time was 5 minutes ago.
        // The other one's pop time was 30 seconds ago, so it still pops.
        let body = ics(&[
            ("late", "20250310T080500Z", "20250310T090000Z"),
            ("just", "20250310T080930Z", "20250310T090000Z"),
            ("started", "20250310T075000Z", "20250310T090000Z"),
        ]);
        let mut h = harness(body, Some(LINK), true);
        settle().await;
        let pop = h.pops.try_recv().unwrap();
        assert_eq!(pop.title, "just");
        assert_eq!(pop.expires_in, Some(570));
        advance(60 * 60).await;
        assert!(h.pops.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn failure_keeps_the_last_data_as_stale_or_is_unavailable() {
        let mut h = harness(
            ics(&[("a", "20250310T100000Z", "20250310T110000Z")]),
            Some(LINK),
            false,
        );
        h.calendar.main_screen(true);
        settle().await;
        assert!(!ready(last_view(&mut h.views)).stale);

        *h.fake.fail.lock().unwrap() = Some(FetchError::status(503));
        advance(15 * 60).await;
        h.calendar.main_screen(true);
        settle().await;
        let card = ready(last_view(&mut h.views));
        assert!(card.stale);
        assert_eq!(card.next.len(), 1);

        // The failure did not reset the clock: the next open tries again.
        *h.fake.fail.lock().unwrap() = None;
        h.calendar.main_screen(true);
        settle().await;
        assert!(!ready(last_view(&mut h.views)).stale);

        let mut h = harness(ics(&[]), Some(LINK), false);
        *h.fake.fail.lock().unwrap() = Some(FetchError::new("timeout"));
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(last_view(&mut h.views), Some(EventsView::Unavailable));
    }

    #[tokio::test(start_paused = true)]
    async fn a_page_that_is_not_a_calendar_is_unavailable() {
        let mut h = harness("<html></html>".into(), Some(LINK), false);
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(last_view(&mut h.views), Some(EventsView::Unavailable));
    }

    #[tokio::test(start_paused = true)]
    async fn an_insecure_link_is_never_fetched() {
        let mut h = harness(ics(&[]), Some("http://calendar.example/a.ics"), false);
        h.calendar.main_screen(true);
        settle().await;
        assert_eq!(last_view(&mut h.views), Some(EventsView::Unavailable));
        assert_eq!(h.fetches(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn the_open_screen_follows_event_starts_and_ends() {
        let mut h = harness(
            ics(&[("Standup", "20250310T083000Z", "20250310T090000Z")]),
            Some(LINK),
            false,
        );
        h.calendar.main_screen(true);
        settle().await;
        let card = ready(last_view(&mut h.views));
        assert!(card.now.is_none());
        assert_eq!(card.next[0].when, "10:30");

        advance(30 * 60).await;
        let card = ready(last_view(&mut h.views));
        let now = card.now.unwrap();
        assert_eq!(
            (now.title.as_str(), now.until.as_str()),
            ("Standup", "11:00")
        );
        assert!(card.next.is_empty());

        advance(30 * 60).await;
        let card = ready(last_view(&mut h.views));
        assert!(card.now.is_none());

        // Closing it stops the updates.
        h.calendar.main_screen(false);
        settle().await;
        advance(24 * 60 * 60).await;
        assert!(h.views.try_recv().is_err());
    }

    #[test]
    fn pop_ids_are_stable_and_differ_by_start() {
        let a = pop_id("uid", utc("2025-03-10 08:30"));
        assert_eq!(a, pop_id("uid", utc("2025-03-10 08:30")));
        assert_ne!(a, pop_id("uid", utc("2025-03-17 08:30")));
        assert_ne!(a, pop_id("other", utc("2025-03-10 08:30")));
        assert_eq!(a.len(), 19);
    }
}
