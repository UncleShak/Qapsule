//! Spike: peek window placement and backdrop material (Mica or Acrylic).

use tauri::{PhysicalPosition, WebviewWindow};

/// Backdrop material, chosen with `QAPSULE_BACKDROP=mica|acrylic|blur` (default Mica).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    Mica,
    Acrylic,
    Blur,
}

impl Backdrop {
    fn from_env() -> Self {
        match std::env::var("QAPSULE_BACKDROP").as_deref() {
            Ok("acrylic") => Self::Acrylic,
            Ok("blur") => Self::Blur,
            _ => Self::Mica,
        }
    }
}

/// Places the window top-centre on the primary monitor, applies the backdrop and shows it.
pub fn setup(window: &WebviewWindow) -> tauri::Result<()> {
    place_top_centre(window)?;
    apply_backdrop(window, Backdrop::from_env());
    window.show()
}

/// Positions in physical pixels so fractional scaling can't leave a gap at the top edge.
fn place_top_centre(window: &WebviewWindow) -> tauri::Result<()> {
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(());
    };
    let mon_pos = monitor.position();
    let mon_size = monitor.size();
    let win_size = window.outer_size()?;
    let x = centre_x(mon_pos.x, mon_size.width, win_size.width);
    // The monitor's top edge, not the work area, so we sit above a top-docked taskbar.
    window.set_position(PhysicalPosition::new(x, mon_pos.y))
}

fn centre_x(mon_x: i32, mon_width: u32, win_width: u32) -> i32 {
    mon_x + (mon_width.saturating_sub(win_width) / 2) as i32
}

#[cfg(windows)]
fn apply_backdrop(window: &WebviewWindow, backdrop: Backdrop) {
    let result = match backdrop {
        Backdrop::Mica => window_vibrancy::apply_mica(window, Some(true)),
        // The tint only applies on Windows 10; Windows 11 22H2+ uses the system
        // backdrop and ignores it, so darkening is done in the UI instead.
        Backdrop::Acrylic => window_vibrancy::apply_acrylic(window, Some((18, 18, 18, 160))),
        // Uses the older composition API, which stays on when the window loses focus.
        // Renders solid black on Windows 11 build 26200, so not usable as-is.
        Backdrop::Blur => window_vibrancy::apply_blur(window, None),
    };
    if let Err(err) = result {
        eprintln!("failed to apply {backdrop:?} backdrop: {err}");
    }
}

#[cfg(not(windows))]
fn apply_backdrop(_window: &WebviewWindow, _backdrop: Backdrop) {}

#[cfg(test)]
mod tests {
    use super::centre_x;

    #[test]
    fn centres_on_primary_monitor() {
        // 1920 wide at 150%: a 200 logical window is 300 physical.
        assert_eq!(centre_x(0, 1920, 300), 810);
    }

    #[test]
    fn odd_leftover_rounds_down() {
        assert_eq!(centre_x(0, 2561, 300), 1130);
    }

    #[test]
    fn respects_negative_monitor_origin() {
        assert_eq!(centre_x(-2560, 2560, 300), -1430);
    }

    #[test]
    fn window_wider_than_monitor_sits_at_origin() {
        assert_eq!(centre_x(100, 200, 300), 100);
    }
}
