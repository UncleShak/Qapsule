//! The only WinRT code: listens for other apps' toasts and feeds the engine.
//! It never logs or formats notification text or app names; errors log HRESULTs.

use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use qapsule_server::Engine;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::Management::{
    UserNotificationListener, UserNotificationListenerAccessStatus,
};
use windows::UI::Notifications::{
    KnownNotificationBindings, UserNotification, UserNotificationChangedEventArgs,
    UserNotificationChangedKind,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, w};

use super::status::Status;
use super::toast::{ToastFields, to_activity};
use super::tracked::{Tracked, aumid_to_open};

pub struct Notifications {
    tracked: Arc<Mutex<Tracked>>,
    status: Arc<Mutex<Status>>,
}

impl Notifications {
    /// Starts listening on its own thread. Does nothing visible if Windows says no.
    /// `on_status` is called once with the outcome, from the listener thread (or
    /// from here if the thread can't start); it must not block for long.
    pub fn start(engine: Engine, on_status: impl Fn(Status) + Send + 'static) -> Notifications {
        let tracked = Arc::new(Mutex::new(Tracked::default()));
        let status = Arc::new(Mutex::new(Status::Starting));
        let on_status = Mutex::new(on_status);
        let report = Arc::new(move |s: Status| {
            if let Ok(f) = on_status.lock() {
                f(s);
            }
        });
        let report_in_thread = report.clone();
        let shared = tracked.clone();
        let shared_status = status.clone();
        let spawned = std::thread::Builder::new()
            .name("qapsule-notifications".into())
            .spawn(move || {
                let publish = |s: Status| {
                    if let Ok(mut current) = shared_status.lock() {
                        *current = s;
                    }
                    report_in_thread(s);
                };
                listen(engine, shared, publish)
            });
        if let Err(e) = spawned {
            eprintln!("qapsule: Windows notifications unavailable: thread ({e})");
            if let Ok(mut current) = status.lock() {
                *current = Status::Unavailable;
            }
            report(Status::Unavailable);
        }
        Notifications { tracked, status }
    }

    /// The latest status the listener reported.
    pub fn status(&self) -> Status {
        self.status
            .lock()
            .map(|s| *s)
            .unwrap_or(Status::Unavailable)
    }

    /// Opens Windows' notification privacy page, where access can be allowed.
    pub fn open_settings(&self) {
        open("ms-settings:privacy-notifications".to_string());
    }

    /// Runs a button the user pressed. Only `open` exists; anything else is ignored.
    pub fn act(&self, source: &str, id: &str, action: &str) {
        let aumid = match self.tracked.lock() {
            Ok(t) => aumid_to_open(&t, source, id, action),
            Err(_) => None,
        };
        let Some(aumid) = aumid else {
            return;
        };
        open(format!("shell:AppsFolder\\{aumid}"));
    }
}

/// Launches a shell target off the caller's thread, so the engine never waits on the shell.
fn open(target: String) {
    let _ = std::thread::Builder::new()
        .name("qapsule-open-app".into())
        .spawn(move || {
            let target = HSTRING::from(target);
            unsafe {
                let com =
                    CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).is_ok();
                ShellExecuteW(None, w!("open"), &target, None, None, SW_SHOWNORMAL);
                if com {
                    CoUninitialize();
                }
            }
        });
}

fn listen(engine: Engine, tracked: Arc<Mutex<Tracked>>, publish: impl Fn(Status)) {
    unsafe {
        // Already-initialised is fine; the result is not needed.
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
    }
    let unavailable = |reason: String| {
        eprintln!("qapsule: Windows notifications unavailable: {reason}");
        publish(Status::Unavailable);
    };
    let listener = match UserNotificationListener::Current() {
        Ok(l) => l,
        Err(e) => return unavailable(format!("{}", e.code())),
    };
    let mut status = match listener.GetAccessStatus() {
        Ok(s) => s,
        Err(e) => return unavailable(format!("{}", e.code())),
    };
    if status == UserNotificationListenerAccessStatus::Unspecified {
        status = match listener.RequestAccessAsync().and_then(|op| op.join()) {
            Ok(s) => s,
            Err(e) => return unavailable(format!("{}", e.code())),
        };
    }
    if status != UserNotificationListenerAccessStatus::Allowed {
        eprintln!(
            "qapsule: Windows notifications unavailable: access status {}",
            status.0
        );
        let denied = status == UserNotificationListenerAccessStatus::Denied
            || status == UserNotificationListenerAccessStatus::Unspecified;
        publish(if denied {
            Status::NeedsPermission
        } else {
            Status::Unavailable
        });
        return;
    }

    // The handler only queues events; everything else happens below, in order.
    let (tx, rx) = channel::<(UserNotificationChangedKind, u32)>();
    let handler =
        TypedEventHandler::<UserNotificationListener, UserNotificationChangedEventArgs>::new(
            move |_, args| {
                if let Some(args) = args.as_ref() {
                    queue(&tx, args);
                }
                Ok(())
            },
        );
    let token = match listener.NotificationChanged(&handler) {
        Ok(token) => token,
        Err(e) => return unavailable(format!("{}", e.code())),
    };
    publish(Status::On);
    // `listener`, `handler` and `token` stay alive for as long as this loop runs.
    for (kind, id) in rx {
        if let Err(e) = process(&engine, &tracked, &listener, kind, id) {
            eprintln!("qapsule: notification event failed ({})", e.code());
        }
    }
    let _ = listener.RemoveNotificationChanged(token);
    drop(handler);
}

fn queue(tx: &Sender<(UserNotificationChangedKind, u32)>, args: &UserNotificationChangedEventArgs) {
    match (args.ChangeKind(), args.UserNotificationId()) {
        (Ok(kind), Ok(id)) => {
            let _ = tx.send((kind, id));
        }
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("qapsule: notification event failed ({})", e.code());
        }
    }
}

/// The toast's text elements; empty if they can't be read.
fn read_texts(notification: &UserNotification) -> Vec<String> {
    let read = || -> windows::core::Result<Vec<String>> {
        let binding = notification
            .Notification()?
            .Visual()?
            .GetBinding(&KnownNotificationBindings::ToastGeneric()?)?;
        let mut texts = Vec::new();
        for element in binding.GetTextElements()? {
            if let Ok(text) = element.Text() {
                texts.push(text.to_string());
            }
        }
        Ok(texts)
    };
    read().unwrap_or_default()
}

fn process(
    engine: &Engine,
    tracked: &Mutex<Tracked>,
    listener: &UserNotificationListener,
    kind: UserNotificationChangedKind,
    id: u32,
) -> windows::core::Result<()> {
    match kind {
        UserNotificationChangedKind::Added => {
            let notification = listener.GetNotification(id)?;
            let (app_name, aumid) = match notification.AppInfo() {
                Ok(info) => (
                    info.DisplayInfo()
                        .and_then(|d| d.DisplayName())
                        .map(|n| n.to_string())
                        .unwrap_or_else(|_| "App".into()),
                    info.AppUserModelId().map(|a| a.to_string()).ok(),
                ),
                Err(_) => ("App".into(), None),
            };
            let fields = ToastFields {
                notification_id: id,
                app_name,
                aumid,
                texts: read_texts(&notification),
            };
            let activity = to_activity(&fields);
            if let Ok(mut t) = tracked.lock() {
                t.insert(activity.id.clone(), (activity.source.clone(), fields.aumid));
            }
            engine.provider_upsert(activity);
        }
        UserNotificationChangedKind::Removed => {
            // The app stays known (bounded by MAX_TRACKED, no text), so the
            // notification history can still open it after Windows removes the toast.
            let removed = tracked
                .lock()
                .ok()
                .and_then(|t| t.source_of(&id.to_string()));
            if let Some(source) = removed {
                engine.provider_remove(source, id.to_string());
            }
        }
        _ => {}
    }
    Ok(())
}
