//! The only WinRT code of the media provider: follows the session Windows calls
//! current through its change events (no polling) and reads its properties,
//! timeline and cover. It never logs titles, artists or app names; errors log
//! HRESULTs and fixed text.
//!
//! The provider lives for the whole process and is never torn down: its thread,
//! its event subscriptions and the manager stay until the process exits. Only a
//! change of session unsubscribes from the old session.
//!
//! Players can hang, so every asynchronous call has a deadline (`answer`), and
//! the buttons run on their own thread.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use qapsule_server::Engine;
use windows::ApplicationModel::AppInfo;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Imaging::{
    BitmapAlphaMode, BitmapDecoder, BitmapEncoder, BitmapInterpolationMode, BitmapPixelFormat,
    BitmapTransform, ColorManagementMode, ExifOrientationMode,
};
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    MediaPropertiesChangedEventArgs, PlaybackInfoChangedEventArgs,
    TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::core::{HRESULT, Interface, RuntimeType};
use windows_future::{AsyncOperationCompletedHandler, IAsyncInfo, IAsyncOperation};

use super::answer::{ANSWER_WITHIN, Background, Op, Waited, wait};
use super::art::{ArtStore, MAX_SIDE, MAX_THUMBNAIL_BYTES};
use super::tracker::{self, Control, Event, Playback, RawTimeline, Read, Reading, Source, Wakes};

type Session = GlobalSystemMediaTransportControlsSession;

/// Quiet time after the last event of a burst before the session is read.
const QUIET: Duration = Duration::from_millis(150);
/// Covers wider or taller than this (decoded) are not used.
const MAX_COVER_SIDE: u32 = 2048;
/// The thumbnail is read in pieces of this size.
const CHUNK: u64 = 64 * 1024;
/// All the reading and scaling of one cover must finish within this.
const COVER_BUDGET: Duration = Duration::from_secs(10);
/// ERROR_TIMEOUT as an HRESULT: what a call that didn't answer fails with.
const TIMED_OUT: i32 = 0x8007_05B4_u32 as i32;
/// Windows counts time in 100 ns ticks since 1601; the Unix epoch is this many ticks in.
const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

fn code(e: &windows::core::Error) -> i32 {
    e.code().0
}

struct Async<T: RuntimeType + 'static>(IAsyncOperation<T>);

impl<T: RuntimeType + 'static> Op for Async<T> {
    type Output = T;

    fn on_done(&self, done: Box<dyn FnOnce() + Send>) -> Result<(), i32> {
        // The handler may be called more than once as far as the type goes; `done` runs once.
        let done = Mutex::new(Some(done));
        self.0
            .SetCompleted(&AsyncOperationCompletedHandler::new(move |_, _| {
                let once = done.lock().ok().and_then(|mut slot| slot.take());
                if let Some(done) = once {
                    done();
                }
                Ok(())
            }))
            .map_err(|e| code(&e))
    }

    fn result(&self) -> Result<T, i32> {
        self.0.GetResults().map_err(|e| code(&e))
    }

    fn cancel(&self) {
        if let Ok(info) = self.0.cast::<IAsyncInfo>() {
            let _ = info.Cancel();
        }
    }
}

/// The outcome of a waited call as a Windows result. A call that didn't answer
/// in time fails with `TIMED_OUT`, after a log line that says nothing about the player.
fn finish<T>(waited: Waited<T>) -> windows::core::Result<T> {
    match waited {
        Waited::Done(value) => Ok(value),
        Waited::TimedOut => {
            eprintln!("qapsule: media player didn't answer");
            Err(HRESULT(TIMED_OUT).into())
        }
        Waited::Failed(code) => Err(HRESULT(code).into()),
    }
}

/// Waits for a call, for at most [`ANSWER_WITHIN`].
fn ask<T: RuntimeType + 'static>(
    op: windows::core::Result<IAsyncOperation<T>>,
) -> windows::core::Result<T> {
    finish(wait(&Async(op?), ANSWER_WITHIN))
}

pub struct Media {
    wakes: Wakes,
}

impl Media {
    /// Follows the current media session on its own thread and reports to `engine`.
    /// Does nothing visible if Windows has no media controls.
    pub fn start(engine: Engine, art: Arc<ArtStore>) -> Media {
        let (wakes, inbox) = Wakes::new();
        let thread_wakes = wakes.clone();
        let spawned = std::thread::Builder::new()
            .name("qapsule-media".into())
            .spawn(move || {
                // Already-initialised is fine; the result is not needed.
                let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
                let manager =
                    match ask(GlobalSystemMediaTransportControlsSessionManager::RequestAsync()) {
                        Ok(manager) => manager,
                        Err(e) => {
                            eprintln!("qapsule: now playing unavailable ({})", code(&e));
                            return;
                        }
                    };
                let events = thread_wakes.clone();
                let handler = TypedEventHandler::<
                    GlobalSystemMediaTransportControlsSessionManager,
                    CurrentSessionChangedEventArgs,
                >::new(move |_, _| {
                    events.raise(Event::SessionChanged);
                    Ok(())
                });
                // Never removed: the subscription lasts as long as the process.
                if let Err(e) = manager.CurrentSessionChanged(&handler) {
                    eprintln!("qapsule: now playing unavailable ({})", code(&e));
                    return;
                }
                // A button can take a player seconds (or never) to answer, so it runs
                // on a thread of its own and never holds up the events.
                let Some(buttons) = Background::spawn("qapsule-media-button", press) else {
                    eprintln!("qapsule: now playing unavailable: thread");
                    return;
                };
                let source = WindowsSource {
                    manager,
                    wakes: thread_wakes,
                    buttons,
                    current: None,
                };
                tracker::run(source, inbox, art, QUIET, |state| engine.set_media(state));
            });
        if let Err(e) = spawned {
            eprintln!("qapsule: now playing unavailable: thread ({e})");
        }
        Media { wakes }
    }

    /// Runs a button of the music view; anything else is ignored.
    pub fn act(&self, source: &str, id: &str, action: &str) {
        if source != "media:" || id != "now-playing" {
            return;
        }
        let control = match action {
            "media.play_pause" => Control::PlayPause,
            "media.next" => Control::Next,
            "media.previous" => Control::Previous,
            _ => return,
        };
        self.wakes.control(control);
    }
}

/// Presses a button on a session, on the button thread.
fn press((session, control): (Session, Control)) {
    let started = match control {
        Control::PlayPause => session.TryTogglePlayPauseAsync(),
        Control::Next => session.TrySkipNextAsync(),
        Control::Previous => session.TrySkipPreviousAsync(),
    };
    match ask(started) {
        Ok(true) => {}
        Ok(false) => eprintln!("qapsule: the player refused the media button"),
        // Already logged by `finish`.
        Err(e) if code(&e) == TIMED_OUT => {}
        Err(e) => eprintln!("qapsule: media button failed ({})", e.code()),
    }
}

/// The session being followed, with the event subscriptions to undo.
struct Current {
    session: Session,
    app: String,
    tokens: [i64; 3],
}

struct WindowsSource {
    manager: GlobalSystemMediaTransportControlsSessionManager,
    wakes: Wakes,
    buttons: Background<(Session, Control)>,
    current: Option<Current>,
}

impl WindowsSource {
    fn unsubscribe(&mut self) {
        if let Some(old) = self.current.take() {
            let _ = old.session.RemoveMediaPropertiesChanged(old.tokens[0]);
            let _ = old.session.RemovePlaybackInfoChanged(old.tokens[1]);
            let _ = old.session.RemoveTimelinePropertiesChanged(old.tokens[2]);
        }
    }

    fn subscribe(&self, session: &Session) -> windows::core::Result<[i64; 3]> {
        let (a, b, c) = (self.wakes.clone(), self.wakes.clone(), self.wakes.clone());
        let properties =
            TypedEventHandler::<Session, MediaPropertiesChangedEventArgs>::new(move |_, _| {
                a.raise(Event::Properties);
                Ok(())
            });
        let playback =
            TypedEventHandler::<Session, PlaybackInfoChangedEventArgs>::new(move |_, _| {
                b.raise(Event::Playback);
                Ok(())
            });
        let timeline =
            TypedEventHandler::<Session, TimelinePropertiesChangedEventArgs>::new(move |_, _| {
                c.raise(Event::Timeline);
                Ok(())
            });
        // WinRT keeps its own reference to each handler until it is removed.
        let first = session.MediaPropertiesChanged(&properties)?;
        let second = match session.PlaybackInfoChanged(&playback) {
            Ok(token) => token,
            Err(e) => {
                let _ = session.RemoveMediaPropertiesChanged(first);
                return Err(e);
            }
        };
        match session.TimelinePropertiesChanged(&timeline) {
            Ok(third) => Ok([first, second, third]),
            Err(e) => {
                let _ = session.RemoveMediaPropertiesChanged(first);
                let _ = session.RemovePlaybackInfoChanged(second);
                Err(e)
            }
        }
    }
}

impl Source for WindowsSource {
    fn session_changed(&mut self) {
        self.unsubscribe();
        // No current session is an error here, and simply means nothing plays.
        let Ok(session) = self.manager.GetCurrentSession() else {
            return;
        };
        match self.subscribe(&session) {
            Ok(tokens) => {
                let app = app_name(&session);
                self.current = Some(Current {
                    session,
                    app,
                    tokens,
                });
            }
            Err(e) => eprintln!("qapsule: media events failed ({})", e.code()),
        }
    }

    fn read(&mut self) -> Read {
        let Some(current) = self.current.as_ref() else {
            return Read::NoSession;
        };
        let session = &current.session;
        let read = || -> windows::core::Result<Reading> {
            let properties = ask(session.TryGetMediaPropertiesAsync())?;
            let title = properties.Title()?.to_string();
            let mut artist = properties.Artist()?.to_string();
            if artist.is_empty() {
                artist = properties.AlbumArtist()?.to_string();
            }
            let info = session.GetPlaybackInfo()?;
            let playback = match info.PlaybackStatus()? {
                Status::Playing => Playback::Playing,
                Status::Paused => Playback::Paused,
                Status::Opened | Status::Changing => Playback::Changing,
                _ => Playback::Stopped,
            };
            let rate = info.PlaybackRate().and_then(|r| r.Value()).unwrap_or(1.0);
            let timeline = session.GetTimelineProperties().ok().map(|t| {
                let span = |d: windows::Foundation::TimeSpan| ticks(d.Duration);
                let now = unix_ticks();
                let stamped = t
                    .LastUpdatedTime()
                    .map_or(now, |time| time.UniversalTime - UNIX_EPOCH_TICKS);
                RawTimeline {
                    start: t.StartTime().map_or(Duration::ZERO, span),
                    end: t.EndTime().map_or(Duration::ZERO, span),
                    position: t.Position().map_or(Duration::ZERO, span),
                    age: ticks(now.saturating_sub(stamped)),
                    rate,
                }
            });
            Ok(Reading {
                app: current.app.clone(),
                title,
                artist,
                playback,
                timeline,
            })
        };
        match read() {
            Ok(reading) => Read::Session(reading),
            Err(e) => {
                // A timeout was logged where it happened.
                if code(&e) != TIMED_OUT {
                    eprintln!("qapsule: media read failed ({})", e.code());
                }
                Read::Unavailable
            }
        }
    }

    fn read_art(&mut self) -> Option<Vec<u8>> {
        let session = &self.current.as_ref()?.session;
        match read_cover(session) {
            Ok(png) => png,
            Err(e) => {
                if code(&e) != TIMED_OUT {
                    eprintln!("qapsule: cover failed ({})", e.code());
                }
                None
            }
        }
    }

    fn control(&mut self, control: Control) {
        let Some(current) = &self.current else {
            return;
        };
        if !self.buttons.offer((current.session.clone(), control)) {
            eprintln!("qapsule: media button ignored (player busy)");
        }
    }
}

/// The session's thumbnail as a PNG of at most 256 x 256, or `None` if it has none or
/// is too big. The bytes read are capped whatever the stream claims its size is.
fn read_cover(session: &Session) -> windows::core::Result<Option<Vec<u8>>> {
    let started = Instant::now();
    let refuse = |why: &str| {
        eprintln!("qapsule: cover refused ({why})");
        Ok(None)
    };
    let properties = ask(session.TryGetMediaPropertiesAsync())?;
    let Ok(thumbnail) = properties.Thumbnail() else {
        return Ok(None);
    };
    let stream = ask(thumbnail.OpenReadAsync())?;
    let size = stream.Size()?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return refuse("size");
    }

    // Read at most one byte more than allowed, so a stream that lies about its size
    // can't get past the limit.
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
    let mut total: u64 = 0;
    loop {
        if started.elapsed() > COVER_BUDGET {
            return refuse("too slow");
        }
        let want = (MAX_THUMBNAIL_BYTES + 1 - total).min(CHUNK) as u32;
        let loaded = ask(reader.LoadAsync(want))?;
        if loaded == 0 {
            break;
        }
        total += u64::from(loaded);
        if total > MAX_THUMBNAIL_BYTES {
            return refuse("size");
        }
    }
    if total == 0 {
        return refuse("empty");
    }
    let mut bytes = vec![0u8; total as usize];
    reader.ReadBytes(&mut bytes)?;

    // Decode from memory: the player's stream is done with.
    let memory = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&memory.GetOutputStreamAt(0)?)?;
    writer.WriteBytes(&bytes)?;
    ask(writer.StoreAsync())?;
    ask(writer.FlushAsync())?;
    writer.DetachStream()?;
    drop(bytes);
    memory.Seek(0)?;

    let decoder = ask(BitmapDecoder::CreateAsync(&memory))?;
    let (width, height) = (decoder.PixelWidth()?, decoder.PixelHeight()?);
    if width == 0 || height == 0 || width > MAX_COVER_SIDE || height > MAX_COVER_SIDE {
        return refuse("dimensions");
    }
    let (scaled_width, scaled_height) = fit(width, height);
    let transform = BitmapTransform::new()?;
    transform.SetScaledWidth(scaled_width)?;
    transform.SetScaledHeight(scaled_height)?;
    transform.SetInterpolationMode(BitmapInterpolationMode::Fant)?;
    let pixels = ask(decoder.GetPixelDataTransformedAsync(
        BitmapPixelFormat::Bgra8,
        BitmapAlphaMode::Premultiplied,
        &transform,
        ExifOrientationMode::IgnoreExifOrientation,
        ColorManagementMode::DoNotColorManage,
    ))?
    .DetachPixelData()?;

    let out = InMemoryRandomAccessStream::new()?;
    let encoder = ask(BitmapEncoder::CreateAsync(
        BitmapEncoder::PngEncoderId()?,
        &out,
    ))?;
    encoder.SetPixelData(
        BitmapPixelFormat::Bgra8,
        BitmapAlphaMode::Premultiplied,
        scaled_width,
        scaled_height,
        96.0,
        96.0,
        &pixels,
    )?;
    encoder.FlushAsync()?.join()?;

    let size = out.Size()?;
    if size == 0 || size > MAX_THUMBNAIL_BYTES {
        return refuse("encoded size");
    }
    let reader = DataReader::CreateDataReader(&out.GetInputStreamAt(0)?)?;
    ask(reader.LoadAsync(size as u32))?;
    let mut png = vec![0u8; size as usize];
    reader.ReadBytes(&mut png)?;
    Ok(Some(png))
}

/// The size that fits `width` x `height` in `MAX_SIDE` square, never larger than the original.
fn fit(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= MAX_SIDE {
        return (width, height);
    }
    let scale =
        |side: u32| (u64::from(side) * u64::from(MAX_SIDE) / u64::from(longest)).max(1) as u32;
    (scale(width), scale(height))
}

fn ticks(ticks: i64) -> Duration {
    Duration::from_nanos(u64::try_from(ticks).unwrap_or(0).saturating_mul(100))
}

/// Now, in 100 ns ticks since the Unix epoch.
fn unix_ticks() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| (d.as_nanos() / 100) as i64)
}

/// The player's name: the packaged app's display name, or the exe name from the
/// session's id. Never the whole id.
fn app_name(session: &Session) -> String {
    let Ok(aumid) = session.SourceAppUserModelId() else {
        return "Player".into();
    };
    AppInfo::GetFromAppUserModelId(&aumid)
        .and_then(|info| info.DisplayInfo())
        .and_then(|display| display.DisplayName())
        .map(|name| name.to_string())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| super::name::from_aumid(&aumid.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_scale_down_to_256_and_never_up() {
        assert_eq!(fit(1000, 500), (256, 128));
        assert_eq!(fit(500, 1000), (128, 256));
        assert_eq!(fit(256, 256), (256, 256));
        assert_eq!(fit(100, 50), (100, 50));
        assert_eq!(fit(10_000, 1), (256, 1));
    }

    #[test]
    fn ticks_convert_and_negative_ones_are_zero() {
        assert_eq!(ticks(10_000_000), Duration::from_secs(1));
        assert_eq!(ticks(-5), Duration::ZERO);
    }
}
