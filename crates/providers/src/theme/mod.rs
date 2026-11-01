//! The theme provider: finds the wallpaper, extracts a safe palette from it and gives it
//! to the engine, which sends it to the UI (SPEC "Theme provider").
//!
//! One thread, blocked on a channel. It wakes only for an event:
//! - the app forwards `WM_SETTINGCHANGE` / `SPI_SETDESKWALLPAPER` ([`Theme::wallpaper_changed`]);
//! - Windows raises `ColorValuesChanged` (the accent);
//! - the expanded view opens ([`Theme::main_screen`]: the cheap moment to notice that
//!   Wallpaper Engine started or stopped, since Windows says nothing about it);
//! - Wallpaper Engine's `config.json` changes (a `notify` watch that exists only while
//!   Wallpaper Engine runs);
//! - the settings change.
//!
//! A burst of events is settled for [`QUIET`], then the palette is worked out once.
//!
//! Image sources, in order: Wallpaper Engine's preview (when it runs), the wallpaper
//! from `IDesktopWallpaper`, then one screen capture. A source that gives an image but
//! no colour (grey) ends the search: the Windows accent is used, as SPEC says. A source
//! that fails (logged once) passes on to the next.

mod decode;
mod system;
pub mod wallpaper_engine;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant, SystemTime};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use qapsule_core::{AccentSource, PaletteSource, PaletteView, Settings};
use qapsule_server::Engine;

use crate::colour::{Rgb, parse_css};
use crate::palette::{self, Palette};

/// Quiet time after the last event of a burst before the palette is worked out.
pub const QUIET: Duration = Duration::from_millis(400);
/// A burst is cut off after this long, so a chatty file can't postpone the work for ever.
const MAX_BURST: Duration = Duration::from_secs(3);
/// Palettes kept by image path and modification time.
const CACHE_SIZE: usize = 4;
/// Windows' default dark-mode accent, for when Windows won't tell us.
const DEFAULT_ACCENT: Rgb = Rgb(0x60, 0xCD, 0xFF);

/// What the provider follows in the settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeConfig {
    pub source: AccentSource,
    pub custom_accent: String,
    pub wallpaper_tint: bool,
}

impl ThemeConfig {
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            source: settings.accent_source,
            custom_accent: settings.custom_accent.clone(),
            wallpaper_tint: settings.wallpaper_tint,
        }
    }

    /// Whether any part of the palette comes from the wallpaper. If not, no image is read.
    fn wants_wallpaper(&self) -> bool {
        self.source == AccentSource::Wallpaper || self.wallpaper_tint
    }
}

/// What the worker can be woken by.
pub(crate) enum Event {
    Config(ThemeConfig),
    Wallpaper,
    Accent,
    Recheck,
    WallpaperEngineConfig,
}

/// A handle to the theme thread.
#[derive(Clone)]
pub struct Theme {
    events: Sender<Event>,
    /// The expanded view is open. Read by the worker: no screen capture then.
    expanded: Arc<AtomicBool>,
}

impl Theme {
    /// Starts the thread. It works out the first palette at once.
    ///
    /// The thread lives as long as the process: it owns a sender to its own inbox (the
    /// watchers need one), so the inbox never disconnects and nothing stops it.
    pub fn start(engine: Engine, config: ThemeConfig) -> Theme {
        let (events, inbox) = mpsc::channel();
        let own = events.clone();
        let expanded = Arc::new(AtomicBool::new(false));
        let open = expanded.clone();
        let spawned = std::thread::Builder::new()
            .name("qapsule-theme".into())
            .spawn(move || run(engine, config, inbox, own, open));
        if let Err(e) = spawned {
            eprintln!("qapsule: the theme provider didn't start ({e})");
        }
        Theme { events, expanded }
    }

    /// The settings changed.
    pub fn set_config(&self, config: ThemeConfig) {
        let _ = self.events.send(Event::Config(config));
    }

    /// Windows said the desktop wallpaper changed (`WM_SETTINGCHANGE`, `SPI_SETDESKWALLPAPER`).
    pub fn wallpaper_changed(&self) {
        let _ = self.events.send(Event::Wallpaper);
    }

    /// The expanded view opened or closed. Opening is when a Wallpaper Engine that started
    /// or quit since the last event is noticed.
    pub fn main_screen(&self, visible: bool) {
        self.expanded.store(visible, Ordering::SeqCst);
        if visible {
            let _ = self.events.send(Event::Recheck);
        }
    }
}

/// The palette the UI should wear, from what the sources found.
///
/// `wallpaper` is the extracted palette and where its image came from, if there was one.
/// `windows` is the Windows accent, if known.
pub fn combine(
    config: &ThemeConfig,
    wallpaper: Option<(Palette, PaletteSource)>,
    windows: Option<Rgb>,
) -> PaletteView {
    let windows_accent = palette::safe(windows.unwrap_or(DEFAULT_ACCENT));
    let tint = wallpaper.map(|(p, _)| p.tint.hex());
    let plain = |accent: Rgb, source| PaletteView {
        accent: accent.hex(),
        accent_2: accent.hex(),
        tint: tint.clone(),
        source,
    };
    match config.source {
        AccentSource::Wallpaper => match wallpaper {
            Some((p, source)) => PaletteView {
                accent: p.accent.hex(),
                accent_2: p.accent_2.hex(),
                tint: Some(p.tint.hex()),
                source,
            },
            None => PaletteView {
                tint: None,
                ..plain(windows_accent, PaletteSource::Windows)
            },
        },
        AccentSource::Windows => plain(windows_accent, PaletteSource::Windows),
        AccentSource::Custom => match parse_css(&config.custom_accent) {
            Some(custom) => plain(palette::safe(custom), PaletteSource::Custom),
            None => plain(windows_accent, PaletteSource::Windows),
        },
    }
}

/// A watch on Wallpaper Engine's `config.json`. Dropping it stops the watch.
struct EngineWatch {
    _watcher: RecommendedWatcher,
    config: PathBuf,
}

struct Worker {
    engine: Engine,
    events: Sender<Event>,
    config: ThemeConfig,
    windows_accent: Option<Rgb>,
    /// Exists only while Wallpaper Engine runs.
    watch: Option<EngineWatch>,
    /// Palettes by image path, modification time and length.
    cache: Vec<(PathBuf, SystemTime, u64, Option<Palette>)>,
    /// The one screen capture taken for the current wallpaper state (`None` colour included).
    capture_memo: Option<(WallKey, Option<Palette>)>,
    expanded: Arc<AtomicBool>,
    sent: Option<PaletteView>,
    last_note: String,
}

fn run(
    engine: Engine,
    config: ThemeConfig,
    inbox: mpsc::Receiver<Event>,
    own: Sender<Event>,
    expanded: Arc<AtomicBool>,
) {
    system::init();
    // Kept alive for as long as the thread runs.
    let _accent_watch = system::watch_accent(own.clone());
    let mut worker = Worker {
        engine,
        events: own,
        config,
        windows_accent: system::windows_accent(),
        watch: None,
        cache: Vec::new(),
        capture_memo: None,
        expanded,
        sent: None,
        last_note: String::new(),
    };
    // Start-up qualifies for a capture.
    worker.refresh(true);
    // `recv` fails only if every sender is gone, which can't happen while this thread
    // owns one; the loop is written so that it would end cleanly all the same.
    while let Ok(first) = inbox.recv() {
        let mut batch = vec![first];
        let started = Instant::now();
        loop {
            let left = MAX_BURST.saturating_sub(started.elapsed()).min(QUIET);
            if left.is_zero() {
                break;
            }
            // A timeout ends the burst; so would a disconnect, and the batch collected so
            // far is still processed before `recv` above ends the thread.
            match inbox.recv_timeout(left) {
                Ok(event) => batch.push(event),
                Err(_) => break,
            }
        }
        // A screen capture is only for a start-up, a new wallpaper or new settings.
        let mut capture = false;
        for event in batch {
            match event {
                Event::Config(config) => {
                    worker.config = config;
                    capture = true;
                }
                Event::Wallpaper => capture = true,
                Event::Accent => worker.windows_accent = system::windows_accent(),
                Event::Recheck | Event::WallpaperEngineConfig => {}
            }
        }
        worker.refresh(capture);
    }
}

impl Worker {
    /// Works out the palette and tells the engine if it changed.
    fn refresh(&mut self, may_capture: bool) {
        let wallpaper = if self.config.wants_wallpaper() {
            self.wallpaper(may_capture)
        } else {
            self.watch = None;
            None
        };
        let view = combine(&self.config, wallpaper, self.windows_accent);
        if self.sent.as_ref() != Some(&view) {
            self.engine.set_palette(Some(view.clone()));
            self.sent = Some(view);
        }
    }

    /// Logs a failure once until something else is logged.
    fn note(&mut self, message: String) {
        if self.last_note != message {
            eprintln!("qapsule: theme: {message}");
            self.last_note = message;
        }
    }

    /// The wallpaper's palette and the kind of source it came from. `None` when there is
    /// no colour to use, whether the wallpaper is grey or no source worked.
    fn wallpaper(&mut self, may_capture: bool) -> Option<(Palette, PaletteSource)> {
        if let Some(image) = self.wallpaper_engine_image() {
            match self.palette_of(&image) {
                Ok(found) => return found.map(|p| (p, PaletteSource::WallpaperEngine)),
                Err(why) => self.note(format!("Wallpaper Engine preview: {why}")),
            }
        }
        let wallpaper = system::desktop_wallpaper();
        match &wallpaper {
            Some(image) => match self.palette_of(image) {
                Ok(found) => return found.map(|p| (p, PaletteSource::Wallpaper)),
                Err(why) => self.note(format!("wallpaper: {why}")),
            },
            None => self.note("Windows reports no wallpaper image".into()),
        }
        // Last resort: one capture per wallpaper state, and only when it would show the
        // desktop. Otherwise the Windows accent is used until the next qualifying event.
        let key = wall_key(wallpaper.as_deref());
        let memo = self.capture_memo.as_ref().filter(|(k, _)| *k == key);
        let expanded = self.expanded.load(Ordering::SeqCst);
        match capture_step(
            memo.is_some(),
            may_capture,
            expanded,
            system::desktop_visible,
        ) {
            CaptureStep::Reuse => memo
                .and_then(|(_, p)| *p)
                .map(|p| (p, PaletteSource::Capture)),
            CaptureStep::Skip => None,
            CaptureStep::Capture => {
                let found = match system::capture_primary() {
                    Some((pixels, width, height)) => palette::extract(&pixels, width, height),
                    None => {
                        self.note("the screen couldn't be captured".into());
                        None
                    }
                };
                self.capture_memo = Some((key, found));
                found.map(|p| (p, PaletteSource::Capture))
            }
        }
    }

    /// The preview image of Wallpaper Engine's current wallpaper, if it runs and its
    /// config can be read. Starts or stops the watch on `config.json` to match.
    fn wallpaper_engine_image(&mut self) -> Option<PathBuf> {
        let Some(process) = system::wallpaper_engine() else {
            self.watch = None;
            return None;
        };
        let Some(config) = find_config(process.exe.as_deref()) else {
            self.watch = None;
            self.note("Wallpaper Engine runs but its config.json wasn't found".into());
            return None;
        };
        if self.watch.as_ref().map(|w| &w.config) != Some(&config) {
            self.watch = self.start_watch(&config);
        }
        let selection = std::fs::read_to_string(&config)
            .map_err(|_| "config.json can't be read".to_string())
            .and_then(|text| wallpaper_engine::parse_config(&text).map_err(|e| e.to_string()))
            .and_then(|selection| {
                wallpaper_engine::preview_image(&selection).map_err(|e| e.to_string())
            });
        match selection {
            Ok(image) => Some(image),
            Err(why) => {
                self.note(format!("Wallpaper Engine: {why}"));
                None
            }
        }
    }

    fn start_watch(&mut self, config: &Path) -> Option<EngineWatch> {
        let dir = config.parent()?;
        let name = config.file_name()?.to_owned();
        let events = self.events.clone();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                if is_config_event(&event.kind, &event.paths, &name) {
                    let _ = events.send(Event::WallpaperEngineConfig);
                }
            })
            .ok()?;
        // The folder, not the file: Wallpaper Engine may replace the file when it saves.
        watcher.watch(dir, RecursiveMode::NonRecursive).ok()?;
        Some(EngineWatch {
            _watcher: watcher,
            config: config.to_path_buf(),
        })
    }

    /// The palette of an image file: from the cache while the file is unchanged, else
    /// decoded and extracted. `Ok(None)` is an image with no usable colour.
    fn palette_of(&mut self, path: &Path) -> Result<Option<Palette>, String> {
        let (modified, length) =
            file_stamp(path).ok_or_else(|| "the image can't be read".to_string())?;
        if let Some((_, _, _, found)) = self
            .cache
            .iter()
            .find(|(p, m, l, _)| p == path && *m == modified && *l == length)
        {
            return Ok(*found);
        }
        let (pixels, width, height) = decode::load(path)?;
        let found = palette::extract(&pixels, width, height);
        self.cache.retain(|(p, _, _, _)| p != path);
        self.cache
            .insert(0, (path.to_path_buf(), modified, length, found));
        self.cache.truncate(CACHE_SIZE);
        Ok(found)
    }
}

/// Modification time and length of a file: what tells that an image changed.
fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The state of the wallpaper a capture stands for: the path Windows reports (or none)
/// and that file's stamp.
type WallKey = (Option<PathBuf>, Option<(SystemTime, u64)>);

fn wall_key(path: Option<&Path>) -> WallKey {
    (path.map(Path::to_path_buf), path.and_then(file_stamp))
}

/// What to do about the screen capture.
#[derive(Debug, PartialEq, Eq)]
enum CaptureStep {
    /// This wallpaper state was captured already: use that result.
    Reuse,
    Capture,
    /// Don't capture: use the Windows accent until the next qualifying event.
    Skip,
}

/// The capture gate. A capture happens only if this wallpaper state has none yet, the
/// event qualifies (start-up, a wallpaper change or a settings change), the expanded view
/// is closed, and the desktop is what's on screen. `desktop_visible` is called last, so
/// the OS is asked only when everything else allows it.
fn capture_step(
    memoised: bool,
    qualifying: bool,
    expanded_open: bool,
    desktop_visible: impl FnOnce() -> bool,
) -> CaptureStep {
    if memoised {
        CaptureStep::Reuse
    } else if qualifying && !expanded_open && desktop_visible() {
        CaptureStep::Capture
    } else {
        CaptureStep::Skip
    }
}

/// Whether a file watch event is about `config.json` being written, created, renamed
/// (a rename shows as a `Modify`) or removed. Removal counts: the next read fails and
/// the provider falls through to the other sources.
fn is_config_event(kind: &EventKind, paths: &[PathBuf], name: &OsStr) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) | EventKind::Any
    ) && paths.iter().any(|p| p.file_name() == Some(name))
}

/// Wallpaper Engine's `config.json`: beside its executable (or one folder up, if it runs
/// from `bin`), else in the default Steam install.
fn find_config(exe: Option<&Path>) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(dir) = exe.and_then(Path::parent) {
        candidates.push(dir.join("config.json"));
        if let Some(up) = dir.parent() {
            candidates.push(up.join("config.json"));
        }
    }
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(
                Path::new(&base)
                    .join("Steam")
                    .join("steamapps")
                    .join("common")
                    .join("wallpaper_engine")
                    .join("config.json"),
            );
        }
    }
    candidates.into_iter().find(|c| c.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Palette {
        Palette {
            accent: Rgb(200, 100, 50),
            accent_2: Rgb(50, 100, 200),
            tint: Rgb(90, 60, 60),
        }
    }

    fn config(source: AccentSource) -> ThemeConfig {
        ThemeConfig {
            source,
            custom_accent: "#336699".into(),
            wallpaper_tint: true,
        }
    }

    #[test]
    fn the_wallpaper_gives_all_three_colours() {
        let view = combine(
            &config(AccentSource::Wallpaper),
            Some((palette(), PaletteSource::WallpaperEngine)),
            Some(Rgb(1, 2, 3)),
        );
        assert_eq!(view.accent, "#c86432");
        assert_eq!(view.accent_2, "#3264c8");
        assert_eq!(view.tint.as_deref(), Some("#5a3c3c"));
        assert_eq!(view.source, PaletteSource::WallpaperEngine);
    }

    #[test]
    fn a_grey_wallpaper_falls_back_to_the_windows_accent_without_a_tint() {
        let view = combine(
            &config(AccentSource::Wallpaper),
            None,
            Some(Rgb(0x00, 0x78, 0xD4)),
        );
        assert_eq!(view.source, PaletteSource::Windows);
        assert_eq!(view.tint, None);
        assert_eq!(view.accent, view.accent_2);
        let accent = parse_css(&view.accent).unwrap();
        assert!(crate::colour::contrast(accent, crate::colour::SURFACE) >= 4.5);
    }

    #[test]
    fn the_windows_source_ignores_the_wallpaper_accent_but_keeps_its_tint() {
        let view = combine(
            &config(AccentSource::Windows),
            Some((palette(), PaletteSource::Wallpaper)),
            Some(Rgb(0x60, 0xCD, 0xFF)),
        );
        assert_eq!(view.source, PaletteSource::Windows);
        assert_eq!(view.accent, "#60cdff");
        assert_eq!(view.tint.as_deref(), Some("#5a3c3c"));
    }

    #[test]
    fn a_custom_accent_is_clamped_and_a_bad_one_falls_back() {
        let view = combine(
            &config(AccentSource::Custom),
            None,
            Some(Rgb(0x60, 0xCD, 0xFF)),
        );
        assert_eq!(view.source, PaletteSource::Custom);
        let accent = parse_css(&view.accent).unwrap();
        assert!(crate::colour::contrast(accent, crate::colour::SURFACE) >= 4.5);

        let bad = ThemeConfig {
            custom_accent: "not a colour".into(),
            ..config(AccentSource::Custom)
        };
        let view = combine(&bad, None, None);
        assert_eq!(view.source, PaletteSource::Windows);
        assert_eq!(view.accent, "#60cdff");
    }

    #[test]
    fn the_capture_gate() {
        let never = || -> bool { panic!("the OS must not be asked") };
        // All conditions met.
        assert_eq!(
            capture_step(false, true, false, || true),
            CaptureStep::Capture
        );
        // A view-open recheck, an open expanded view, or a covered desktop: no capture.
        assert_eq!(capture_step(false, false, false, never), CaptureStep::Skip);
        assert_eq!(capture_step(false, true, true, never), CaptureStep::Skip);
        assert_eq!(
            capture_step(false, true, false, || false),
            CaptureStep::Skip
        );
        // A state already captured is reused, whatever else holds.
        assert_eq!(capture_step(true, false, true, never), CaptureStep::Reuse);
    }

    #[test]
    fn the_wallpaper_state_key_follows_path_and_file() {
        assert_eq!(wall_key(None), (None, None));
        let dir = std::env::temp_dir().join(format!("qapsule-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("w.png");
        std::fs::write(&file, b"one").unwrap();
        let first = wall_key(Some(&file));
        assert_eq!(first, wall_key(Some(&file)));
        std::fs::write(&file, b"three").unwrap();
        assert_ne!(first, wall_key(Some(&file)), "a new length is a new state");
        assert_ne!(first, wall_key(Some(&dir.join("other.png"))));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn config_events_include_writes_renames_and_removal() {
        use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};
        let name = OsStr::new("config.json");
        let config = vec![PathBuf::from("C:/we/config.json")];
        let log = vec![PathBuf::from("C:/we/log.txt")];
        let swap = vec![
            PathBuf::from("C:/we/config.tmp"),
            PathBuf::from("C:/we/config.json"),
        ];
        for kind in [
            EventKind::Create(CreateKind::File),
            EventKind::Modify(ModifyKind::Any),
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            EventKind::Remove(RemoveKind::File),
        ] {
            assert!(is_config_event(&kind, &config, name), "{kind:?}");
            assert!(!is_config_event(&kind, &log, name), "{kind:?}");
        }
        // A save by rename: the temp file replaces config.json.
        let rename = EventKind::Modify(ModifyKind::Name(RenameMode::Both));
        assert!(is_config_event(&rename, &swap, name));
        let access = EventKind::Access(AccessKind::Any);
        assert!(!is_config_event(&access, &config, name));
    }

    #[test]
    fn no_image_is_read_when_nothing_wants_the_wallpaper() {
        let mut c = config(AccentSource::Custom);
        c.wallpaper_tint = false;
        assert!(!c.wants_wallpaper());
        c.wallpaper_tint = true;
        assert!(c.wants_wallpaper());
        c.source = AccentSource::Wallpaper;
        c.wallpaper_tint = false;
        assert!(c.wants_wallpaper());
    }
}
