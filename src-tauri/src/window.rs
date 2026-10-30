//! The one capsule window: flush with the top edge, out of the taskbar and
//! Alt+Tab, and clipped to a region so clicks outside the pill pass through.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;
use tauri::{AppHandle, PhysicalPosition, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WM_NCACTIVATE, WM_NCPAINT,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

use crate::monitor;

/// The last region the UI asked for, in CSS pixels, so it can be redone when the
/// display scale changes without the UI noticing anything.
static LAST_REGION: Mutex<Vec<Rect>> = Mutex::new(Vec::new());

/// Rust has emptied the region because the capsule is hidden. While set, the UI's
/// late `set_hit_rects` calls can't make anything clickable again: this is Rust's
/// "empty when hidden" guarantee (decision 3). Reset when the capsule is shown.
static CLEARED: AtomicBool = AtomicBool::new(false);

pub fn set_cleared(cleared: bool) {
    CLEARED.store(cleared, Ordering::SeqCst);
}

pub fn is_cleared() -> bool {
    CLEARED.load(Ordering::SeqCst)
}

/// A rectangle in CSS pixels, relative to the window.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Creates the capsule window from the `main` entry in tauri.conf.json (which has
/// `create: false`, so this is the only place it is made). The webview starts with
/// nothing clickable; the UI paints the capsule tucked, then slides it in.
/// Don't call from a synchronous command or the event loop thread: on Windows that deadlocks.
pub fn create(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .expect("tauri.conf.json defines the main window");
    let window = WebviewWindowBuilder::from_config(app, &config)?.build()?;
    make_tool_window(&window);
    place(&window);
    set_region(&window, &[]);
    let subclassed = window.clone();
    app.run_on_main_thread(move || hide_frame_painting(&subclassed))?;
    let placed = window.clone();
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        // Only a display scale change resizes the window. Re-centre on the resize, not on
        // ScaleFactorChanged: after that event tao moves the window to the rect Windows
        // suggests, which grows from the left edge and would undo the centring.
        WindowEvent::Resized(_) => {
            place(&placed);
            let rects = LAST_REGION.lock().unwrap().clone();
            set_region(&placed, &rects);
        }
        // However the window went (destroyed on purpose, Alt+F4, a crash), the core
        // decides whether it needs a new one.
        WindowEvent::Destroyed => crate::webview_gone(&handle),
        _ => {}
    });
    window.show()?;
    Ok(window)
}

fn hwnd(window: &WebviewWindow) -> Option<HWND> {
    window.hwnd().ok().map(|h| HWND(h.0))
}

/// Centres the window on the primary monitor, touching its top edge.
pub fn place(window: &WebviewWindow) {
    let (Some(top), Ok(size)) = (monitor::primary_top_centre(), window.outer_size()) else {
        return;
    };
    let x = top.x - size.width as i32 / 2;
    let _ = window.set_position(PhysicalPosition::new(x, top.top));
}

/// A tool window stays out of Alt+Tab as well as the taskbar.
pub fn make_tool_window(window: &WebviewWindow) {
    let Some(hwnd) = hwnd(window) else { return };
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let style = (style | WS_EX_TOOLWINDOW.0 as isize) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style);
    }
}

/// Stops Windows painting a title bar into the window when it is activated. The
/// window has no frame, but Windows still repaints one on activation, and it shows
/// as white bars through the transparent area around the capsule.
/// Must be called on the thread that created the window.
pub fn hide_frame_painting(window: &WebviewWindow) {
    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        unsafe {
            match msg {
                // An lparam of -1 updates the active state without repainting the frame.
                WM_NCACTIVATE => DefWindowProcW(hwnd, msg, wparam, LPARAM(-1)),
                WM_NCPAINT => LRESULT(0),
                _ => DefSubclassProc(hwnd, msg, wparam, lparam),
            }
        }
    }
    let Some(hwnd) = hwnd(window) else { return };
    unsafe {
        let _ = SetWindowSubclass(hwnd, Some(proc), 1, 0);
    }
}

/// Clips the window to `rects` (CSS pixels): only they are drawn and clickable.
pub fn set_region(window: &WebviewWindow, rects: &[Rect]) {
    rects.clone_into(&mut LAST_REGION.lock().unwrap());
    let Some(hwnd) = hwnd(window) else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let px = |v: f64| (v * scale).round() as i32;
    unsafe {
        let region = CreateRectRgn(0, 0, 0, 0);
        for r in rects {
            let part = CreateRectRgn(px(r.x), px(r.y), px(r.x + r.w), px(r.y + r.h));
            CombineRgn(Some(region), Some(region), Some(part), RGN_OR);
            let _ = DeleteObject(part.into());
        }
        // On success the system owns the region.
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(region.into());
        }
    }
}

#[cfg(test)]
mod tests {
    /// The window width is written in three places; they must agree.
    #[test]
    fn window_width_matches_the_ui() {
        let root = env!("CARGO_MANIFEST_DIR");
        let read = |path: &str| std::fs::read_to_string(format!("{root}/{path}")).unwrap();
        let number_after = |text: &str, marker: &str| -> f64 {
            let at = text.find(marker).unwrap_or_else(|| panic!("no {marker}"));
            let digits: String = text[at + marker.len()..]
                .chars()
                .skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            digits.parse().unwrap()
        };
        let width = number_after(&read("tauri.conf.json"), "\"width\"");
        let geometry = read("../ui/src/capsule/geometry.ts");
        assert_eq!(width, number_after(&geometry, "WINDOW_WIDTH ="));
        let css = read("../ui/src/index.css");
        assert_eq!(width / 2.0, number_after(&css, "transform-origin:"));
    }
}
