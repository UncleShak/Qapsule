//! The app shell: wires the engine and pipe server to the window, the tray and
//! the watcher. Keep logic thin here; testable code lives in `crates/`.

mod lifecycle;
mod watcher;
mod window;

use std::path::PathBuf;
use std::sync::Mutex;

use qapsule_core::{Effect, Settings, UiState, VisibilityMode};
use qapsule_server::{Engine, Listener, Shell, config, pipe_name, run_effect};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewWindow, Wry};

use lifecycle::Lifecycle;
use window::Rect;

/// Sends the core's coalesced updates to the webview, creating or destroying the
/// webview as the capsule comes and goes.
struct WindowShell {
    app: AppHandle,
    lifecycle: Lifecycle,
}

impl Shell for WindowShell {
    fn update(&mut self, state: UiState) {
        self.lifecycle.observe(state.shown);
        // Nothing to do if the webview doesn't exist: it asks for a snapshot when it starts.
        let _ = self.app.emit_to("main", "ui-state", state);
    }

    fn run(&mut self, effect: Effect) {
        run_effect(effect);
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
    window::set_region(&window, &rects);
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

fn build_tray(
    app: &tauri::App,
    current: VisibilityMode,
) -> tauri::Result<Vec<(VisibilityMode, CheckMenuItem<Wry>)>> {
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
    let menu = Menu::with_items(
        app,
        &[&visibility, &PredefinedMenuItem::separator(app)?, &quit],
    )?;

    let mut tray = TrayIconBuilder::with_id("qapsule")
        .tooltip("Qapsule")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => app.exit(0),
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
    Ok(items)
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
            let shell = WindowShell {
                app: app.handle().clone(),
                lifecycle: lifecycle.clone(),
            };
            let pipe = pipe_name();
            let initial = settings.clone();
            let started = tauri::async_runtime::block_on(async move {
                let listener = Listener::bind(&pipe)?;
                let engine = Engine::spawn(initial, shell);
                tauri::async_runtime::spawn(listener.run(engine.clone()));
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

            let mode_items = build_tray(app, settings.visibility)?;
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
