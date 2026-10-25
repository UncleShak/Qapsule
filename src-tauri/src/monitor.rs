//! Where the top edge of the primary monitor is. The capsule window and the edge
//! strip both sit there, so they ask in one place.

use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};

/// The top centre of the primary monitor, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct TopCentre {
    /// Horizontal centre of the monitor.
    pub x: i32,
    pub top: i32,
    /// Display scale: 1.0 at 100%.
    pub scale: f64,
}

pub fn primary_top_centre() -> Option<TopCentre> {
    unsafe {
        let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return None;
        }
        let (mut dpi_x, mut dpi_y) = (96, 96);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_err() {
            dpi_x = 96;
        }
        let m = info.rcMonitor;
        Some(TopCentre {
            x: m.left + (m.right - m.left) / 2,
            top: m.top,
            scale: f64::from(dpi_x.max(96)) / 96.0,
        })
    }
}
