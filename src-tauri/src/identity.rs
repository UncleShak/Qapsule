//! Diagnostic for the development package identity: reports whether the
//! process has an identity, the notification access status, whether
//! notification changes can be observed and whether file writes stay real.
//! It never reads or prints notification contents.

use std::path::Path;

use windows::ApplicationModel::Package;
use windows::Foundation::TypedEventHandler;
use windows::UI::Notifications::Management::{
    UserNotificationListener, UserNotificationListenerAccessStatus,
};
use windows::UI::Notifications::UserNotificationChangedEventArgs;

const PROBE: &str = ".identity-probe";

/// A multi-line, human-readable report.
pub fn check() -> String {
    let mut out = String::new();

    match Package::Current()
        .and_then(|p| p.Id())
        .and_then(|id| id.FamilyName())
    {
        Ok(name) => out.push_str(&format!("identity: {name}\n")),
        Err(_) => out.push_str("identity: no identity\n"),
    }

    match UserNotificationListener::Current() {
        Err(e) => out.push_str(&format!("listener: unavailable ({})\n", e.code())),
        Ok(listener) => {
            match listener.RequestAccessAsync().and_then(|op| op.join()) {
                Ok(status) => out.push_str(&format!("request: {}\n", access_name(status))),
                Err(e) => out.push_str(&format!("request: error ({})\n", e.code())),
            }
            match listener.GetAccessStatus() {
                Ok(status) => out.push_str(&format!("access: {}\n", access_name(status))),
                Err(e) => out.push_str(&format!("access: error ({})\n", e.code())),
            }
            let handler = TypedEventHandler::<
                UserNotificationListener,
                UserNotificationChangedEventArgs,
            >::new(|_, _| Ok(()));
            match listener.NotificationChanged(&handler) {
                Ok(token) => {
                    let _ = listener.RemoveNotificationChanged(token);
                    out.push_str("subscribe: ok\n");
                }
                Err(e) => out.push_str(&format!("subscribe: failed ({})\n", e.code())),
            }
        }
    }

    out.push_str(&write_probe());
    out.push_str(
        "expected when registered:\n\
         \x20 identity: a family name starting with Qynetik.Qapsule.Dev_\n\
         \x20 request/access: Allowed (you may have to accept a Windows prompt or allow it in Settings > Privacy > Notifications)\n\
         \x20 subscribe: ok\n\
         \x20 writes: real\n",
    );
    out
}

/// Writes a probe file into the config directory and checks that it did not
/// land in the package's virtualised store.
fn write_probe() -> String {
    let Some(dir) =
        qapsule_server::config::config_path().and_then(|p| p.parent().map(Path::to_path_buf))
    else {
        return "writes: unknown (no config path)\n".to_string();
    };
    let mut out = format!("config dir: {}\n", dir.display());
    let probe = dir.join(PROBE);
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&probe, b"probe")) {
        out.push_str(&format!("writes: unknown (probe failed: {e})\n"));
        return out;
    }
    let mut virtualised = None;
    if let (Ok(local), Ok(roaming)) = (std::env::var("LOCALAPPDATA"), std::env::var("APPDATA"))
        && let Ok(rel) = dir.strip_prefix(&roaming)
        && let Ok(entries) = std::fs::read_dir(Path::new(&local).join("Packages"))
    {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("Qynetik.Qapsule.Dev_")
            {
                let candidate = entry
                    .path()
                    .join("LocalCache")
                    .join("Roaming")
                    .join(rel)
                    .join(PROBE);
                if candidate.exists() {
                    virtualised = Some(candidate);
                }
            }
        }
    }
    match virtualised {
        Some(p) => {
            out.push_str(&format!("writes: VIRTUALISED ({})\n", p.display()));
            let _ = std::fs::remove_file(p);
        }
        None => out.push_str("writes: real\n"),
    }
    let _ = std::fs::remove_file(&probe);
    out
}

fn access_name(status: UserNotificationListenerAccessStatus) -> String {
    match status {
        UserNotificationListenerAccessStatus::Allowed => "Allowed".into(),
        UserNotificationListenerAccessStatus::Denied => "Denied".into(),
        UserNotificationListenerAccessStatus::Unspecified => "Unspecified".into(),
        other => format!("{}", other.0),
    }
}
