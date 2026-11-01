//! The Windows side of the theme provider: where the wallpaper is, what the accent is,
//! whether Wallpaper Engine runs, and a one-off screen capture. Everything here is a
//! single call that returns; nothing polls. Off Windows each function finds nothing,
//! so the rest of the provider builds and tests anywhere.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use crate::colour::Rgb;

use super::Event;

/// A running Wallpaper Engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeProcess {
    /// Its executable, if Windows let us ask.
    pub exe: Option<PathBuf>,
}

/// The capture is scaled to this many pixels on its long edge; it is only sampled.
pub const CAPTURE_LONG_EDGE: i32 = 128;

#[cfg(windows)]
pub use real::*;

#[cfg(not(windows))]
pub use stub::*;

#[cfg(not(windows))]
mod stub {
    use super::*;

    pub struct AccentWatch;

    pub fn init() {}
    pub fn desktop_wallpaper() -> Option<PathBuf> {
        None
    }
    pub fn windows_accent() -> Option<Rgb> {
        None
    }
    pub fn watch_accent(_events: Sender<Event>) -> Option<AccentWatch> {
        None
    }
    pub fn wallpaper_engine() -> Option<WeProcess> {
        None
    }
    pub fn capture_primary() -> Option<(Vec<u8>, usize, usize)> {
        None
    }
    pub fn desktop_visible() -> bool {
        false
    }
}

#[cfg(windows)]
mod real {
    use super::*;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::ViewManagement::{UIColorType, UISettings};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HALFTONE, ReleaseDC, SRCCOPY,
        SelectObject, SetStretchBltMode, StretchBlt,
    };
    use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetWindowRect, IsIconic,
        IsWindowVisible, SM_CXSCREEN, SM_CYSCREEN,
    };
    use windows::core::{BOOL, PCWSTR, PWSTR};

    /// Starts COM and WinRT on this thread (multi-threaded). Already started is fine.
    pub fn init() {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    }

    /// Takes a COM string the callee allocated, and frees it.
    unsafe fn take(text: PWSTR) -> String {
        let out = unsafe { text.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(text.0 as *const _)) };
        out
    }

    /// The image file of the primary monitor's wallpaper (`IDesktopWallpaper`). `None` if
    /// there is none (a solid colour) or Windows won't say.
    pub fn desktop_wallpaper() -> Option<PathBuf> {
        unsafe {
            let wallpaper: IDesktopWallpaper =
                CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL).ok()?;
            let count = wallpaper.GetMonitorDevicePathCount().ok()?;
            let mut chosen: Option<String> = None;
            for i in 0..count {
                let Ok(id) = wallpaper.GetMonitorDevicePathAt(i) else {
                    continue;
                };
                let id = take(id);
                let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                let on_primary = wallpaper
                    .GetMonitorRECT(PCWSTR(wide.as_ptr()))
                    .is_ok_and(|r| r.left == 0 && r.top == 0);
                if on_primary || chosen.is_none() {
                    chosen = Some(id);
                }
                if on_primary {
                    break;
                }
            }
            let id = chosen?;
            let wide: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
            let path = take(wallpaper.GetWallpaper(PCWSTR(wide.as_ptr())).ok()?);
            (!path.is_empty()).then(|| PathBuf::from(path))
        }
    }

    /// The accent colour Windows is using.
    pub fn windows_accent() -> Option<Rgb> {
        let color = UISettings::new()
            .ok()?
            .GetColorValue(UIColorType::Accent)
            .ok()?;
        Some(Rgb(color.R, color.G, color.B))
    }

    /// The `ColorValuesChanged` subscription. Dropping it unsubscribes.
    pub struct AccentWatch {
        settings: UISettings,
        token: i64,
    }

    impl Drop for AccentWatch {
        fn drop(&mut self) {
            let _ = self.settings.RemoveColorValuesChanged(self.token);
        }
    }

    /// Sends [`Event::Accent`] whenever the Windows colours change.
    pub fn watch_accent(events: Sender<Event>) -> Option<AccentWatch> {
        let settings = UISettings::new().ok()?;
        let handler =
            TypedEventHandler::<UISettings, windows::core::IInspectable>::new(move |_, _| {
                let _ = events.send(Event::Accent);
                Ok(())
            });
        let token = settings.ColorValuesChanged(&handler).ok()?;
        Some(AccentWatch { settings, token })
    }

    /// Whether the desktop itself is what the user sees on the primary monitor, so a capture
    /// would show the wallpaper. Two cheap checks, in order:
    /// 1. the foreground window's class is `Progman` or `WorkerW` (the desktop, or its
    ///    wallpaper host), or there is no foreground window;
    /// 2. otherwise no visible, un-minimised, un-cloaked top-level window (other than the
    ///    shell's own `Progman`, `WorkerW` and taskbar) covers the primary monitor.
    ///    Windows that merely overlap part of the screen do not count: a capture still
    ///    shows mostly wallpaper then, and the palette only samples 128 px.
    pub fn desktop_visible() -> bool {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.0.is_null() || is_shell_class(foreground) {
                return true;
            }
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            let mut covered = false;
            let _ = EnumWindows(
                Some(covers),
                LPARAM(&mut (sw, sh, &mut covered) as *mut _ as isize),
            );
            !covered
        }
    }

    unsafe fn is_shell_class(window: HWND) -> bool {
        let mut class = [0u16; 64];
        let len = unsafe { GetClassNameW(window, &mut class) } as usize;
        let class = String::from_utf16_lossy(&class[..len.min(class.len())]);
        matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd")
    }

    /// `EnumWindows` callback: sets `covered` if `window` fills the screen. Returns FALSE to stop.
    unsafe extern "system" fn covers(window: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let (sw, sh, covered) = &mut *(data.0 as *mut (i32, i32, &mut bool));
            if !IsWindowVisible(window).as_bool()
                || IsIconic(window).as_bool()
                || is_shell_class(window)
            {
                return BOOL(1);
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED,
                (&mut cloaked as *mut u32).cast(),
                size_of::<u32>() as u32,
            );
            let mut rect = RECT::default();
            if cloaked == 0
                && GetWindowRect(window, &mut rect).is_ok()
                && rect.left <= 0
                && rect.top <= 0
                && rect.right >= *sw
                && rect.bottom >= *sh
            {
                **covered = true;
                return BOOL(0);
            }
            BOOL(1)
        }
    }

    /// Wallpaper Engine's process, if one of its executables is running.
    pub fn wallpaper_engine() -> Option<WeProcess> {
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
            let mut entry = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut found = None;
            let mut next = Process32FirstW(snapshot, &mut entry).is_ok();
            while next {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
                if name.eq_ignore_ascii_case("wallpaper64.exe")
                    || name.eq_ignore_ascii_case("wallpaper32.exe")
                {
                    found = Some(entry.th32ProcessID);
                    break;
                }
                next = Process32NextW(snapshot, &mut entry).is_ok();
            }
            let _ = CloseHandle(snapshot);
            let pid = found?;
            Some(WeProcess {
                exe: image_path(pid),
            })
        }
    }

    unsafe fn image_path(pid: u32) -> Option<PathBuf> {
        unsafe {
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buffer = [0u16; 1024];
            let mut len = buffer.len() as u32;
            let result = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buffer.as_mut_ptr()),
                &mut len,
            );
            let _ = CloseHandle(process);
            result.ok()?;
            Some(PathBuf::from(String::from_utf16_lossy(
                &buffer[..len as usize],
            )))
        }
    }

    /// One capture of the primary monitor, scaled down to about [`CAPTURE_LONG_EDGE`]
    /// pixels: RGBA, width, height. Never called on a timer.
    pub fn capture_primary() -> Option<(Vec<u8>, usize, usize)> {
        unsafe {
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            if sw <= 0 || sh <= 0 {
                return None;
            }
            let scale = (f64::from(CAPTURE_LONG_EDGE) / f64::from(sw.max(sh))).min(1.0);
            let w = ((f64::from(sw) * scale).round() as i32).max(1);
            let h = ((f64::from(sh) * scale).round() as i32).max(1);

            let screen = GetDC(None);
            if screen.is_invalid() {
                return None;
            }
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, w, h);
            let mut pixels = None;
            if !memory.is_invalid() && !bitmap.is_invalid() {
                let old = SelectObject(memory, bitmap.into());
                // NULL (or HGDI_ERROR for regions) means the bitmap is not selected.
                if !old.is_invalid() && old.0 as isize != -1 {
                    SetStretchBltMode(memory, HALFTONE);
                    let drawn = StretchBlt(memory, 0, 0, w, h, Some(screen), 0, 0, sw, sh, SRCCOPY)
                        .as_bool();
                    // GetDIBits needs the bitmap out of every device context.
                    SelectObject(memory, old);
                    if drawn {
                        let mut info = BITMAPINFO {
                            bmiHeader: BITMAPINFOHEADER {
                                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                                biWidth: w,
                                biHeight: -h, // top-down
                                biPlanes: 1,
                                biBitCount: 32,
                                biCompression: BI_RGB.0,
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        let mut bgra = vec![0u8; (w * h * 4) as usize];
                        let lines = GetDIBits(
                            memory,
                            bitmap,
                            0,
                            h as u32,
                            Some(bgra.as_mut_ptr().cast()),
                            &mut info,
                            DIB_RGB_COLORS,
                        );
                        if lines == h {
                            for px in bgra.as_chunks_mut::<4>().0 {
                                px.swap(0, 2);
                                px[3] = 255;
                            }
                            pixels = Some((bgra, w as usize, h as usize));
                        }
                    }
                }
            }
            if !bitmap.is_invalid() {
                let _ = DeleteObject(bitmap.into());
            }
            if !memory.is_invalid() {
                let _ = DeleteDC(memory);
            }
            ReleaseDC(None::<HWND>, screen);
            pixels
        }
    }
}
