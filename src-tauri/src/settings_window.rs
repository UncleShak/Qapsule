//! The settings window: a normal window made on demand from the tray and destroyed
//! when closed, so it costs nothing while closed. It is independent of the capsule's
//! webview (decisions 0003 and 0005): the lifecycle never sees it.
//!
//! Traffic between this window and Rust is request/response commands, not UI updates,
//! so the coalescer rule (at most 4 updates a second) doesn't apply. The capsule still
//! gets its changes through the coalescer, because `set_appearance` goes through the
//! same `push_settings` path as the tray and the config watch.

use qapsule_core::Settings;
use qapsule_providers::colour::parse_css;
use qapsule_server::config::{Appearance, AppearancePatch, Change};
use tauri::utils::config::Color;
use tauri::{AppHandle, Manager, State, Theme, WebviewUrl, WebviewWindowBuilder};

use crate::AppState;

const LABEL: &str = "settings";

/// `#rrggbb` for any colour the theme provider can read.
fn normalise_colour(text: &str) -> Option<String> {
    parse_css(text).map(|c| c.hex())
}

/// Opens the window, or focuses it if it is already open.
pub fn open(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window(LABEL) {
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        return;
    }
    // Creating a window from the event loop thread (a tray click) can deadlock on Windows.
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let built = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("settings.html".into()))
            .title("Qapsule settings")
            .inner_size(420.0, 420.0)
            .resizable(false)
            .maximizable(false)
            .center()
            .theme(Some(Theme::Dark))
            .background_color(Color(14, 14, 18, 255))
            .build();
        match built {
            Ok(window) => {
                let _ = window.set_focus();
            }
            // A second click that raced the first.
            Err(_) if app.get_webview_window(LABEL).is_some() => {}
            Err(e) => eprintln!("qapsule: couldn't open the settings window: {e}"),
        }
    });
}

#[tauri::command]
pub fn get_appearance(state: State<'_, AppState>) -> Appearance {
    Appearance::of(&state.settings.lock().unwrap().settings)
}

/// Validates, applies live, saves to the config file, and returns what is now in effect.
#[tauri::command]
pub fn set_appearance(
    state: State<'_, AppState>,
    patch: AppearancePatch,
) -> Result<Appearance, String> {
    let patch = patch.validated(normalise_colour)?;
    let mut live = state.settings.lock().unwrap();
    live.change(
        Change::Appearance(patch),
        state.config_path.as_deref(),
        &mut |s: &Settings| crate::push_settings(&state, s),
    );
    Ok(Appearance::of(&live.settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_checked_by_the_real_parser_and_stored_as_lower_case_hex() {
        assert_eq!(normalise_colour("#60CDFF").as_deref(), Some("#60cdff"));
        assert_eq!(normalise_colour(" #abc ").as_deref(), Some("#aabbcc"));
        assert_eq!(normalise_colour("zzz"), None);
        assert_eq!(normalise_colour("#12345"), None);
        assert_eq!(normalise_colour(""), None);
        let bad = AppearancePatch {
            custom_accent: Some("#12".into()),
            ..Default::default()
        };
        assert!(bad.validated(normalise_colour).is_err());
    }
}
