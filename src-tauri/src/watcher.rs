//! A hidden window on its own thread that Windows notifies about the hotkey,
//! fullscreen apps and display changes, plus the edge strip: a tiny, nearly
//! transparent native window at the top centre of the screen that brings a tucked
//! capsule out on hover, and works while the webview doesn't exist.
//! Purely event-driven: no polling.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::thread;

use qapsule_server::Engine;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, TME_LEAVE, TRACKMOUSEEVENT,
    TrackMouseEvent, VK_SPACE,
};
use windows::Win32::UI::Shell::{ABM_NEW, ABN_FULLSCREENAPP, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClassNameW, GetForegroundWindow,
    GetMessageW, GetWindowRect, HWND_TOPMOST, IDC_ARROW, IsZoomed, KillTimer, LWA_ALPHA,
    LoadCursorW, MA_NOACTIVATE, MSG, PostMessageW, RegisterClassW, SWP_HIDEWINDOW, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SetLayeredWindowAttributes, SetTimer, SetWindowPos, TranslateMessage, WM_APP,
    WM_DISPLAYCHANGE, WM_DPICHANGED, WM_HOTKEY, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_TIMER,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::w;

use crate::monitor;

const WM_APPBAR: u32 = WM_APP + 1;
/// Asks the strip to show or hide itself to match `STRIP_WANTED`.
const WM_STRIP: u32 = WM_APP + 2;
const HOTKEY_ID: i32 = 1;
/// One-shot timer that checks the foreground window again after a fullscreen app opens.
const FULLSCREEN_RECHECK_ID: usize = 1;
/// Windows announces a fullscreen app before the window has finished growing to fill
/// the monitor (Firefox-based browsers lag by about 300 ms), so look again after this.
const FULLSCREEN_RECHECK_MS: u32 = 500;
/// From WinUser.h; the `windows` crate only exposes it with the Controls feature.
const WM_MOUSELEAVE: u32 = 0x02A3;
/// The strip's size in logical pixels.
const STRIP_WIDTH: f64 = 120.0;
const STRIP_HEIGHT: f64 = 3.0;

/// The strip window, once it exists.
static STRIP: AtomicIsize = AtomicIsize::new(0);
/// Whether the strip should be on screen: true while the capsule is tucked away.
static STRIP_WANTED: AtomicBool = AtomicBool::new(false);
/// The pointer is over the strip, and Windows will say when it leaves.
static STRIP_TRACKING: AtomicBool = AtomicBool::new(false);

struct Context {
    engine: Engine,
    on_display_change: Box<dyn Fn() + Send + Sync>,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

/// Shows the edge strip while the capsule is tucked away, and hides it while it's out.
/// Safe to call from any thread, before or after the watcher has started.
pub fn set_edge_strip(visible: bool) {
    STRIP_WANTED.store(visible, Ordering::SeqCst);
    let strip = STRIP.load(Ordering::SeqCst);
    if strip != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(strip as _)), WM_STRIP, WPARAM(0), LPARAM(0));
        }
    }
}

/// Starts the watcher. Call once.
pub fn spawn(engine: Engine, on_display_change: impl Fn() + Send + Sync + 'static) {
    let context = Context {
        engine,
        on_display_change: Box::new(on_display_change),
    };
    if CONTEXT.set(context).is_err() {
        return;
    }
    thread::Builder::new()
        .name("qapsule-watcher".into())
        .spawn(|| unsafe { run() })
        .expect("starting the watcher thread");
}

unsafe fn run() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let class = w!("QapsuleWatcher");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        // A hidden top-level window: message-only windows miss the broadcasts we need.
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("Qapsule watcher"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            eprintln!("qapsule: couldn't create the watcher window");
            return;
        };

        // Registering as an app bar (without reserving space) brings ABN_FULLSCREENAPP.
        let mut appbar = APPBARDATA {
            cbSize: size_of::<APPBARDATA>() as u32,
            hWnd: hwnd,
            uCallbackMessage: WM_APPBAR,
            ..Default::default()
        };
        SHAppBarMessage(ABM_NEW, &mut appbar);

        if RegisterHotKey(
            Some(hwnd),
            HOTKEY_ID,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            u32::from(VK_SPACE.0),
        )
        .is_err()
        {
            eprintln!("qapsule: Ctrl+Alt+Space is taken by another app; the hotkey is off");
        }

        let strip_class = w!("QapsuleEdgeStrip");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(strip_proc),
            hInstance: instance.into(),
            lpszClassName: strip_class,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        });
        match CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            strip_class,
            w!("Qapsule edge"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        ) {
            Ok(strip) => {
                // Alpha 1 of 255: invisible, but still receives the mouse (alpha 0 would not).
                let _ = SetLayeredWindowAttributes(strip, COLORREF(0), 1, LWA_ALPHA);
                STRIP.store(strip.0 as isize, Ordering::SeqCst);
                update_strip(strip);
            }
            Err(e) => eprintln!("qapsule: couldn't create the edge strip: {e}"),
        }

        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(context) = CONTEXT.get() {
        match msg {
            WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => context.engine.toggle_expanded(),
            WM_APPBAR if wparam.0 as u32 == ABN_FULLSCREENAPP => {
                let opening = lparam.0 != 0;
                unsafe {
                    if opening {
                        SetTimer(
                            Some(hwnd),
                            FULLSCREEN_RECHECK_ID,
                            FULLSCREEN_RECHECK_MS,
                            None,
                        );
                    } else {
                        let _ = KillTimer(Some(hwnd), FULLSCREEN_RECHECK_ID);
                    }
                }
                context
                    .engine
                    .fullscreen(opening && foreground_is_fullscreen());
            }
            WM_TIMER if wparam.0 == FULLSCREEN_RECHECK_ID => {
                unsafe {
                    let _ = KillTimer(Some(hwnd), FULLSCREEN_RECHECK_ID);
                }
                context.engine.fullscreen(foreground_is_fullscreen());
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => {
                (context.on_display_change)();
                let strip = STRIP.load(Ordering::SeqCst);
                if strip != 0 {
                    unsafe { update_strip(HWND(strip as _)) };
                }
            }
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Puts the strip at the top centre of the primary monitor, shown or hidden as wanted.
unsafe fn update_strip(strip: HWND) {
    let Some(top) = monitor::primary_top_centre() else {
        return;
    };
    let (w, h) = (
        (STRIP_WIDTH * top.scale) as i32,
        (STRIP_HEIGHT * top.scale).ceil() as i32,
    );
    let show = if STRIP_WANTED.load(Ordering::SeqCst) {
        SWP_SHOWWINDOW
    } else {
        SWP_HIDEWINDOW
    };
    unsafe {
        let _ = SetWindowPos(
            strip,
            Some(HWND_TOPMOST),
            top.x - w / 2,
            top.top,
            w,
            h,
            show | SWP_NOACTIVATE,
        );
    }
}

unsafe extern "system" fn strip_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_STRIP => unsafe { update_strip(hwnd) },
        WM_MOUSEMOVE => {
            if !STRIP_TRACKING.swap(true, Ordering::SeqCst) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    let _ = TrackMouseEvent(&mut track);
                }
                if let Some(context) = CONTEXT.get() {
                    context.engine.edge_hover();
                }
            }
        }
        WM_MOUSELEAVE => STRIP_TRACKING.store(false, Ordering::SeqCst),
        // Touching the strip must never take focus from the app being used.
        WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Windows also reports the desktop itself as "fullscreen" (for example after Win+D),
/// and a maximized window when the taskbar auto-hides. Neither should hide the capsule.
/// A maximized window overhangs the monitor by its resize border, while a fullscreen
/// one fits it exactly, even when Windows still calls it maximized (Firefox and Zen do).
fn foreground_is_fullscreen() -> bool {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return false;
        }
        let mut class = [0u16; 32];
        let len = GetClassNameW(window, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len]);
        // The desktop, and the Alt+Tab switcher that briefly fills the screen.
        if matches!(
            class.as_str(),
            "Progman" | "WorkerW" | "XamlExplorerHostIslandWindow"
        ) {
            return false;
        }
        let mut rect = RECT::default();
        if GetWindowRect(window, &mut rect).is_err() {
            return false;
        }
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        let covers = rect.left <= m.left
            && rect.top <= m.top
            && rect.right >= m.right
            && rect.bottom >= m.bottom;
        let overhangs = rect.left < m.left
            || rect.top < m.top
            || rect.right > m.right
            || rect.bottom > m.bottom;
        covers && !(IsZoomed(window).as_bool() && overhangs)
    }
}
