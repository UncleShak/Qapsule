//! The app shell: wires the engine and pipe server to the window, the tray and
//! the watcher. Keep logic thin here; testable code lives in `crates/`.

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
    /// The last `focus` the core asked for.
    focused: bool,
    /// The window that had the foreground before the capsule took it.
    previous_foreground: Option<isize>,
}

impl WindowShell {
    /// The hotkey opened the view: take focus, remembering who had it. Closing gives it back.
    fn follow_focus(&mut self, focus: bool) {
        if focus == self.focused {
            return;
        }
        if focus {
            if let Some(main) = self.app.get_webview_window("main") {
                self.focused = true;
                self.previous_foreground = window::take_focus(&main);
            }
        } else {
            self.focused = false;
            if let Some(previous) = self.previous_foreground.take() {
                window::give_back_focus(self.app.get_webview_window("main").as_ref(), previous);
            }
        }
    }
}

impl Shell for WindowShell {
    fn update(&mut self, state: UiState) {
        let focus = state.focus;
        // Nothing to do if the webview doesn't exist: it asks for a snapshot when it starts.
        let _ = self.app.emit_to("main", "ui-state", state);
        self.follow_focus(focus);
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
    settings: Mutex<Settings>,
    config_path: Option<PathBuf>,
    mode_items: Vec<(VisibilityMode, CheckMenuItem<Wry>)>,
}

#[tauri::command]
async fn get_snapshot(state: State<'_, AppState>) -> Result<UiState, String> {
    state
        .engine
        .snapshot()
        .await
        .ok_or_else(|| "the engine has stopped".into())
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

fn set_mode(app: &AppHandle, mode: VisibilityMode) {
    let state = app.state::<AppState>();
    let settings = {
        let mut settings = state.settings.lock().unwrap();
        settings.visibility = mode;
        settings.clone()
    };
    state.engine.settings(settings);
    for (item_mode, item) in &state.mode_items {
        let _ = item.set_checked(*item_mode == mode);
    }
    if let Some(path) = &state.config_path
        && let Err(e) = config::save_visibility(path, mode)
    {
        eprintln!("qapsule: couldn't save the visibility mode: {e}");
    }
}

type ModeItems = Vec<(VisibilityMode, CheckMenuItem<Wry>)>;

fn build_tray(
    app: &tauri::App,
    current: VisibilityMode,
    notifications: Arc<OnceLock<Notifications>>,
) -> tauri::Result<(ModeItems, MenuItem<Wry>)> {
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
    let menu = Menu::with_items(
        app,
        &[
            &status,
            &PredefinedMenuItem::separator(app)?,
            &visibility,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("qapsule")
        .tooltip("Qapsule")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
            "windows-notifications" => {
                if let Some(notifications) = notifications.get() {
                    notifications.open_settings();
                }
            }
            id => {
                if let Some(mode) = VisibilityMode::parse(id) {
                    set_mode(app, mode);
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok((items, status))
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
            let shell = WindowShell {
                app: app.handle().clone(),
                lifecycle: lifecycle.clone(),
                notifications: notifications.clone(),
                weather: weather.clone(),
                calendar: calendar.clone(),
                focused: false,
                previous_foreground: None,
            };
            let pipe = pipe_name();
            let initial = settings.clone();
            let city = settings.weather_city().map(str::to_owned);
            let calendar_config = CalendarConfig {
                minutes_before: settings.calendar_minutes_before,
                pops: settings.pop_setting("calendar:events").pop,
            };
            let started = tauri::async_runtime::block_on(async move {
                let listener = Listener::bind(&pipe)?;
                let engine = Engine::spawn(initial, shell);
                tauri::async_runtime::spawn(listener.run(engine.clone()));
                // Starts its task on this runtime; nothing is fetched until the main screen opens.
                let _ = weather.set(Weather::start(engine.clone(), city));
                let _ = calendar.set(Calendar::start(engine.clone(), calendar_config));
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

            let (mode_items, status_item) =
                build_tray(app, settings.visibility, notifications.clone())?;
            // Only an unmet permission is actionable; the rest is information.
            let _ = notifications.set(Notifications::start(engine.clone(), move |status| {
                let _ = status_item.set_text(status.label());
                let _ = status_item.set_enabled(status == Status::NeedsPermission);
            }));
            app.manage(AppState {
                engine: engine.clone(),
                settings: Mutex::new(settings),
                config_path,
                mode_items,
            });

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
