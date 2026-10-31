//! The app shell: wires the engine and pipe server to the window, the tray and
//! the watcher. Keep logic thin here; testable code lives in `crates/`.

mod autostart;
pub mod identity;
mod lifecycle;
mod monitor;
mod watcher;
mod window;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use qapsule_core::{Effect, PresenceEffect, Settings, UiState, VisibilityMode};
use qapsule_providers::calendar::{Calendar, CalendarConfig};
use qapsule_providers::notifications::{Notifications, Status};
use qapsule_providers::weather::Weather;
use qapsule_server::config::{Change, Live};
use qapsule_server::config_watch::{self, ConfigWatch};
use qapsule_server::{Engine, Listener, Shell, config, pipe_name, run_effect};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewWindow, Wry};

use lifecycle::Lifecycle;
use window::Rect;

/// Sends the core's coalesced updates to the webview, and applies its presence
/// effects: creating or destroying the webview as the capsule comes and goes.
struct WindowShell {
    app: AppHandle,
    lifecycle: Lifecycle,
    /// Set once the engine exists; the shell is built before it.
    notifications: Arc<OnceLock<Notifications>>,
    /// Set once the engine exists, like `notifications`.
    weather: Arc<OnceLock<Weather>>,
    calendar: Arc<OnceLock<Calendar>>,
    focus: Arc<window::Focus>,
}

impl Shell for WindowShell {
    fn update(&mut self, state: UiState) {
        let focus = state.focus;
        // Nothing to emit if the webview doesn't exist: it asks for a snapshot when it starts.
        let _ = self.app.emit_to("main", "ui-state", state);
        self.focus
            .follow(focus, self.app.get_webview_window("main").as_ref(), false);
    }

    fn run(&mut self, effect: Effect) {
        match effect {
            Effect::Provider { source, id, action } if source.starts_with("windows:") => {
                if let Some(notifications) = self.notifications.get() {
                    notifications.act(&source, &id, &action);
                }
            }
            effect => run_effect(effect),
        }
    }

    fn presence(&mut self, effect: PresenceEffect) {
        self.lifecycle.apply(effect);
    }

    fn main_screen(&mut self, visible: bool) {
        if let Some(weather) = self.weather.get() {
            weather.main_screen(visible);
        }
        if let Some(calendar) = self.calendar.get() {
            calendar.main_screen(visible);
        }
    }
}

struct AppState {
    engine: Engine,
    settings: Mutex<Live>,
    config_path: Option<PathBuf>,
    mode_items: Vec<(VisibilityMode, CheckMenuItem<Wry>)>,
    dnd_item: CheckMenuItem<Wry>,
    autostart_item: CheckMenuItem<Wry>,
    weather: Arc<OnceLock<Weather>>,
    calendar: Arc<OnceLock<Calendar>>,
    /// Keeps the config file watch alive for as long as the app runs.
    config_watch: Mutex<Option<ConfigWatch>>,
    focus: Arc<window::Focus>,
}

#[tauri::command]
async fn get_snapshot(
    state: State<'_, AppState>,
    window: WebviewWindow,
) -> Result<UiState, String> {
    let snapshot = state
        .engine
        .snapshot()
        .await
        .ok_or_else(|| String::from("the engine has stopped"))?;
    // A webview recreated while focus is wanted takes it now; one that isn't wanted doesn't.
    state.focus.follow(snapshot.focus, Some(&window), true);
    Ok(snapshot)
}

#[tauri::command]
fn press_action(state: State<'_, AppState>, source: String, id: String, action: String) {
    state.engine.press(source, id, action);
}

#[tauri::command]
fn dismiss_pop(state: State<'_, AppState>) {
    state.engine.dismiss_pop();
}

#[tauri::command]
fn pointer_slowed(state: State<'_, AppState>) {
    state.engine.pointer_slowed();
}

#[tauri::command]
fn expand_click(state: State<'_, AppState>) {
    state.engine.expand_click();
}

#[tauri::command]
fn close_expanded(state: State<'_, AppState>) {
    state.engine.close_expanded();
}

#[tauri::command]
fn undo(state: State<'_, AppState>) {
    state.engine.undo();
}

#[tauri::command]
fn swap_peek(state: State<'_, AppState>) {
    state.engine.swap_peek();
}

#[tauri::command]
fn set_pointer(state: State<'_, AppState>, over: bool) {
    state.engine.pointer(over);
}

/// The UI reports where the capsule is drawn; everything else lets clicks through.
#[tauri::command]
fn set_hit_rects(window: WebviewWindow, rects: Vec<Rect>) {
    if window::is_cleared() && !rects.is_empty() {
        return;
    }
    window::set_region(&window, &rects);
}

/// The webview no longer exists; the core decides whether to make another.
fn webview_gone(app: &AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        state.engine.webview_gone();
    }
}

/// Sends settings to the engine and the providers that follow them. Called with
/// the settings lock held (see `config::Live`): sends are channel pushes that never
/// block, and holding the lock keeps them in the same order as the changes.
fn push_settings(state: &AppState, settings: &Settings) {
    state.engine.settings(settings.clone());
    if let Some(weather) = state.weather.get() {
        weather.set_city(settings.weather_city().map(str::to_owned));
    }
    if let Some(calendar) = state.calendar.get() {
        calendar.set_config(calendar_config(settings));
    }
}

/// Makes the checkboxes match the settings in memory. Never call it with the
/// settings lock held: `set_checked` waits for the main thread, and the tray
/// handler that runs there takes the same lock.
fn sync_tray(state: &AppState) {
    let current = state.settings.lock().unwrap().settings.clone();
    for (mode, item) in &state.mode_items {
        let _ = item.set_checked(*mode == current.visibility);
    }
    let _ = state.dnd_item.set_checked(current.do_not_disturb);
}

/// A click on the tray: the change takes effect at once and is saved, or
/// remembered if the file is invalid.
fn user_change(app: &AppHandle, change: impl FnOnce(&Settings) -> Change) {
    let state = app.state::<AppState>();
    {
        let mut live = state.settings.lock().unwrap();
        let change = change(&live.settings);
        live.change(change, state.config_path.as_deref(), &mut |s| {
            push_settings(&state, s)
        });
    }
    sync_tray(&state);
}

fn toggle_autostart(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Err(reason) = autostart::set_enabled(!autostart::is_enabled()) {
        eprintln!("qapsule: start with Windows not changed ({reason})");
    }
    // Show what the registry says, whatever happened.
    let _ = state.autostart_item.set_checked(autostart::is_enabled());
}

/// Applies settings reloaded from the file, with any tray edits that couldn't be
/// saved laid on top. Identical settings (our own saves come back through the
/// watcher) change nothing.
fn apply_reloaded(app: &AppHandle, new: Settings, warnings: Vec<String>) {
    let state = app.state::<AppState>();
    let changed = {
        let mut live = state.settings.lock().unwrap();
        live.reloaded(new, state.config_path.as_deref(), &mut |s| {
            push_settings(&state, s)
        })
    };
    if changed {
        for warning in warnings {
            eprintln!("qapsule: {warning}");
        }
        sync_tray(&state);
    }
}

fn calendar_config(settings: &Settings) -> CalendarConfig {
    CalendarConfig {
        minutes_before: settings.calendar_minutes_before,
        pops: settings.pop_setting("calendar:events").pop,
    }
}

struct TrayItems {
    modes: Vec<(VisibilityMode, CheckMenuItem<Wry>)>,
    status: MenuItem<Wry>,
    dnd: CheckMenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

fn build_tray(
    app: &tauri::App,
    settings: &Settings,
    notifications: Arc<OnceLock<Notifications>>,
) -> tauri::Result<TrayItems> {
    let current = settings.visibility;
    let modes = [
        (VisibilityMode::Auto, "Auto"),
        (VisibilityMode::Always, "Always show"),
        (VisibilityMode::OnDemand, "Peek on demand"),
    ];
    let mut items = Vec::new();
    for (mode, label) in modes {
        let item = CheckMenuItem::with_id(
            app,
            mode.as_str(),
            label,
            true,
            mode == current,
            None::<&str>,
        )?;
        items.push((mode, item));
    }
    let refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|(_, i)| i as _).collect();
    let visibility = Submenu::with_items(app, "Visibility", true, &refs)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Qapsule", true, None::<&str>)?;
    let status = MenuItem::with_id(
        app,
        "windows-notifications",
        Status::Starting.label(),
        false,
        None::<&str>,
    )?;
    let dnd = CheckMenuItem::with_id(
        app,
        "do-not-disturb",
        "Do Not Disturb",
        true,
        settings.do_not_disturb,
        None::<&str>,
    )?;
    // A dev build loads its UI from the Vite server, which isn't running at
    // sign-in, so starting it then would show nothing.
    let release = !cfg!(debug_assertions);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        if release {
            "Start with Windows"
        } else {
            "Start with Windows (built app only)"
        },
        release,
        autostart::is_enabled(),
        None::<&str>,
    )?;
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &dnd,
            &visibility,
            &autostart,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("qapsule")
        .tooltip("Qapsule")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "do-not-disturb" => {
                user_change(app, |s| Change::DoNotDisturb(!s.do_not_disturb));
            }
            "autostart" => toggle_autostart(app),
            "windows-notifications" => {
                if let Some(notifications) = notifications.get() {
                    notifications.open_settings();
                }
            }
            id => {
                if let Some(mode) = VisibilityMode::parse(id) {
                    user_change(app, |_| Change::Visibility(mode));
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(TrayItems {
        modes: items,
        status,
        dnd,
        autostart,
    })
}

/// Reloads the config when the file changes. Without a watch the app still works;
/// edits then apply at the next start.
fn start_config_watch(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(path) = state.config_path.clone() else {
        return;
    };
    let handle = app.clone();
    // The watch spawns its task on the runtime, so start it inside that.
    let watch = tauri::async_runtime::block_on(async move {
        config_watch::watch(&path, move |settings, warnings| {
            apply_reloaded(&handle, settings, warnings)
        })
    });
    match watch {
        Ok(watch) => *state.config_watch.lock().unwrap() = Some(watch),
        Err(reason) => eprintln!("qapsule: config changes won't be picked up ({reason})"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            press_action,
            dismiss_pop,
            undo,
            swap_peek,
            set_pointer,
            pointer_slowed,
            expand_click,
            close_expanded,
            set_hit_rects
        ])
        .setup(|app| {
            let config_path = config::config_path();
            let (settings, warnings) = match &config_path {
                Some(path) => config::load(path),
                None => (Settings::default(), Vec::new()),
            };
            for warning in warnings {
                eprintln!("qapsule: {warning}");
            }

            let lifecycle = Lifecycle::new(app.handle().clone());
            let notifications = Arc::new(OnceLock::new());
            let weather = Arc::new(OnceLock::new());
            let calendar = Arc::new(OnceLock::new());
            let focus = Arc::new(window::Focus::default());
            let shell = WindowShell {
                app: app.handle().clone(),
                lifecycle: lifecycle.clone(),
                notifications: notifications.clone(),
                weather: weather.clone(),
                calendar: calendar.clone(),
                focus: focus.clone(),
            };
            let pipe = pipe_name();
            let initial = settings.clone();
            let city = settings.weather_city().map(str::to_owned);
            let calendar_config = calendar_config(&settings);
            let (weather_slot, calendar_slot) = (weather.clone(), calendar.clone());
            let started = tauri::async_runtime::block_on(async move {
                let listener = Listener::bind(&pipe)?;
                let engine = Engine::spawn(initial, shell);
                tauri::async_runtime::spawn(listener.run(engine.clone()));
                // Starts its task on this runtime; nothing is fetched until the main screen opens.
                let _ = weather_slot.set(Weather::start(engine.clone(), city));
                let _ = calendar_slot.set(Calendar::start(engine.clone(), calendar_config));
                Ok::<_, std::io::Error>(engine)
            });
            let engine = match started {
                Ok(engine) => engine,
                Err(e) => {
                    // Most likely another Qapsule is already running.
                    eprintln!("qapsule: couldn't open the pipe ({e}); is Qapsule already running?");
                    std::process::exit(1);
                }
            };

            let tray = build_tray(app, &settings, notifications.clone())?;
            let status_item = tray.status;
            // Only an unmet permission is actionable; the rest is information.
            let _ = notifications.set(Notifications::start(engine.clone(), move |status| {
                let _ = status_item.set_text(status.label());
                let _ = status_item.set_enabled(status == Status::NeedsPermission);
            }));
            app.manage(AppState {
                engine: engine.clone(),
                settings: Mutex::new(Live::new(settings)),
                config_path,
                mode_items: tray.modes,
                dnd_item: tray.dnd,
                autostart_item: tray.autostart,
                weather,
                calendar,
                config_watch: Mutex::new(None),
                focus,
            });
            start_config_watch(app.handle());

            let handle = app.handle().clone();
            watcher::spawn(engine, move || {
                if let Some(main) = handle.get_webview_window("main") {
                    window::place(&main);
                }
            });
            // The window is created (or not) by the lifecycle, now that commands can be answered.
            lifecycle.start();
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            // Destroying the webview closes the last window; Qapsule keeps running
            // until Quit in the tray menu.
            if let RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
