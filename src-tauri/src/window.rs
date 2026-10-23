//! The one capsule window: flush with the top edge, out of the taskbar and
//! Alt+Tab, and clipped to a region so clicks outside the pill pass through.

use serde::Deserialize;
use tauri::{PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WM_NCACTIVATE, WM_NCPAINT,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

/// A rectangle in CSS pixels, relative to the window.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn hwnd(window: &WebviewWindow) -> Option<HWND> {
    window.hwnd().ok().map(|h| HWND(h.0))
}

/// Centres the window on the primary monitor, touching its top edge.
pub fn place(window: &WebviewWindow) {
    let (Ok(Some(monitor)), Ok(size)) = (window.primary_monitor(), window.outer_size()) else {
        return;
    };
    let origin = monitor.position();
    let width = monitor.size().width as i32;
    let x = origin.x + (width - size.width as i32) / 2;
    let _ = window.set_position(PhysicalPosition::new(x, origin.y));
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

/// Clips the window to `rects`: only they are drawn and clickable.
pub fn set_region(window: &WebviewWindow, rects: &[Rect]) {
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
