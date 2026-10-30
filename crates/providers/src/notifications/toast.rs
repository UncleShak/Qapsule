//! Pure conversion from a toast's fields to an activity. No Windows APIs here.

use qapsule_protocol::{Action, ActionKind, Activity, State};

const KEY_PREFIX: &str = "windows:";
const MAX_KEY: usize = 64;
const MAX_TITLE: usize = 80;
const MAX_SUBTITLE: usize = 120;
const MAX_AUMID: usize = 256;
const LIFETIME_SECS: u64 = 300;

/// What the listener reads from one toast.
#[derive(Debug, Clone)]
pub struct ToastFields {
    pub notification_id: u32,
    pub app_name: String,
    pub aumid: Option<String>,
    pub texts: Vec<String>,
}

/// `windows:` plus the app's name: no control characters, whitespace as dashes, at most 64 chars.
pub fn source_key(app_name: &str) -> String {
    let mut name = String::new();
    let mut pending_dash = false;
    for c in app_name.chars() {
        if c.is_whitespace() {
            pending_dash = true;
        } else if !c.is_control() {
            if pending_dash {
                name.push('-');
                pending_dash = false;
            }
            name.push(c);
        }
    }
    let name = name.trim_matches('-');
    let room = MAX_KEY - KEY_PREFIX.chars().count();
    let cut: String = name.chars().take(room).collect();
    let cut = cut.trim_matches('-');
    if cut.is_empty() {
        format!("{KEY_PREFIX}App")
    } else {
        format!("{KEY_PREFIX}{cut}")
    }
}

/// An AUMID we are willing to hand to the shell.
pub fn valid_aumid(s: &str) -> bool {
    let len = s.chars().count();
    (1..=MAX_AUMID).contains(&len)
        && !s.chars().any(|c| c.is_control() || "\"<>|".contains(c))
        && !s.contains("..")
        && !s.contains("://")
        && !s.starts_with('\\')
        && !s.starts_with(char::is_whitespace)
        && !s.ends_with(char::is_whitespace)
}

/// Control characters become spaces, whitespace runs collapse, ends are trimmed.
fn clean(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// At most `max` chars, ending in `…` when cut.
fn truncate(s: String, max: usize) -> String {
    if s.chars().count() <= max {
        return s;
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

pub fn to_activity(f: &ToastFields) -> Activity {
    let mut texts = f.texts.iter().map(|t| clean(t)).filter(|t| !t.is_empty());
    let title = texts
        .next()
        .map(|t| truncate(t, MAX_TITLE))
        .unwrap_or_else(|| "New notification".to_string());
    let rest = texts.collect::<Vec<_>>().join(" · ");
    let subtitle = (!rest.is_empty()).then(|| truncate(rest, MAX_SUBTITLE));

    let actions = match &f.aumid {
        Some(aumid) if valid_aumid(aumid) => vec![Action {
            id: "open".into(),
            label: "Open".into(),
            kind: ActionKind::Callback,
            primary: true,
        }],
        _ => Vec::new(),
    };

    Activity {
        id: f.notification_id.to_string(),
        source: source_key(&f.app_name),
        title,
        subtitle,
        state: State::Info,
        progress: None,
        icon: None,
        expires_in: Some(LIFETIME_SECS),
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(app: &str, aumid: Option<&str>, texts: &[&str]) -> ToastFields {
        ToastFields {
            notification_id: 7,
            app_name: app.into(),
            aumid: aumid.map(String::from),
            texts: texts.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn every_output_validates() {
        let long = "x".repeat(300);
        let names = ["Mail", "😀 Chat", "日本語アプリ", "   ", "", long.as_str()];
        let texts = [
            "hi",
            "😀😀😀",
            "日本語".repeat(100).leak(),
            "  \n\t ",
            "",
            long.as_str(),
        ];
        for name in names {
            for a in texts {
                for b in texts {
                    for aumid in [None, Some("App.Id!x"), Some("bad|id")] {
                        let act = to_activity(&fields(name, aumid, &[a, b]));
                        act.validate().unwrap();
                        assert!(act.source.chars().count() <= 64);
                    }
                }
            }
        }
        to_activity(&fields("", None, &[])).validate().unwrap();
    }

    #[test]
    fn spaces_become_dashes() {
        assert_eq!(source_key("Microsoft Teams"), "windows:Microsoft-Teams");
        assert_eq!(source_key("  A \t\n B  "), "windows:A-B");
    }

    #[test]
    fn empty_name_is_app() {
        assert_eq!(source_key(""), "windows:App");
        assert_eq!(source_key(" \n "), "windows:App");
        assert_eq!(source_key("\u{7}\u{0}"), "windows:App");
    }

    #[test]
    fn long_names_are_cut_by_chars() {
        let key = source_key(&"日".repeat(300));
        assert_eq!(key.chars().count(), 64);
        let key = source_key(&format!("{} {}", "a".repeat(55), "b"));
        assert!(!key.ends_with('-'));
        assert!(key.chars().count() <= 64);
    }

    #[test]
    fn multi_line_text_is_flattened() {
        let a = to_activity(&fields("A", None, &["one\r\ntwo\n\nthree", "x\ty", "z"]));
        assert_eq!(a.title, "one two three");
        assert_eq!(a.subtitle.as_deref(), Some("x y · z"));
    }

    #[test]
    fn truncation_counts_chars() {
        let a = to_activity(&fields("A", None, &[&"é".repeat(100), &"日".repeat(200)]));
        assert_eq!(a.title.chars().count(), 80);
        assert!(a.title.ends_with('…'));
        assert_eq!(a.subtitle.as_ref().unwrap().chars().count(), 120);
        assert!(a.subtitle.unwrap().ends_with('…'));
        let exact = to_activity(&fields("A", None, &[&"a".repeat(80)]));
        assert_eq!(exact.title, "a".repeat(80));
    }

    #[test]
    fn no_text_gives_default_title() {
        let a = to_activity(&fields("A", None, &[]));
        assert_eq!(a.title, "New notification");
        assert_eq!(a.subtitle, None);
        let b = to_activity(&fields("A", None, &["  ", "\n"]));
        assert_eq!(b.title, "New notification");
    }

    #[test]
    fn blank_texts_are_skipped() {
        let a = to_activity(&fields("A", None, &["", "Title", " ", "Body"]));
        assert_eq!(a.title, "Title");
        assert_eq!(a.subtitle.as_deref(), Some("Body"));
    }

    #[test]
    fn fixed_fields() {
        let a = to_activity(&fields("A", None, &["t"]));
        assert_eq!(a.id, "7");
        assert_eq!(a.state, State::Info);
        assert_eq!(a.icon, None);
        assert_eq!(a.progress, None);
        assert_eq!(a.expires_in, Some(300));
    }

    #[test]
    fn aumid_decides_the_action() {
        assert!(to_activity(&fields("A", None, &["t"])).actions.is_empty());
        assert!(
            to_activity(&fields("A", Some("a<b"), &["t"]))
                .actions
                .is_empty()
        );
        assert!(
            to_activity(&fields("A", Some(""), &["t"]))
                .actions
                .is_empty()
        );
        assert!(
            to_activity(&fields("A", Some(" x"), &["t"]))
                .actions
                .is_empty()
        );
        assert!(
            to_activity(&fields("A", Some("x\n"), &["t"]))
                .actions
                .is_empty()
        );
        assert!(
            to_activity(&fields("A", Some(&"a".repeat(257)), &["t"]))
                .actions
                .is_empty()
        );
        assert_eq!(
            to_activity(&fields("A", Some(&"a".repeat(256)), &["t"]))
                .actions
                .len(),
            1
        );
    }

    #[test]
    fn aumid_refuses_traversal_urls_and_unc_paths() {
        assert!(!valid_aumid("..\\x"));
        assert!(!valid_aumid("a\\..\\x"));
        assert!(!valid_aumid("\\\\server\\share"));
        assert!(!valid_aumid("\\x"));
        assert!(!valid_aumid("http://x"));
    }

    #[test]
    fn aumid_allows_single_backslash_paths_and_packaged_ids() {
        assert!(valid_aumid(
            "{1AC14E77-02E7-4E5D-B744-2EB1AE5198B7}\\cmd.exe"
        ));
        assert!(valid_aumid("5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App"));
    }

    #[test]
    fn aumid_with_shell_prefix_is_allowed_by_the_rules() {
        // It is only ever appended under `shell:AppsFolder\`, so no rule refuses it.
        assert!(valid_aumid("shell:foo"));
    }

    #[test]
    fn action_is_fixed_whatever_the_text() {
        for text in ["Open", "Delete everything", "日本語", ""] {
            let a = to_activity(&fields("A", Some("Pkg!App"), &[text]));
            assert_eq!(a.actions.len(), 1);
            let act = &a.actions[0];
            assert_eq!(act.id, "open");
            assert_eq!(act.label, "Open");
            assert_eq!(act.kind, ActionKind::Callback);
            assert!(act.primary);
        }
    }
}
