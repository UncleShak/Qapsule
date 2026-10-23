//! A hidden window on its own thread that Windows notifies about the hotkey,
//! fullscreen apps and display changes. Purely event-driven: no polling.

use std::sync::OnceLock;
use std::thread;

use qapsule_server::Engine;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, VK_SPACE,
};
use windows::Win32::UI::Shell::{ABM_NEW, ABN_FULLSCREENAPP, APPBARDATA, SHAppBarMessage};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClassNameW, GetForegroundWindow,
    GetMessageW, GetWindowRect, MSG, RegisterClassW, TranslateMessage, WM_APP, WM_DISPLAYCHANGE,
    WM_DPICHANGED, WM_HOTKEY, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};
use windows::core::w;

const WM_APPBAR: u32 = WM_APP + 1;
const HOTKEY_ID: i32 = 1;

struct Context {
    engine: Engine,
    on_display_change: Box<dyn Fn() + Send + Sync>,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

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
            WM_HOTKEY if wparam.0 as i32 == HOTKEY_ID => context.engine.toggle_pinned(),
            WM_APPBAR if wparam.0 as u32 == ABN_FULLSCREENAPP => {
                let opening = lparam.0 != 0;
                context
                    .engine
                    .fullscreen(opening && foreground_is_fullscreen());
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => (context.on_display_change)(),
            _ => {}
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Windows also reports the desktop itself as "fullscreen" (for example after Win+D).
fn foreground_is_fullscreen() -> bool {
    unsafe {
        let window = GetForegroundWindow();
        if window.is_invalid() {
            return false;
        }
        let mut class = [0u16; 32];
        let len = GetClassNameW(window, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..len]);
        if class == "Progman" || class == "WorkerW" {
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
        rect.left <= m.left && rect.top <= m.top && rect.right >= m.right && rect.bottom >= m.bottom
    }
}
