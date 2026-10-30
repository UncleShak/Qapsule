use std::fs;
use std::path::Path;

use super::limits::*;
use super::*;

fn samples(dir: &str) -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("samples")
        .join(dir);
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| {
            let text = fs::read_to_string(&path).unwrap();
            (path.display().to_string(), text.trim_end().to_string())
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no samples in {}", dir.display());
    files
}

fn activity() -> Activity {
    Activity {
        id: "a".into(),
        source: "s".into(),
        title: "Working".into(),
        subtitle: None,
        state: State::Running,
        progress: None,
        icon: None,
        expires_in: None,
        actions: vec![],
    }
}

#[test]
fn protocol_version_is_v1() {
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn every_valid_sample_decodes_and_round_trips() {
    for (path, text) in samples("valid") {
        let message = decode_client(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let again = decode_client(&encode(&message)).unwrap();
        assert_eq!(message, again, "{path} changed in a round trip");
    }
}

#[test]
fn every_invalid_sample_fails_with_its_folder_code() {
    for code in ["invalid_json", "invalid_message", "unsupported_version"] {
        let expected: ErrorCode = serde_json::from_value(code.into()).unwrap();
        for (path, text) in samples(&format!("invalid/{code}")) {
            match decode_client(&text) {
                Ok(m) => panic!("{path} should be refused but decoded to {m:?}"),
                Err(e) => assert_eq!(e.code, expected, "{path}: {}", e.message),
            }
        }
    }
}

#[test]
fn the_spec_example_decodes_to_the_expected_activity() {
    let text = r#"{"v":1,"type":"upsert","activity":{"id":"cc-4821","source":"claude-code","title":"Claude needs your input","state":"needs_input","actions":[{"id":"go","label":"Open terminal","kind":"focus_window","pid":4821,"primary":true},{"id":"x","label":"Dismiss","kind":"dismiss"}]}}"#;
    let ClientMessage::Upsert { activity } = decode_client(text).unwrap() else {
        panic!("not an upsert");
    };
    assert_eq!(activity.state, State::NeedsInput);
    assert_eq!(
        activity.actions[0].kind,
        ActionKind::FocusWindow { pid: 4821 }
    );
    assert!(activity.actions[0].primary);
    assert_eq!(activity.actions[1].kind, ActionKind::Dismiss);
    assert!(!activity.actions[1].primary);
}

#[test]
fn encoding_puts_the_version_first_and_omits_empty_fields() {
    let line = encode(&ClientMessage::Upsert {
        activity: activity(),
    });
    assert_eq!(
        line,
        r#"{"v":1,"type":"upsert","activity":{"id":"a","source":"s","title":"Working","state":"running"}}"#
    );
    assert_eq!(encode(&ServerMessage::Ok), r#"{"v":1,"type":"ok"}"#);
}

#[test]
fn progress_is_a_number_or_indeterminate() {
    let mut a = activity();
    a.progress = Some(Progress::Fraction(0.5));
    assert!(encode(&a).contains(r#""progress":0.5"#));
    a.progress = Some(Progress::INDETERMINATE);
    assert!(encode(&a).contains(r#""progress":"indeterminate""#));
    let parsed: Progress = serde_json::from_str(r#""indeterminate""#).unwrap();
    assert_eq!(parsed, Progress::INDETERMINATE);
    assert!(serde_json::from_str::<Progress>(r#""halfway""#).is_err());
}

#[test]
fn server_messages_round_trip() {
    let messages = [
        ServerMessage::Ok,
        ServerMessage::Error {
            code: ErrorCode::RateLimited,
            message: "slow down".into(),
        },
        ServerMessage::ListResult {
            activities: vec![activity()],
        },
        ServerMessage::Action {
            source: "s".into(),
            id: "a".into(),
            action: "go".into(),
        },
    ];
    for message in messages {
        assert_eq!(decode_server(&encode(&message)).unwrap(), message);
    }
}

#[test]
fn oversized_messages_are_refused_before_parsing() {
    let line = format!(
        r#"{{"v":1,"type":"hello","client":"{}"}}"#,
        "x".repeat(MAX_MESSAGE_BYTES)
    );
    assert_eq!(decode_client(&line).unwrap_err().code, ErrorCode::TooLarge);
}

#[test]
fn text_limits_count_characters_not_bytes() {
    let mut a = activity();
    a.title = "é".repeat(MAX_TITLE_CHARS);
    assert!(a.validate().is_ok());
    a.title.push('é');
    assert!(a.validate().is_err());
    a.title = "line one\nline two".into();
    assert!(a.validate().is_err(), "newlines are refused");
}

#[test]
fn lifetimes_default_by_state() {
    let mut a = activity();
    assert_eq!(a.lifetime_secs(), DEFAULT_LIFETIME_SECS);
    a.state = State::Succeeded;
    assert_eq!(a.lifetime_secs(), FINISHED_LIFETIME_SECS);
    a.expires_in = Some(30);
    assert_eq!(a.lifetime_secs(), 30);
    a.expires_in = Some(0);
    assert!(a.validate().is_err());
    a.expires_in = Some(MAX_EXPIRES_IN_SECS + 1);
    assert!(a.validate().is_err());
}

#[test]
fn open_url_allows_only_web_links() {
    let link = |url: &str| Action {
        id: "o".into(),
        label: "Open".into(),
        kind: ActionKind::OpenUrl { url: url.into() },
        primary: false,
    };
    assert!(link("https://example.com").validate().is_ok());
    assert!(link("HTTP://example.com").validate().is_ok());
    for bad in [
        "javascript:alert(1)",
        "file:///C:/x",
        "ms-settings:",
        "C:\\x.exe",
        "",
    ] {
        assert!(link(bad).validate().is_err(), "{bad} should be refused");
    }
}

#[test]
fn duplicate_action_ids_are_refused() {
    let mut a = activity();
    let dismiss = Action {
        id: "x".into(),
        label: "Dismiss".into(),
        kind: ActionKind::Dismiss,
        primary: false,
    };
    a.actions = vec![dismiss.clone(), dismiss];
    assert!(a.validate().is_err());
}
