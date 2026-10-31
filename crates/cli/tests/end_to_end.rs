//! The real `qapsule` binary against a real pipe server, on a private pipe name.

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use qapsule_core::{Effect, Peek, PeekItem, PresenceEffect, Settings, UiState};
use qapsule_protocol::{ErrorCode, ServerMessage, decode_server};
use qapsule_server::{Engine, Listener, Shell};

#[derive(Clone, Default)]
struct Recorder {
    updates: Arc<Mutex<Vec<(Instant, UiState)>>>,
    effects: Arc<Mutex<Vec<Effect>>>,
}

impl Shell for Recorder {
    fn update(&mut self, state: UiState) {
        self.updates.lock().unwrap().push((Instant::now(), state));
    }
    fn run(&mut self, effect: Effect) {
        self.effects.lock().unwrap().push(effect);
    }
    fn presence(&mut self, _effect: PresenceEffect) {}
    fn main_screen(&mut self, _visible: bool) {}
}

struct Server {
    pipe: String,
    engine: Engine,
    recorder: Recorder,
    runtime: tokio::runtime::Runtime,
}

fn start() -> Server {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let pipe = format!(
        r"\\.\pipe\qapsule-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let recorder = Recorder::default();
    let engine = runtime.block_on(async {
        let engine = Engine::spawn(Settings::default(), recorder.clone());
        let listener = Listener::bind(&pipe).unwrap();
        tokio::spawn(listener.run(engine.clone()));
        engine
    });
    Server {
        pipe,
        engine,
        recorder,
        runtime,
    }
}

impl Server {
    fn qapsule(&self, args: &[&str]) -> Output {
        self.qapsule_with_stdin(args, "")
    }

    fn qapsule_with_stdin(&self, args: &[&str], stdin: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_qapsule"))
            .args(args)
            .env("QAPSULE_PIPE", &self.pipe)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    fn list(&self) -> String {
        let out = self.qapsule(&["list"]);
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    }

    fn raw(&self) -> (File, BufReader<File>) {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.pipe)
            .unwrap();
        let reader = BufReader::new(file.try_clone().unwrap());
        (file, reader)
    }

    fn snapshot(&self) -> UiState {
        self.runtime.block_on(self.engine.snapshot()).unwrap()
    }
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn read_reply(reader: &mut BufReader<File>) -> ServerMessage {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    decode_server(line.trim_end()).unwrap()
}

fn error_code(message: ServerMessage) -> ErrorCode {
    match message {
        ServerMessage::Error { code, .. } => code,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn push_list_done_and_clear() {
    let server = start();
    assert_eq!(server.list().trim(), "Nothing happening.");

    let out = server.qapsule(&[
        "push",
        "--id",
        "build",
        "--title",
        "cargo build",
        "--progress",
        "40%",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "", "push is quiet");
    let list = server.list();
    assert!(
        list.contains("running") && list.contains("cli/build") && list.contains("40%"),
        "{list}"
    );

    let out = server.qapsule(&["done", "--id", "build"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(server.list().starts_with("done"));

    server.qapsule(&["push", "--id", "x", "--source", "s", "--title", "x"]);
    let out = server.qapsule(&["clear", "--all"]);
    assert!(out.status.success());
    assert_eq!(server.list().trim(), "Nothing happening.");
}

#[test]
fn set_link_refuses_a_bad_link_before_touching_credential_manager() {
    let server = start();
    let out = server.qapsule_with_stdin(&["calendar", "set-link"], "http://secret.example/a.ics\n");
    assert!(!out.status.success());
    assert!(stderr(&out).contains("https://"), "{}", stderr(&out));
    assert!(!stderr(&out).contains("secret.example"));
    assert!(stdout(&out).is_empty());
}

#[test]
fn errors_come_back_as_messages_and_a_failing_exit_code() {
    let server = start();
    let out = server.qapsule(&["done", "--id", "nope"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("no activity `nope`"),
        "{}",
        stderr(&out)
    );

    let long = "x".repeat(81);
    let out = server.qapsule(&["push", "--id", "a", "--title", &long]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("at most 80 characters"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn pushing_to_a_reserved_source_is_refused() {
    let server = start();
    let out = server.qapsule(&[
        "push",
        "--source",
        "windows:Fake",
        "--id",
        "a",
        "--title",
        "hello",
    ]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("the windows: source prefix is reserved for Qapsule"),
        "{}",
        stderr(&out)
    );
    let out = server.qapsule(&[
        "push",
        "--source",
        "Windows:Fake",
        "--id",
        "a",
        "--title",
        "hi",
    ]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("is reserved for Qapsule"),
        "{}",
        stderr(&out)
    );
    assert_eq!(server.list().trim(), "Nothing happening.");
}

#[test]
fn the_cli_explains_when_qapsule_is_not_running() {
    let out = Command::new(env!("CARGO_BIN_EXE_qapsule"))
        .args(["list"])
        .env("QAPSULE_PIPE", r"\\.\pipe\qapsule-test-nobody-home")
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("Qapsule isn't running"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn wrap_reports_the_command_and_passes_on_its_exit_code() {
    let server = start();
    let out = server.qapsule(&["wrap", "--id", "w", "--", "cmd", "/c", "exit", "3"]);
    assert_eq!(out.status.code(), Some(3), "{}", stderr(&out));
    let list = server.list();
    assert!(
        list.starts_with("failed") && list.contains("wrap/w") && list.contains("exit code 3"),
        "{list}"
    );

    let out = server.qapsule(&["wrap", "--id", "ok", "--", "cmd", "/c", "echo", "hello"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(
        stdout(&out).contains("hello"),
        "the command's output passes through"
    );
    assert!(server.list().contains("done         wrap/ok"));
}

#[test]
fn wrap_still_runs_the_command_without_qapsule() {
    let out = Command::new(env!("CARGO_BIN_EXE_qapsule"))
        .args(["wrap", "--", "cmd", "/c", "exit", "4"])
        .env("QAPSULE_PIPE", r"\\.\pipe\qapsule-test-nobody-home")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
}

#[test]
fn callback_buttons_reach_the_waiting_client() {
    let server = start();
    let child = Command::new(env!("CARGO_BIN_EXE_qapsule"))
        .args([
            "push",
            "--id",
            "q",
            "--title",
            "Deploy?",
            "--state",
            "needs-input",
            "--callback",
            "yes:Deploy",
            "--wait",
        ])
        .env("QAPSULE_PIPE", &server.pipe)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    // Wait until the push has landed, then press the button as the UI would.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !server.list().contains("cli/q") {
        assert!(Instant::now() < deadline, "the push never arrived");
        thread::sleep(Duration::from_millis(20));
    }
    server.engine.press("cli".into(), "q".into(), "yes".into());
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "yes");
}

#[test]
fn focus_and_link_buttons_go_to_the_action_router() {
    let server = start();
    server.qapsule(&["push", "--id", "a", "--title", "x", "--focus-pid", "1234"]);
    server
        .engine
        .press("cli".into(), "a".into(), "focus".into());
    server.snapshot(); // The engine has handled the press once it answers this.
    assert_eq!(
        *server.recorder.effects.lock().unwrap(),
        [Effect::FocusWindow(1234)]
    );
}

#[test]
fn the_claude_code_hook_turns_events_into_activities() {
    let server = start();
    let event = |name: &str, extra: &str| {
        format!(
            r#"{{"session_id":"s1","cwd":"C:\\code\\qapsule","hook_event_name":"{name}"{extra}}}"#
        )
    };
    let out = server.qapsule_with_stdin(&["hook", "claude-code"], &event("UserPromptSubmit", ""));
    assert!(
        out.status.success() && out.stdout.is_empty() && out.stderr.is_empty(),
        "{out:?}"
    );
    assert!(
        server
            .list()
            .contains("running      claude-code/cc-s1  Claude is working · qapsule")
    );

    server.qapsule_with_stdin(
        &["hook", "claude-code"],
        &event("Notification", r#","message":"Claude needs your permission to use Bash","notification_type":"permission_prompt""#),
    );
    assert!(server.list().contains("needs input"));
    let pop = server.snapshot().pop.expect("needs input pops");
    assert_eq!(pop.title, "Claude needs you");
    assert!(pop.sticky);
    assert_eq!(pop.actions[0].kind, "focus_window");

    server.qapsule_with_stdin(&["hook", "claude-code"], &event("SessionEnd", ""));
    assert_eq!(server.list().trim(), "Nothing happening.");
}

#[test]
fn the_hook_is_silent_even_without_qapsule() {
    let out = Command::new(env!("CARGO_BIN_EXE_qapsule"))
        .args(["hook", "claude-code"])
        .env("QAPSULE_PIPE", r"\\.\pipe\qapsule-test-nobody-home")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
}

#[test]
fn bad_input_gets_an_error_and_the_connection_carries_on() {
    let server = start();
    let (mut writer, mut reader) = server.raw();
    writer.write_all(b"this is not json\n").unwrap();
    assert_eq!(error_code(read_reply(&mut reader)), ErrorCode::InvalidJson);
    writer.write_all(b"{\"v\":2,\"type\":\"list\"}\n").unwrap();
    assert_eq!(
        error_code(read_reply(&mut reader)),
        ErrorCode::UnsupportedVersion
    );
    let mut huge = vec![b'x'; 70 * 1024];
    huge.push(b'\n');
    writer.write_all(&huge).unwrap();
    assert_eq!(error_code(read_reply(&mut reader)), ErrorCode::TooLarge);
    writer.write_all(b"{\"v\":1,\"type\":\"hello\"}\n").unwrap();
    assert_eq!(read_reply(&mut reader), ServerMessage::Ok);
}

#[test]
fn more_than_twenty_messages_a_second_are_dropped() {
    let server = start();
    let (mut writer, mut reader) = server.raw();
    let burst = "{\"v\":1,\"type\":\"hello\"}\n".repeat(30);
    writer.write_all(burst.as_bytes()).unwrap();
    let replies: Vec<_> = (0..30).map(|_| read_reply(&mut reader)).collect();
    let ok = replies.iter().filter(|r| **r == ServerMessage::Ok).count();
    let limited = replies
        .iter()
        .filter(|r| {
            matches!(
                r,
                ServerMessage::Error {
                    code: ErrorCode::RateLimited,
                    ..
                }
            )
        })
        .count();
    assert_eq!((ok, limited), (20, 10));
}

#[test]
fn ui_updates_are_coalesced_however_fast_clients_push() {
    let server = start();
    let (mut writer, mut reader) = server.raw();
    let started = Instant::now();
    // 10 pushes a second for 2 seconds, like a progress bar.
    for i in 0..20 {
        let line = format!(
            "{{\"v\":1,\"type\":\"upsert\",\"activity\":{{\"id\":\"p\",\"source\":\"s\",\"title\":\"x\",\"state\":\"running\",\"progress\":{}}}}}\n",
            f64::from(i) / 20.0
        );
        writer.write_all(line.as_bytes()).unwrap();
        assert_eq!(read_reply(&mut reader), ServerMessage::Ok);
        thread::sleep(Duration::from_millis(100));
    }
    thread::sleep(Duration::from_millis(300));
    let updates = server.recorder.updates.lock().unwrap();
    let during: Vec<_> = updates.iter().filter(|(at, _)| *at >= started).collect();
    let seconds = started.elapsed().as_secs_f64();
    assert!(
        during.len() as f64 <= seconds * 4.0 + 1.0,
        "{} updates in {seconds:.1} s",
        during.len()
    );
    for pair in during.windows(2) {
        assert!(
            pair[1].0 - pair[0].0 >= Duration::from_millis(240),
            "two updates too close together"
        );
    }
    let Peek::One { main } = &during.last().unwrap().1.peek else {
        panic!()
    };
    let PeekItem::Activity(main) = main else {
        panic!("expected an activity")
    };
    assert_eq!(main.progress, Some(0.95), "the last value is never lost");
}

#[test]
fn only_one_qapsule_can_own_a_pipe_name() {
    let server = start();
    let second = server
        .runtime
        .block_on(async { Listener::bind(&server.pipe).map(|_| ()) });
    assert!(second.is_err());
}
