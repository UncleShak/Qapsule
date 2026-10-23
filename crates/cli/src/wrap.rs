//! `qapsule wrap -- <command>`: runs a command, shown as running and then done
//! or failed. The command always runs, whether or not Qapsule is.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use qapsule_protocol::limits::{MAX_SUBTITLE_CHARS, MAX_TITLE_CHARS};
use qapsule_protocol::{Activity, ClientMessage, Progress, State};
use windows::Win32::System::Console::SetConsoleCtrlHandler;
use windows::core::BOOL;

use crate::client::Client;
use crate::{WrapArgs, dismiss_action, fit, focus_action, parent_pid};

/// Runs the command and returns its exit code.
pub fn run(args: &WrapArgs) -> i32 {
    let id = args
        .id
        .clone()
        .unwrap_or_else(|| format!("wrap-{}", std::process::id()));
    let command_line = args.command.join(" ");
    let title = fit(
        args.title.as_deref().unwrap_or(&command_line),
        MAX_TITLE_CHARS,
    );

    // Ctrl+C reaches the command too; stay alive long enough to report how it ended.
    // A handler function (unlike ignoring the signal) is not inherited by the command.
    unsafe extern "system" fn ignore(_: u32) -> BOOL {
        true.into()
    }
    unsafe {
        let _ = SetConsoleCtrlHandler(Some(ignore), true);
    }

    let mut actions = Vec::new();
    if let Some(pid) = parent_pid() {
        actions.push(focus_action(pid, "Open terminal"));
    }
    actions.push(dismiss_action());
    let mut client = Client::connect().ok();
    let mut send = |message: ClientMessage| {
        if let Some(c) = client.as_mut()
            && c.request(&message).is_err()
        {
            client = None;
        }
    };
    send(ClientMessage::Upsert {
        activity: Activity {
            id: id.clone(),
            source: args.source.clone(),
            title,
            subtitle: None,
            state: State::Running,
            progress: Some(Progress::INDETERMINATE),
            icon: Some("build".into()),
            // Long builds outlive the default hour.
            expires_in: Some(qapsule_protocol::limits::MAX_EXPIRES_IN_SECS),
            actions,
        },
    });

    let started = Instant::now();
    let program = resolve(&args.command[0]);
    let status = Command::new(&program).args(&args.command[1..]).status();
    let took = describe(started.elapsed());
    let (code, state, subtitle) = match status {
        Ok(status) => {
            let code = status.code().unwrap_or(1);
            let state = if status.success() {
                State::Succeeded
            } else {
                State::Failed
            };
            (code, state, format!("exit code {code} · {took}"))
        }
        Err(e) => {
            eprintln!("qapsule: couldn't run {}: {e}", args.command[0]);
            (1, State::Failed, format!("couldn't start: {e}"))
        }
    };
    send(ClientMessage::End {
        source: args.source.clone(),
        id,
        state,
        title: None,
        subtitle: Some(fit(&subtitle, MAX_SUBTITLE_CHARS)),
    });
    code
}

/// Finds a program on PATH, trying each PATHEXT extension, so `npm` finds `npm.cmd`.
fn resolve(program: &str) -> PathBuf {
    let path = Path::new(program);
    if path.extension().is_some() || path.components().count() > 1 {
        return path.to_path_buf();
    }
    let extensions = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
    let dirs = env::var_os("PATH")
        .map(|p| env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    for dir in dirs {
        for ext in extensions.split(';').filter(|e| !e.is_empty()) {
            let candidate = dir.join(format!("{program}{}", ext.to_ascii_lowercase()));
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    path.to_path_buf()
}

/// `45 s`, `3 min 12 s`, `1 h 4 min`.
fn describe(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{s} s"),
        60..3600 => format!("{} min {} s", s / 60, s % 60),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(describe(Duration::from_secs(45)), "45 s");
        assert_eq!(describe(Duration::from_secs(192)), "3 min 12 s");
        assert_eq!(describe(Duration::from_secs(3840)), "1 h 4 min");
    }

    #[test]
    fn finds_programs_with_any_windows_extension() {
        let found = resolve("cmd");
        assert!(
            found
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("cmd.exe"),
            "{found:?}"
        );
        assert_eq!(resolve("C:/x/y.exe"), PathBuf::from("C:/x/y.exe"));
    }
}
