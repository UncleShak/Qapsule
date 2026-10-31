//! Weather from Open-Meteo, fetched when the main screen opens.
//!
//! Privacy: the only data sent is the city the user typed in settings. Logs
//! never contain the city or a URL, only a short reason and an HTTP status.

mod http;
mod parse;
mod wmo;

use std::future::Future;
use std::time::Duration;

use qapsule_core::view::{WeatherReady, WeatherView};
use qapsule_server::Engine;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

pub use parse::{ParseError, Place, parse_forecast, parse_geocode};

/// Data older than this is fetched again when the main screen opens.
const MAX_AGE: Duration = Duration::from_secs(30 * 60);

/// Why a fetch failed. Holds no URL and no city, so it is safe to log.
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

/// Where the data comes from; swapped for a fake in tests.
pub trait Fetcher: Send + Sync + 'static {
    /// The place for a city name, `None` if nothing matches.
    fn geocode(&self, city: &str)
    -> impl Future<Output = Result<Option<Place>, FetchError>> + Send;
    fn forecast(
        &self,
        place: &Place,
    ) -> impl Future<Output = Result<WeatherReady, FetchError>> + Send;
}

/// Where the views go; the engine in the app, a channel in tests.
type Sink = Box<dyn Fn(WeatherView) + Send>;

/// The weather provider. Dropping it stops the task.
pub struct Weather {
    tx: mpsc::UnboundedSender<Msg>,
}

enum Msg {
    /// The main screen opened (`true`) or closed (`false`).
    Screen(bool),
    /// The configured city changed.
    City(Option<String>),
}

impl Weather {
    /// Starts the provider on the current tokio runtime. Nothing is fetched
    /// until the main screen opens.
    pub fn start(engine: Engine, city: Option<String>) -> Weather {
        let sink: Sink = Box::new(move |view| engine.set_weather(view));
        spawn(http::Http::new(), city, sink).0
    }

    /// The main screen opened (`true`) or closed (`false`).
    pub fn main_screen(&self, visible: bool) {
        let _ = self.tx.send(Msg::Screen(visible));
    }

    /// The configured city changed (a reloaded config). A different city drops the
    /// cached place and data, so the next open fetches fresh.
    pub fn set_city(&self, city: Option<String>) {
        let _ = self.tx.send(Msg::City(city));
    }
}

fn spawn<F: Fetcher>(fetcher: F, city: Option<String>, sink: Sink) -> (Weather, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut state = State::new(fetcher, city, sink);
    let task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            match message {
                Msg::City(city) => state.set_city(city),
                Msg::Screen(false) => {}
                Msg::Screen(true) => {
                    state.opened(Instant::now()).await;
                    // Opens that arrived during the fetch are not new requests;
                    // a city change is.
                    while let Ok(message) = rx.try_recv() {
                        if let Msg::City(city) = message {
                            state.set_city(city);
                        }
                    }
                }
            }
        }
    });
    (Weather { tx }, task)
}

struct State<F> {
    fetcher: F,
    city: Option<String>,
    sink: Sink,
    place: Option<Place>,
    /// The last good data and when it was fetched.
    last: Option<(WeatherReady, Instant)>,
}

impl<F: Fetcher> State<F> {
    fn new(fetcher: F, city: Option<String>, sink: Sink) -> Self {
        let city = city.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        Self {
            fetcher,
            city,
            sink,
            place: None,
            last: None,
        }
    }

    /// A different city drops the cached place and data.
    fn set_city(&mut self, city: Option<String>) {
        let city = city.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
        if city != self.city {
            self.city = city;
            self.place = None;
            self.last = None;
        }
    }

    /// The main screen opened at `now`.
    async fn opened(&mut self, now: Instant) {
        let Some(city) = self.city.clone() else {
            (self.sink)(WeatherView::NoCity);
            return;
        };
        if let Some((_, fetched)) = &self.last
            && now.duration_since(*fetched) < MAX_AGE
        {
            return;
        }
        match self.fetch(&city).await {
            Ok(Some(ready)) => {
                self.last = Some((ready.clone(), now));
                (self.sink)(WeatherView::Ready(ready));
            }
            Ok(None) => {
                eprintln!("qapsule: weather city not found");
                (self.sink)(WeatherView::Unavailable);
            }
            Err(error) => {
                match error.status {
                    Some(code) => eprintln!("qapsule: weather refresh failed (status {code})"),
                    None => eprintln!("qapsule: weather refresh failed ({})", error.reason),
                }
                let view = match &self.last {
                    Some((last, _)) => WeatherView::Ready(WeatherReady {
                        stale: true,
                        ..last.clone()
                    }),
                    None => WeatherView::Unavailable,
                };
                (self.sink)(view);
            }
        }
    }

    /// Geocodes once per city, then fetches the forecast. `None`: unknown city.
    async fn fetch(&mut self, city: &str) -> Result<Option<WeatherReady>, FetchError> {
        let place = match &self.place {
            Some(place) => place.clone(),
            None => match self.fetcher.geocode(city).await? {
                Some(place) => {
                    self.place = Some(place.clone());
                    place
                }
                None => return Ok(None),
            },
        };
        self.fetcher.forecast(&place).await.map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tokio::sync::{Notify, mpsc::UnboundedReceiver};

    #[derive(Default)]
    struct Fake {
        geocodes: AtomicUsize,
        forecasts: AtomicUsize,
        fail: Mutex<Option<FetchError>>,
        no_match: Mutex<bool>,
        gate: Option<Arc<Notify>>,
    }

    impl Fetcher for Arc<Fake> {
        fn geocode(
            &self,
            _city: &str,
        ) -> impl Future<Output = Result<Option<Place>, FetchError>> + Send {
            let this = self.clone();
            async move {
                this.geocodes.fetch_add(1, Ordering::SeqCst);
                if *this.no_match.lock().unwrap() {
                    return Ok(None);
                }
                if let Some(e) = this.fail.lock().unwrap().clone() {
                    return Err(e);
                }
                Ok(Some(Place {
                    name: "Pretoria".into(),
                    latitude: 1.0,
                    longitude: 2.0,
                }))
            }
        }

        fn forecast(
            &self,
            place: &Place,
        ) -> impl Future<Output = Result<WeatherReady, FetchError>> + Send {
            let this = self.clone();
            let name = place.name.clone();
            async move {
                let n = this.forecasts.fetch_add(1, Ordering::SeqCst) + 1;
                if let Some(gate) = &this.gate {
                    gate.notified().await;
                }
                if let Some(e) = this.fail.lock().unwrap().clone() {
                    return Err(e);
                }
                Ok(WeatherReady {
                    city: name,
                    temp_c: n as i32,
                    condition: "Clear".into(),
                    glyph: "clear".into(),
                    high_c: 30,
                    low_c: 10,
                    hourly: vec![],
                    stale: false,
                })
            }
        }
    }

    fn views() -> (Sink, UnboundedReceiver<WeatherView>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Box::new(move |v| tx.send(v).unwrap()), rx)
    }

    fn state(
        fake: &Arc<Fake>,
        city: Option<&str>,
    ) -> (State<Arc<Fake>>, UnboundedReceiver<WeatherView>) {
        let (sink, rx) = views();
        (State::new(fake.clone(), city.map(String::from), sink), rx)
    }

    fn temp(view: WeatherView) -> (i32, bool) {
        match view {
            WeatherView::Ready(r) => (r.temp_c, r.stale),
            other => panic!("expected Ready, got {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn no_city_says_so_and_fetches_nothing() {
        let fake = Arc::new(Fake::default());
        for city in [None, Some("   ")] {
            let (mut s, mut rx) = state(&fake, city);
            s.opened(Instant::now()).await;
            assert_eq!(rx.try_recv().unwrap(), WeatherView::NoCity);
        }
        assert_eq!(fake.geocodes.load(Ordering::SeqCst), 0);
        assert_eq!(fake.forecasts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn refreshes_after_thirty_minutes_not_before() {
        let fake = Arc::new(Fake::default());
        let (mut s, mut rx) = state(&fake, Some("Pretoria"));

        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (1, false));

        tokio::time::advance(Duration::from_secs(29 * 60)).await;
        s.opened(Instant::now()).await;
        assert!(rx.try_recv().is_err());
        assert_eq!(fake.forecasts.load(Ordering::SeqCst), 1);

        tokio::time::advance(Duration::from_secs(60)).await;
        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (2, false));
        // The place is looked up only once.
        assert_eq!(fake.geocodes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn failure_keeps_the_last_data_as_stale() {
        let fake = Arc::new(Fake::default());
        let (mut s, mut rx) = state(&fake, Some("Pretoria"));
        s.opened(Instant::now()).await;
        rx.try_recv().unwrap();

        *fake.fail.lock().unwrap() = Some(FetchError::status(503));
        tokio::time::advance(MAX_AGE).await;
        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (1, true));

        // The failure did not reset the clock: the next open tries again.
        *fake.fail.lock().unwrap() = None;
        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (3, false));
    }

    #[tokio::test(start_paused = true)]
    async fn failure_without_data_is_unavailable_and_retried_on_next_open() {
        let fake = Arc::new(Fake::default());
        *fake.fail.lock().unwrap() = Some(FetchError::new("timeout"));
        let (mut s, mut rx) = state(&fake, Some("Pretoria"));
        s.opened(Instant::now()).await;
        assert_eq!(rx.try_recv().unwrap(), WeatherView::Unavailable);

        *fake.fail.lock().unwrap() = None;
        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (1, false));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_city_is_unavailable() {
        let fake = Arc::new(Fake::default());
        *fake.no_match.lock().unwrap() = true;
        let (mut s, mut rx) = state(&fake, Some("Zzqxvwkj"));
        s.opened(Instant::now()).await;
        assert_eq!(rx.try_recv().unwrap(), WeatherView::Unavailable);
        assert_eq!(fake.forecasts.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn opens_during_a_fetch_do_not_fetch_again() {
        let gate = Arc::new(Notify::new());
        let fake = Arc::new(Fake {
            gate: Some(gate.clone()),
            ..Fake::default()
        });
        let (sink, mut rx) = views();
        let (weather, task) = spawn(fake.clone(), Some("Pretoria".into()), sink);

        weather.main_screen(true);
        weather.main_screen(true);
        weather.main_screen(false);
        weather.main_screen(true);
        gate.notify_one();

        assert_eq!(temp(rx.recv().await.unwrap()), (1, false));
        drop(weather);
        task.await.unwrap();
        assert_eq!(fake.forecasts.load(Ordering::SeqCst), 1);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_city_is_fetched_fresh_and_the_same_city_is_not() {
        let fake = Arc::new(Fake::default());
        let (mut s, mut rx) = state(&fake, Some("Pretoria"));
        s.opened(Instant::now()).await;
        rx.try_recv().unwrap();

        s.set_city(Some(" Pretoria ".into()));
        s.opened(Instant::now()).await;
        assert!(rx.try_recv().is_err(), "same city keeps the cache");

        s.set_city(Some("Durban".into()));
        s.opened(Instant::now()).await;
        assert_eq!(temp(rx.try_recv().unwrap()), (2, false));
        assert_eq!(fake.geocodes.load(Ordering::SeqCst), 2);

        s.set_city(None);
        s.opened(Instant::now()).await;
        assert_eq!(rx.try_recv().unwrap(), WeatherView::NoCity);
    }

    #[tokio::test(start_paused = true)]
    async fn a_city_sent_to_the_task_applies_to_the_next_open() {
        let fake = Arc::new(Fake::default());
        let (sink, mut rx) = views();
        let (weather, task) = spawn(fake.clone(), Some("Pretoria".into()), sink);
        weather.main_screen(true);
        assert_eq!(temp(rx.recv().await.unwrap()), (1, false));
        weather.set_city(Some("Durban".into()));
        weather.main_screen(true);
        assert_eq!(temp(rx.recv().await.unwrap()), (2, false));
        drop(weather);
        task.await.unwrap();
        assert_eq!(fake.geocodes.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn closing_the_screen_does_nothing() {
        let fake = Arc::new(Fake::default());
        let (sink, mut rx) = views();
        let (weather, task) = spawn(fake.clone(), Some("Pretoria".into()), sink);
        weather.main_screen(false);
        drop(weather);
        task.await.unwrap();
        assert!(rx.try_recv().is_err());
        assert_eq!(fake.geocodes.load(Ordering::SeqCst), 0);
    }
}
