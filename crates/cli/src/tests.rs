use clap::Parser;
use qapsule_protocol::{ActionKind, ClientMessage, Progress, State};

use super::*;

fn parse(args: &[&str]) -> Command {
    let mut all = vec!["qapsule"];
    all.extend_from_slice(args);
    Cli::try_parse_from(all)
        .unwrap_or_else(|e| panic!("{e}"))
        .command
}

fn push(args: &[&str]) -> PushArgs {
    let mut all = vec!["push"];
    all.extend_from_slice(args);
    match parse(&all) {
        Command::Push(args) => args,
        other => panic!("{other:?}"),
    }
}

#[test]
fn version_names_the_protocol() {
    let error = Cli::try_parse_from(["qapsule", "--version"]).unwrap_err();
    assert!(error.to_string().contains("(protocol v1)"), "{error}");
}

#[test]
fn push_builds_the_spec_example() {
    let args = push(&[
        "--id",
        "cc-4821",
        "--source",
        "claude-code",
        "--title",
        "Claude needs your input",
        "--state",
        "needs-input",
        "--focus-pid",
        "4821",
        "--dismiss",
    ]);
    let activity = args.activity(Some(1));
    assert_eq!(activity.state, State::NeedsInput);
    assert_eq!(activity.actions.len(), 2);
    assert_eq!(
        activity.actions[0].kind,
        ActionKind::FocusWindow { pid: 4821 }
    );
    assert!(activity.actions[0].primary);
    assert_eq!(activity.actions[1].kind, ActionKind::Dismiss);
    assert!(activity.validate().is_ok());
}

#[test]
fn push_defaults_to_a_running_cli_activity() {
    let activity = push(&["--id", "a", "--title", "Hello"]).activity(None);
    assert_eq!(activity.source, "cli");
    assert_eq!(activity.state, State::Running);
    assert!(activity.actions.is_empty());
}

#[test]
fn focus_parent_uses_the_calling_process() {
    let activity = push(&["--id", "a", "--title", "x", "--focus-parent"]).activity(Some(77));
    assert_eq!(
        activity.actions[0].kind,
        ActionKind::FocusWindow { pid: 77 }
    );
}

#[test]
fn progress_accepts_fractions_percentages_and_indeterminate() {
    let progress = |p: &str| push(&["--id", "a", "--title", "x", "--progress", p]).progress;
    assert_eq!(progress("0.25"), Some(Progress::Fraction(0.25)));
    assert_eq!(progress("40%"), Some(Progress::Fraction(0.4)));
    assert_eq!(progress("indeterminate"), Some(Progress::INDETERMINATE));
    for bad in ["2", "150%", "lots"] {
        assert!(
            Cli::try_parse_from([
                "qapsule",
                "push",
                "--id",
                "a",
                "--title",
                "x",
                "--progress",
                bad
            ])
            .is_err()
        );
    }
}

#[test]
fn callbacks_and_links_become_buttons() {
    let activity = push(&[
        "--id",
        "a",
        "--title",
        "x",
        "--url",
        "https://example.com",
        "--callback",
        "yes:Approve",
    ])
    .activity(None);
    assert_eq!(
        activity.actions[0].kind,
        ActionKind::OpenUrl {
            url: "https://example.com".into()
        }
    );
    assert!(activity.actions[0].primary);
    assert_eq!(activity.actions[1].id, "yes");
    assert_eq!(activity.actions[1].label, "Approve");
    assert!(!activity.actions[1].primary);
}

#[test]
fn done_and_fail_end_the_activity() {
    let Command::Fail(args) = parse(&[
        "fail",
        "--id",
        "b",
        "--source",
        "wrap",
        "--subtitle",
        "exit 2",
    ]) else {
        panic!()
    };
    assert_eq!(
        args.message(State::Failed),
        ClientMessage::End {
            source: "wrap".into(),
            id: "b".into(),
            state: State::Failed,
            title: None,
            subtitle: Some("exit 2".into())
        }
    );
}

#[test]
fn clear_needs_a_source_or_all() {
    assert!(Cli::try_parse_from(["qapsule", "clear"]).is_err());
    assert!(Cli::try_parse_from(["qapsule", "clear", "--id", "a"]).is_err());
    assert!(Cli::try_parse_from(["qapsule", "clear", "--all", "--source", "s"]).is_err());
    assert!(matches!(
        parse(&["clear", "--all"]),
        Command::Clear { all: true, .. }
    ));
    assert!(matches!(
        parse(&["clear", "--source", "s", "--id", "a"]),
        Command::Clear { .. }
    ));
}

#[test]
fn wrap_takes_the_rest_of_the_line() {
    let Command::Wrap(args) = parse(&["wrap", "--", "cargo", "build", "--release"]) else {
        panic!()
    };
    assert_eq!(args.command, ["cargo", "build", "--release"]);
    let Command::Wrap(args) = parse(&["wrap", "cargo", "test", "-q"]) else {
        panic!()
    };
    assert_eq!(args.command, ["cargo", "test", "-q"]);
}

#[test]
fn fit_shortens_with_an_ellipsis() {
    assert_eq!(fit("short", 10), "short");
    assert_eq!(fit("a longer line", 6), "a lon…");
    assert_eq!(fit("a longer line", 6).chars().count(), 6);
}

mod claude_code_hook {
    use super::*;
    use crate::hook::claude_code;

    fn event(name: &str, extra: &str) -> String {
        format!(
            r#"{{"session_id":"abc-123","cwd":"C:\\Users\\me\\qapsule","hook_event_name":"{name}"{extra}}}"#
        )
    }

    fn upserted(message: Option<ClientMessage>) -> qapsule_protocol::Activity {
        match message {
            Some(ClientMessage::Upsert { activity }) => {
                assert!(activity.validate().is_ok());
                activity
            }
            other => panic!("expected an upsert, got {other:?}"),
        }
    }

    #[test]
    fn prompt_submitted_means_running() {
        let a = upserted(claude_code(
            &event("UserPromptSubmit", r#","prompt":"hi""#),
            Some(9),
        ));
        assert_eq!(a.id, "cc-abc-123");
        assert_eq!(a.source, "claude-code");
        assert_eq!(a.state, State::Running);
        assert_eq!(a.progress, Some(Progress::INDETERMINATE));
        assert_eq!(a.subtitle.as_deref(), Some("qapsule"));
        assert_eq!(a.actions[0].kind, ActionKind::FocusWindow { pid: 9 });
    }

    #[test]
    fn a_finished_tool_means_running_again() {
        let a = upserted(claude_code(
            &event("PostToolUse", r#","tool_name":"Bash""#),
            None,
        ));
        assert_eq!(a.state, State::Running);
        assert_eq!(a.actions.len(), 1, "only Dismiss without a pid");
    }

    #[test]
    fn permission_prompts_need_input() {
        let a = upserted(claude_code(
            &event(
                "Notification",
                r#","message":"Claude needs your permission to use Bash","notification_type":"permission_prompt""#,
            ),
            Some(9),
        ));
        assert_eq!(a.state, State::NeedsInput);
        assert_eq!(a.title, "Claude needs you");
        assert_eq!(
            a.subtitle.as_deref(),
            Some("Claude needs your permission to use Bash · qapsule")
        );
        assert_eq!(a.progress, None);
    }

    #[test]
    fn other_notifications_are_ignored() {
        for kind in ["idle_prompt", "auth_success", "anything_new"] {
            let extra = format!(r#","message":"m","notification_type":"{kind}""#);
            assert_eq!(claude_code(&event("Notification", &extra), None), None);
        }
    }

    #[test]
    fn stop_succeeds_and_stop_failure_fails() {
        assert_eq!(
            upserted(claude_code(&event("Stop", ""), None)).state,
            State::Succeeded
        );
        assert_eq!(
            upserted(claude_code(&event("StopFailure", ""), None)).state,
            State::Failed
        );
    }

    #[test]
    fn session_end_clears_it() {
        assert_eq!(
            claude_code(&event("SessionEnd", r#","reason":"exit""#), None),
            Some(ClientMessage::Clear {
                source: Some("claude-code".into()),
                id: Some("cc-abc-123".into())
            })
        );
    }

    #[test]
    fn junk_and_unknown_events_are_ignored() {
        assert_eq!(claude_code("not json", None), None);
        assert_eq!(claude_code("{}", None), None);
        assert_eq!(claude_code(&event("PreCompact", ""), None), None);
    }

    #[test]
    fn long_messages_are_shortened() {
        let long = "x".repeat(500);
        let extra = format!(r#","message":"{long}","notification_type":"permission_prompt""#);
        let a = upserted(claude_code(&event("Notification", &extra), None));
        assert_eq!(a.subtitle.unwrap().chars().count(), 120);
    }
}
