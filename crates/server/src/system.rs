//! The action router's Windows side: opening links, bringing windows forward,
//! and reading the local time.

use std::collections::HashMap;
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow, GetWindow,
    GetWindowLongW, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
    SW_RESTORE, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, HSTRING, w};

/// Opens an http or https link in the default browser. Anything else is ignored.
pub fn open_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &HSTRING::from(url),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW reports success as a value above 32.
    result.0 as usize > 32
}

/// Walks from `start` up its parent chain to the first process that has a window.
/// Terminals host the shell in a separate process, so the window is usually a few steps up.
pub fn find_window_owner(
    start: u32,
    parent_of: impl Fn(u32) -> Option<u32>,
    has_window: impl Fn(u32) -> bool,
) -> Option<u32> {
    let mut pid = start;
    let mut seen = Vec::new();
    for _ in 0..32 {
        if has_window(pid) {
            return Some(pid);
        }
        seen.push(pid);
        pid = parent_of(pid).filter(|p| *p != 0 && !seen.contains(p))?;
    }
    None
}

/// The process owning the window for `pid`: itself or its nearest ancestor with a window.
///
/// Resolve this while the pushing client is still connected. Hooks run in short-lived
/// shells, and once one exits, the chain from it up to the terminal is broken.
pub fn window_owner(pid: u32) -> Option<u32> {
    let parents = process_parents();
    let windows = top_level_windows();
    find_window_owner(
        pid,
        |p| parents.get(&p).copied(),
        |p| windows.contains_key(&p),
    )
}

/// Brings forward the window of `pid` or its nearest ancestor that has one.
pub fn focus_process_window(pid: u32) -> bool {
    let windows = top_level_windows();
    let owner = if windows.contains_key(&pid) {
        Some(pid)
    } else {
        window_owner(pid)
    };
    let Some(hwnd) = owner.and_then(|o| windows.get(&o).copied()) else {
        return false;
    };
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        // Windows only lets the thread that has the foreground hand it on. This runs on an
        // engine thread after the user's click, so borrow the foreground thread's input
        // state for the moment of the switch.
        let me = GetCurrentThreadId();
        let foreground = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let attached = foreground != 0
            && foreground != me
            && AttachThreadInput(me, foreground, true).as_bool();
        let focused = SetForegroundWindow(hwnd).as_bool();
        let _ = BringWindowToTop(hwnd);
        if attached {
            let _ = AttachThreadInput(me, foreground, false);
        }
        focused
    }
}

/// Every running process mapped to its parent.
fn process_parents() -> HashMap<u32, u32> {
    let mut parents = HashMap::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return parents;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    parents
}

/// The first visible, titled, unowned app window of each process.
fn top_level_windows() -> HashMap<u32, HWND> {
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        unsafe {
            let found = &mut *(lparam.0 as *mut HashMap<u32, HWND>);
            let tool = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0;
            let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid());
            if IsWindowVisible(hwnd).as_bool() && !tool && !owned && GetWindowTextLengthW(hwnd) > 0
            {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                found.entry(pid).or_insert(hwnd);
            }
        }
        true.into()
    }
    let mut found: HashMap<u32, HWND> = HashMap::new();
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut found as *mut _ as isize));
    }
    found
}

/// The local time as `HH:MM`, and how long until the minute changes.
pub fn local_clock() -> (String, Duration) {
    let t = unsafe { GetLocalTime() };
    let into_minute = u64::from(t.wSecond) * 1000 + u64::from(t.wMilliseconds);
    (
        format!("{:02}:{:02}", t.wHour, t.wMinute),
        Duration::from_millis(60_000u64.saturating_sub(into_minute).max(1)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_up_to_the_first_process_with_a_window() {
        // hook (40) → claude (30) → shell (20) → terminal (10, has the window)
        let parent = |p| match p {
            40 => Some(30),
            30 => Some(20),
            20 => Some(10),
            10 => Some(1),
            _ => None,
        };
        assert_eq!(find_window_owner(40, parent, |p| p == 10), Some(10));
        assert_eq!(find_window_owner(10, parent, |p| p == 10), Some(10));
        assert_eq!(find_window_owner(40, parent, |_| false), None);
    }

    #[test]
    fn stops_on_loops_and_pid_zero() {
        assert_eq!(
            find_window_owner(5, |p| Some(if p == 5 { 6 } else { 5 }), |_| false),
            None
        );
        assert_eq!(find_window_owner(5, |_| Some(0), |_| false), None);
    }

    #[test]
    fn this_process_has_a_parent_chain() {
        let parents = process_parents();
        assert!(parents.contains_key(&std::process::id()));
    }

    #[test]
    fn a_process_without_a_window_resolves_to_an_ancestor() {
        // Tests run under cargo, under a shell, usually under a terminal or IDE with a window.
        // On a headless CI runner there may be none, which is also a valid answer.
        if let Some(owner) = window_owner(std::process::id()) {
            assert_ne!(owner, std::process::id());
        }
    }

    #[test]
    fn refuses_non_web_links() {
        assert!(!open_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!open_url("ms-settings:display"));
    }

    #[test]
    fn clock_is_hours_and_minutes() {
        let (text, until) = local_clock();
        assert_eq!(text.len(), 5);
        assert_eq!(&text[2..3], ":");
        assert!(until <= Duration::from_secs(60));
    }
}
