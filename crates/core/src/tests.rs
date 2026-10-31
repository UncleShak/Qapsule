use std::time::{Duration, Instant};

use qapsule_protocol::{
    Action, ActionKind, Activity, ClientMessage, ErrorCode, Progress, ServerMessage, State,
};

use super::*;

/// A fake clock: a fixed start plus however much time the test has moved on.
struct Clock(Instant, Duration);

impl Clock {
    fn new() -> Self {
        Clock(Instant::now(), Duration::ZERO)
    }
    fn now(&self) -> Instant {
        self.0 + self.1
    }
    fn advance(&mut self, millis: u64) -> Instant {
        self.1 += Duration::from_millis(millis);
        self.now()
    }
}

fn activity(source: &str, id: &str, state: State) -> Activity {
    Activity {
        id: id.into(),
        source: source.into(),
        title: format!("{id} title"),
        subtitle: Some(format!("{id} subtitle")),
        state,
        progress: None,
        icon: None,
        expires_in: None,
        actions: vec![],
    }
}

fn action(id: &str, kind: ActionKind, primary: bool) -> Action {
    Action {
        id: id.into(),
        label: id.into(),
        kind,
        primary,
    }
}

fn upsert(core: &mut Core, a: Activity, now: Instant) -> ServerMessage {
    core.handle(1, ClientMessage::Upsert { activity: a }, now)
}

fn end(core: &mut Core, source: &str, id: &str, state: State, now: Instant) -> ServerMessage {
    core.handle(
        1,
        ClientMessage::End {
            source: source.into(),
            id: id.into(),
            state,
            title: None,
            subtitle: None,
        },
        now,
    )
}

fn peek_ids(core: &Core, now: Instant) -> Vec<String> {
    match core.snapshot(now).peek {
        Peek::Idle => vec![],
        Peek::One { main } => vec![main.id],
        Peek::Dual { main, side } => vec![main.id, side.id],
    }
}

fn pop_id(core: &Core, now: Instant) -> Option<String> {
    core.snapshot(now).pop.map(|p| p.id)
}

#[test]
fn upsert_then_list_shows_the_activity() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    assert_eq!(
        upsert(&mut core, activity("s", "a", State::Running), clock.now()),
        ServerMessage::Ok
    );
    let ServerMessage::ListResult { activities } =
        core.handle(1, ClientMessage::List { source: None }, clock.now())
    else {
        panic!("expected a list");
    };
    assert_eq!(activities.len(), 1);
    assert_eq!(activities[0].expires_in, Some(3600));
}

#[test]
fn pushing_the_same_id_updates_instead_of_adding() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(&mut core, activity("s", "a", State::Running), clock.now());
    let mut changed = activity("s", "a", State::Running);
    changed.title = "new title".into();
    upsert(&mut core, changed, clock.now());
    upsert(
        &mut core,
        activity("other", "a", State::Running),
        clock.now(),
    );
    assert_eq!(
        core.store().len(),
        2,
        "same id in another source is a different activity"
    );
    let Peek::Dual { main, .. } = core.snapshot(clock.now()).peek else {
        panic!("expected dual");
    };
    assert_eq!(main.source, "other", "ties go to the newest");
}

#[test]
fn priority_is_needs_input_failed_progress_running_then_finished() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut with_progress = activity("s", "progress", State::Running);
    with_progress.progress = Some(Progress::Fraction(0.3));
    upsert(
        &mut core,
        activity("s", "info", State::Info),
        clock.advance(1),
    );
    upsert(
        &mut core,
        activity("s", "running", State::Running),
        clock.advance(1),
    );
    upsert(&mut core, with_progress, clock.advance(1));
    upsert(
        &mut core,
        activity("s", "failed", State::Failed),
        clock.advance(1),
    );
    upsert(
        &mut core,
        activity("s", "input", State::NeedsInput),
        clock.advance(1),
    );
    let order: Vec<_> = core
        .store()
        .ranked()
        .iter()
        .map(|(k, _)| k.1.clone())
        .collect();
    assert_eq!(order, ["input", "failed", "progress", "running", "info"]);
}

#[test]
fn peek_is_idle_then_one_then_dual() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    assert_eq!(core.snapshot(clock.now()).peek, Peek::Idle);
    upsert(&mut core, activity("s", "a", State::Running), clock.now());
    assert_eq!(peek_ids(&core, clock.now()), ["a"]);
    upsert(
        &mut core,
        activity("s", "b", State::NeedsInput),
        clock.now(),
    );
    assert_eq!(peek_ids(&core, clock.now()), ["b", "a"]);

    let settings = Settings {
        dual_status: false,
        ..Settings::default()
    };
    core.set_settings(settings, clock.now());
    assert_eq!(peek_ids(&core, clock.now()), ["b"]);
}

#[test]
fn swapping_flips_the_dual_peek_until_the_order_changes() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("s", "a", State::Running),
        clock.advance(1),
    );
    upsert(
        &mut core,
        activity("s", "b", State::Running),
        clock.advance(1),
    );
    assert_eq!(peek_ids(&core, clock.now()), ["b", "a"]);
    core.swap_peek(clock.now());
    assert_eq!(peek_ids(&core, clock.now()), ["a", "b"]);
    core.swap_peek(clock.now());
    assert_eq!(peek_ids(&core, clock.now()), ["b", "a"]);
    core.swap_peek(clock.now());
    upsert(
        &mut core,
        activity("s", "c", State::Running),
        clock.advance(1),
    );
    assert_eq!(
        peek_ids(&core, clock.now()),
        ["c", "b"],
        "a new item resets the swap"
    );
}

#[test]
fn running_activities_expire_after_an_hour_and_finished_ones_after_four_seconds() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("s", "long", State::Running),
        clock.now(),
    );
    upsert(
        &mut core,
        activity("s", "short", State::Running),
        clock.now(),
    );
    end(&mut core, "s", "short", State::Succeeded, clock.now());
    assert_eq!(
        core.next_deadline(),
        Some(clock.now()),
        "an update is owed first"
    );
    core.take_update(clock.now());
    assert_eq!(
        core.next_deadline(),
        Some(clock.now() + Duration::from_secs(4))
    );

    core.tick(clock.advance(3_999));
    assert_eq!(core.store().len(), 2);
    core.tick(clock.advance(1));
    assert_eq!(core.store().len(), 1);
    core.tick(clock.advance(3_600_000));
    assert!(core.store().is_empty());
}

#[test]
fn custom_expiry_is_respected() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("s", "a", State::Info);
    a.expires_in = Some(10);
    upsert(&mut core, a, clock.now());
    core.tick(clock.advance(9_999));
    assert_eq!(core.store().len(), 1);
    core.tick(clock.advance(1));
    assert!(core.store().is_empty());
}

#[test]
fn end_of_an_unknown_activity_is_not_found() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let reply = end(&mut core, "s", "nope", State::Succeeded, clock.now());
    assert!(matches!(
        reply,
        ServerMessage::Error {
            code: ErrorCode::NotFound,
            ..
        }
    ));
}

#[test]
fn a_source_may_have_at_most_fifty_activities() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    for i in 0..50 {
        assert_eq!(
            upsert(
                &mut core,
                activity("s", &i.to_string(), State::Info),
                clock.now()
            ),
            ServerMessage::Ok
        );
    }
    let reply = upsert(&mut core, activity("s", "51", State::Info), clock.now());
    assert!(matches!(
        reply,
        ServerMessage::Error {
            code: ErrorCode::TooManyActivities,
            ..
        }
    ));
    assert_eq!(
        upsert(&mut core, activity("s", "0", State::Running), clock.now()),
        ServerMessage::Ok,
        "updates still work"
    );
    assert_eq!(
        upsert(&mut core, activity("t", "0", State::Info), clock.now()),
        ServerMessage::Ok,
        "other sources still work"
    );
}

#[test]
fn clear_removes_one_a_source_or_everything() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    for (s, id) in [("s", "a"), ("s", "b"), ("t", "a")] {
        upsert(&mut core, activity(s, id, State::NeedsInput), clock.now());
    }
    let clear = |source: Option<&str>, id: Option<&str>| ClientMessage::Clear {
        source: source.map(Into::into),
        id: id.map(Into::into),
    };
    core.handle(1, clear(Some("s"), Some("a")), clock.now());
    assert_eq!(core.store().len(), 2);
    core.handle(1, clear(Some("s"), None), clock.now());
    assert_eq!(core.store().len(), 1);
    assert_eq!(
        pop_id(&core, clock.now()).as_deref(),
        Some("a"),
        "t/a's pop is still there"
    );
    core.handle(1, clear(None, None), clock.now());
    assert!(core.store().is_empty());
    assert_eq!(
        pop_id(&core, clock.now()),
        None,
        "pops of cleared activities go too"
    );
}

#[test]
fn needs_input_pops_and_stays_until_answered() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::Running),
        clock.now(),
    );
    assert_eq!(pop_id(&core, clock.now()), None, "running doesn't pop");
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    let pop = core.snapshot(clock.now()).pop.unwrap();
    assert!(pop.sticky);
    assert_eq!(pop.title, "cc title");

    core.tick(clock.advance(60_000));
    assert_eq!(
        pop_id(&core, clock.now()).as_deref(),
        Some("cc"),
        "still waiting after a minute"
    );
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    assert_eq!(
        core.snapshot(clock.now()).pop.unwrap().seq,
        pop.seq,
        "the same state doesn't pop again"
    );

    upsert(
        &mut core,
        activity("claude-code", "cc", State::Running),
        clock.now(),
    );
    assert_eq!(
        pop_id(&core, clock.now()),
        None,
        "answered, so the pop goes"
    );
}

#[test]
fn finished_pops_shrink_back_after_five_seconds() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("wrap", "w", State::Running),
        clock.now(),
    );
    end(&mut core, "wrap", "w", State::Succeeded, clock.now());
    assert!(!core.snapshot(clock.now()).pop.unwrap().sticky);
    core.tick(clock.advance(4_000));
    assert!(core.store().is_empty(), "the activity has gone");
    assert_eq!(
        pop_id(&core, clock.now()).as_deref(),
        Some("w"),
        "but its pop finishes its five seconds"
    );
    core.tick(clock.advance(1_000));
    assert_eq!(pop_id(&core, clock.now()), None);
}

#[test]
fn info_pops_only_when_new() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(&mut core, activity("s", "i", State::Info), clock.now());
    let seq = core.snapshot(clock.now()).pop.unwrap().seq;
    upsert(&mut core, activity("s", "i", State::Info), clock.now());
    assert_eq!(core.snapshot(clock.now()).pop.unwrap().seq, seq);
}

#[test]
fn a_newer_pop_shows_first_then_the_waiting_one_returns() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    upsert(
        &mut core,
        activity("wrap", "w", State::Failed),
        clock.advance(1_000),
    );
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("w"));
    core.tick(clock.advance(5_000));
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("cc"));
}

#[test]
fn per_source_settings_control_pops_and_details() {
    let clock = Clock::new();
    let (settings, warnings) = Settings::from_toml(
        r#"
        [notifications]
        agents = { pop = true, details = false }
        my-script = { pop = false }
        "#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut core = Core::new(settings, clock.now());

    upsert(
        &mut core,
        activity("my-script", "quiet", State::Failed),
        clock.now(),
    );
    assert_eq!(pop_id(&core, clock.now()), None);

    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    let pop = core.snapshot(clock.now()).pop.unwrap();
    assert_eq!(pop.title, "Needs your input");
    assert_eq!(pop.subtitle, None);
    assert_eq!(pop.source_label, "Claude Code");
}

#[test]
fn do_not_disturb_holds_pops_until_it_ends() {
    let clock = Clock::new();
    let mut settings = Settings {
        do_not_disturb: true,
        ..Settings::default()
    };
    let mut core = Core::new(settings.clone(), clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    upsert(&mut core, activity("wrap", "w", State::Failed), clock.now());
    assert_eq!(pop_id(&core, clock.now()), None);
    settings.do_not_disturb = false;
    core.set_settings(settings, clock.now());
    assert_eq!(
        pop_id(&core, clock.now()).as_deref(),
        Some("cc"),
        "only what still needs you comes back"
    );
}

fn needs_input_with_actions() -> Activity {
    let mut a = activity("claude-code", "cc", State::NeedsInput);
    a.actions = vec![
        action("go", ActionKind::FocusWindow { pid: 4821 }, true),
        action("x", ActionKind::Dismiss, false),
    ];
    a
}

#[test]
fn pressing_focus_window_handles_the_pop_but_keeps_the_activity() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(&mut core, needs_input_with_actions(), clock.now());
    let pop = core.snapshot(clock.now()).pop.unwrap();
    assert_eq!(pop.actions[0].kind, "focus_window");
    assert!(pop.actions[0].primary && !pop.actions[1].primary);

    assert_eq!(
        core.press("claude-code", "cc", "go", clock.now()),
        Some(Effect::FocusWindow(4821))
    );
    assert_eq!(pop_id(&core, clock.now()), None);
    assert_eq!(peek_ids(&core, clock.now()), ["cc"]);
    assert_eq!(core.press("claude-code", "cc", "nope", clock.now()), None);
}

#[test]
fn dismiss_removes_the_activity_and_undo_brings_it_back() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(&mut core, needs_input_with_actions(), clock.now());
    assert_eq!(core.press("claude-code", "cc", "x", clock.now()), None);
    let state = core.snapshot(clock.now());
    assert!(state.undo && state.pop.is_none() && state.peek == Peek::Idle);

    core.undo(clock.advance(1_000));
    let state = core.snapshot(clock.now());
    assert!(!state.undo);
    assert_eq!(state.pop.unwrap().id, "cc");
    assert_eq!(peek_ids(&core, clock.now()), ["cc"]);

    core.press("claude-code", "cc", "x", clock.now());
    core.tick(clock.advance(4_000));
    assert!(!core.snapshot(clock.now()).undo, "the undo offer runs out");
    core.undo(clock.now());
    assert!(core.store().is_empty());
}

#[test]
fn dismissing_just_the_pop_keeps_the_activity() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(&mut core, needs_input_with_actions(), clock.now());
    core.dismiss_pop(clock.now());
    let state = core.snapshot(clock.now());
    assert!(state.undo && state.pop.is_none());
    assert_eq!(peek_ids(&core, clock.now()), ["cc"]);
}

#[test]
fn callbacks_go_to_the_connection_that_pushed() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("s", "a", State::Info);
    a.actions = vec![action("ok", ActionKind::Callback, true)];
    core.handle(7, ClientMessage::Upsert { activity: a }, clock.now());
    assert_eq!(
        core.press("s", "a", "ok", clock.now()),
        Some(Effect::Callback {
            conn: 7,
            message: ServerMessage::Action {
                source: "s".into(),
                id: "a".into(),
                action: "ok".into()
            }
        })
    );
    core.connection_closed(7);
    assert_eq!(
        core.press("s", "a", "ok", clock.now()),
        None,
        "nobody left to call back"
    );
}

#[test]
fn open_url_is_passed_to_the_shell() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("s", "a", State::Info);
    a.actions = vec![action(
        "o",
        ActionKind::OpenUrl {
            url: "https://example.com".into(),
        },
        false,
    )];
    upsert(&mut core, a, clock.now());
    assert_eq!(
        core.press("s", "a", "o", clock.now()),
        Some(Effect::OpenUrl("https://example.com".into()))
    );
}

#[test]
fn ui_updates_are_coalesced_to_four_per_second() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut sent = 0;
    let mut last = None;
    // A client pushing progress 10 times a second for 2 seconds.
    for i in 0..20 {
        let mut a = activity("wrap", "w", State::Running);
        a.progress = Some(Progress::Fraction(f64::from(i) / 20.0));
        upsert(&mut core, a, clock.now());
        while let Some(due) = core.next_deadline().filter(|d| *d <= clock.now()) {
            core.tick(due);
            match core.take_update(clock.now()) {
                Some(state) => {
                    sent += 1;
                    last = Some(state);
                }
                None => break,
            }
        }
        clock.advance(100);
    }
    assert!(sent <= 9, "{sent} updates in 2 seconds");
    // The final value is never lost: it arrives when the interval allows.
    let due = core.next_deadline().unwrap();
    if let Some(state) = core.take_update(due.max(clock.now())) {
        last = Some(state);
    }
    let Some(UiState {
        peek: Peek::One { main },
        ..
    }) = last
    else {
        panic!("expected one item");
    };
    assert_eq!(main.progress, Some(0.95));
}

#[test]
fn the_first_update_is_sent_at_once() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    assert!(core.take_update(clock.now()).is_some());
    assert!(core.take_update(clock.now()).is_none(), "nothing changed");
    upsert(&mut core, activity("s", "a", State::Running), clock.now());
    assert!(
        core.take_update(clock.now()).is_none(),
        "too soon after the last"
    );
    assert_eq!(core.next_deadline(), Some(clock.now() + UI_UPDATE_INTERVAL));
}

#[test]
fn elapsed_minutes_count_from_the_start_of_the_run() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::Running),
        clock.now(),
    );
    clock.advance(6 * 60_000 + 59_000);
    upsert(
        &mut core,
        activity("claude-code", "cc", State::Running),
        clock.now(),
    );
    let Peek::One { main } = core.snapshot(clock.now()).peek else {
        panic!()
    };
    assert_eq!(main.elapsed_min, Some(6));
    assert_eq!(main.icon, "agent");

    end(
        &mut core,
        "claude-code",
        "cc",
        State::Succeeded,
        clock.now(),
    );
    let Peek::One { main } = core.snapshot(clock.now()).peek else {
        panic!()
    };
    assert_eq!(main.elapsed_min, None);
    upsert(
        &mut core,
        activity("claude-code", "cc", State::Running),
        clock.now(),
    );
    let Peek::One { main } = core.snapshot(clock.now()).peek else {
        panic!()
    };
    assert_eq!(
        main.elapsed_min,
        Some(0),
        "a new run starts the count again"
    );
}

#[test]
fn visibility_modes() {
    let mut clock = Clock::new();
    let mut settings = Settings::default();
    let mut core = Core::new(settings.clone(), clock.now());
    let shown = |core: &Core, now| core.snapshot(now).shown;

    // Auto: visible, except behind a fullscreen app.
    assert!(shown(&core, clock.now()));
    core.set_fullscreen(true, clock.now());
    assert!(!shown(&core, clock.now()));
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    assert!(shown(&core, clock.now()), "allowed pops still come out");
    core.dismiss_pop(clock.now());
    core.tick(clock.advance(4_000));
    assert!(!shown(&core, clock.now()));

    // Always show: visible even over fullscreen apps.
    settings.visibility = VisibilityMode::Always;
    core.set_settings(settings.clone(), clock.now());
    assert!(shown(&core, clock.now()));

    // Peek on demand: tucked away until hovered or the hotkey.
    settings.visibility = VisibilityMode::OnDemand;
    core.set_settings(settings, clock.now());
    core.set_fullscreen(false, clock.now());
    assert!(!shown(&core, clock.now()));
    core.set_pointer(true, clock.now());
    assert!(shown(&core, clock.now()));
    core.set_pointer(false, clock.now());
    core.tick(clock.advance(599));
    assert!(
        shown(&core, clock.now()),
        "a short grace period after leaving"
    );
    core.tick(clock.advance(1));
    assert!(!shown(&core, clock.now()));
    core.toggle_expanded(clock.now());
    assert!(shown(&core, clock.now()));
    core.toggle_expanded(clock.now());
    assert!(!shown(&core, clock.now()));
}

#[test]
fn settings_ignore_mistakes_and_keep_defaults() {
    let (settings, warnings) = Settings::from_toml(
        r#"
        [general]
        visibility = "sometimes"
        hotkey = "Ctrl+Alt+Space"
        [peek]
        dual_status = "yes"
        [notifications]
        do_not_disturb = true
        music = { pop = false, details = 3 }
        "#,
    );
    assert_eq!(settings.visibility, VisibilityMode::Auto);
    assert!(settings.dual_status);
    assert!(settings.do_not_disturb);
    assert_eq!(
        settings.pop_setting("music"),
        PopSetting {
            pop: false,
            details: true
        }
    );
    assert_eq!(warnings.len(), 3, "{warnings:?}");

    let (settings, warnings) = Settings::from_toml("this is [not toml");
    assert_eq!(settings, Settings::default());
    assert_eq!(warnings.len(), 1);
}

#[test]
fn pop_settings_fall_back_from_source_to_category_to_default() {
    let (settings, _) = Settings::from_toml(
        r#"
        [notifications]
        agents = { pop = false }
        codex = { pop = true, details = false }
        "#,
    );
    assert!(!settings.pop_setting("claude-code").pop);
    assert_eq!(
        settings.pop_setting("codex"),
        PopSetting {
            pop: true,
            details: false
        }
    );
    assert_eq!(settings.pop_setting("anything"), PopSetting::default());
}

#[test]
fn rate_limiter_allows_twenty_per_second() {
    let mut clock = Clock::new();
    let mut limiter = RateLimiter::new(20);
    for _ in 0..20 {
        assert!(limiter.allow(clock.now()));
    }
    assert!(!limiter.allow(clock.now()));
    assert!(!limiter.allow(clock.advance(999)));
    assert!(limiter.allow(clock.advance(1)));
}

#[test]
fn touching_the_top_edge_reveals_a_tucked_capsule_for_a_while() {
    let mut clock = Clock::new();
    let settings = Settings {
        visibility: VisibilityMode::OnDemand,
        ..Settings::default()
    };
    let mut core = Core::new(settings, clock.now());
    assert!(!core.snapshot(clock.now()).shown);
    core.reveal_from_edge(clock.now());
    assert!(core.snapshot(clock.now()).shown);
    core.tick(clock.advance(1_999));
    assert!(core.snapshot(clock.now()).shown);
    core.tick(clock.advance(1));
    assert!(!core.snapshot(clock.now()).shown, "nobody came over");

    core.reveal_from_edge(clock.now());
    core.set_pointer(true, clock.advance(500));
    core.tick(clock.advance(5_000));
    assert!(
        core.snapshot(clock.now()).shown,
        "stays while the pointer is over it"
    );
}

#[test]
fn presence_follows_shown_without_waiting_for_the_coalescer() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.set_fullscreen(true, clock.now());
    assert_eq!(
        core.take_presence(clock.now()),
        vec![PresenceEffect::EdgeStrip(true)]
    );
    // An update goes out, so the next one is rate limited; presence isn't held back.
    assert!(core.take_update(clock.now()).is_some());
    core.toggle_expanded(clock.advance(10));
    assert!(core.take_update(clock.now()).is_none());
    assert_eq!(
        core.take_presence(clock.now()),
        vec![
            PresenceEffect::EdgeStrip(false),
            PresenceEffect::CreateWebview
        ]
    );
    core.toggle_expanded(clock.advance(10));
    assert_eq!(
        core.take_presence(clock.now()),
        vec![PresenceEffect::EdgeStrip(true)]
    );
    assert!(
        core.next_deadline()
            .is_some_and(|d| d <= clock.now() + presence::CLEAR_AFTER)
    );
}

#[test]
fn an_edge_reveal_shows_at_once_and_tucks_when_it_ends() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.set_fullscreen(true, clock.now());
    core.take_presence(clock.now());
    assert!(core.take_update(clock.now()).is_some());

    core.reveal_from_edge(clock.advance(10));
    assert_eq!(
        core.take_presence(clock.now()),
        vec![
            PresenceEffect::EdgeStrip(false),
            PresenceEffect::CreateWebview
        ]
    );
    assert!(core.take_update(clock.now()).is_none(), "rate limited");

    let end = clock.advance(EDGE_REVEAL.as_millis() as u64);
    core.tick(end);
    assert_eq!(
        core.take_presence(end),
        vec![PresenceEffect::EdgeStrip(true)]
    );
}

fn provider_core(toml: &str, clock: &Clock) -> Core {
    let (settings, warnings) = Settings::from_toml(toml);
    assert!(warnings.is_empty(), "{warnings:?}");
    Core::new(settings, clock.now())
}

#[test]
fn provider_upsert_pops_an_info_activity() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));
    let pop = core.snapshot(clock.now()).pop.unwrap();
    assert_eq!(pop.icon, "app");
    assert_eq!(pop.title, "n1 title");
}

#[test]
fn provider_pops_follow_source_and_category_settings() {
    let clock = Clock::new();
    let mut core = provider_core(
        "[notifications]\n\"windows:Fake\" = { pop = false }\n",
        &clock,
    );
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()), None);

    let mut core = provider_core("[notifications]\nwindows_apps = { pop = false }\n", &clock);
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()), None);

    let mut core = provider_core(
        "[notifications]\nwindows_apps = { pop = false }\n\"windows:Fake\" = { pop = true }\n",
        &clock,
    );
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));
}

#[test]
fn provider_details_off_hides_text_in_pop_and_list() {
    let clock = Clock::new();
    let mut core = provider_core(
        "[notifications]\nwindows_apps = { pop = true, details = false }\n",
        &clock,
    );
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    let state = core.snapshot(clock.now());
    let pop = state.pop.unwrap();
    assert_eq!(pop.title, "New notification");
    assert_eq!(pop.subtitle, None);
    assert!(matches!(state.peek, Peek::Idle));
    let ServerMessage::ListResult { activities } = core.handle(
        1,
        ClientMessage::List {
            source: Some("windows:Fake".into()),
        },
        clock.now(),
    ) else {
        panic!("expected a list");
    };
    assert_eq!(activities[0].title, "New notification");
    assert_eq!(activities[0].subtitle, None);

    // Pushed sources keep today's behaviour: details apply to pops only.
    let mut core = provider_core(
        "[notifications]\napps = { pop = true, details = false }\n",
        &clock,
    );
    upsert(&mut core, activity("other", "p1", State::Info), clock.now());
    let Peek::One { main } = core.snapshot(clock.now()).peek else {
        panic!("expected one item");
    };
    assert_eq!(main.title, "p1 title");
}

#[test]
fn provider_details_on_shows_text() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    let state = core.snapshot(clock.now());
    let pop = state.pop.unwrap();
    assert_eq!(pop.title, "n1 title");
    assert_eq!(pop.subtitle.as_deref(), Some("n1 subtitle"));
}

#[test]
fn do_not_disturb_holds_provider_pops() {
    let clock = Clock::new();
    let settings = Settings {
        do_not_disturb: true,
        ..Settings::default()
    };
    let mut core = Core::new(settings, clock.now());
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()), None);
    assert!(peek_ids(&core, clock.now()).is_empty());
    assert_eq!(core.store().len(), 1);
}

#[test]
fn provider_upsert_refuses_pushed_sources() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let err = core
        .provider_upsert(activity("my-script", "a", State::Info), clock.now())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidMessage);
    assert!(core.store().is_empty());
}

#[test]
fn the_fifty_first_provider_activity_evicts_the_oldest() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    for i in 0..50 {
        core.provider_upsert(
            activity("windows:Fake", &i.to_string(), State::Info),
            clock.advance(1),
        )
        .unwrap();
    }
    core.provider_upsert(
        activity("windows:Fake", "50", State::Info),
        clock.advance(1),
    )
    .unwrap();
    let store = core.store();
    assert_eq!(store.len(), 50);
    assert!(store.get(&("windows:Fake".into(), "0".into())).is_none());
    assert!(store.get(&("windows:Fake".into(), "1".into())).is_some());
    assert!(store.get(&("windows:Fake".into(), "50".into())).is_some());
}

#[test]
fn provider_remove_clears_the_activity_and_its_pop() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.provider_upsert(activity("windows:Fake", "n1", State::Info), clock.now())
        .unwrap();
    assert!(pop_id(&core, clock.now()).is_some());
    core.provider_remove("windows:Fake", "n1", clock.now());
    assert_eq!(pop_id(&core, clock.now()), None);
    assert!(core.store().is_empty());
    core.provider_remove("windows:Fake", "n1", clock.now());
}

#[test]
fn provider_callback_buttons_become_provider_effects() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("windows:Fake", "n1", State::Info);
    a.actions = vec![action("open", ActionKind::Callback, true)];
    core.provider_upsert(a, clock.now()).unwrap();
    assert_eq!(
        core.press("windows:Fake", "n1", "open", clock.now()),
        Some(Effect::Provider {
            source: "windows:Fake".into(),
            id: "n1".into(),
            action: "open".into(),
        })
    );
    assert_eq!(
        pop_id(&core, clock.now()),
        None,
        "the press handled the pop"
    );
}

#[test]
fn windows_sources_get_a_friendly_label() {
    assert_eq!(
        view::source_label("windows:Microsoft-Teams"),
        "Microsoft Teams"
    );
    assert_eq!(settings::category("windows:Anything"), "windows_apps");
}

#[test]
fn provider_remove_drops_a_pending_undo_for_that_activity() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("windows:Fake", "n1", State::Info);
    a.actions = vec![action("x", ActionKind::Dismiss, false)];
    core.provider_upsert(a, clock.now()).unwrap();
    core.press("windows:Fake", "n1", "x", clock.now());
    assert!(core.snapshot(clock.now()).undo);
    core.provider_remove("windows:Fake", "n1", clock.now());
    assert!(!core.snapshot(clock.now()).undo);
    core.undo(clock.now());
    assert!(core.store().is_empty());
    assert_eq!(pop_id(&core, clock.now()), None);
}

#[test]
fn eviction_drops_the_evicted_activitys_queued_pop() {
    let mut clock = Clock::new();
    let on = Settings::default();
    let (off, _) = Settings::from_toml("[notifications]\n\"windows:Fake\" = { pop = false }\n");
    let mut core = Core::new(on.clone(), clock.now());
    core.provider_upsert(activity("windows:Fake", "0", State::Info), clock.advance(1))
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("0"));
    core.set_settings(off, clock.now());
    // Do Not Disturb isn't involved, so the queued pop survives the settings change.
    for i in 1..50 {
        core.provider_upsert(
            activity("windows:Fake", &i.to_string(), State::Info),
            clock.advance(1),
        )
        .unwrap();
    }
    core.set_settings(on, clock.now());
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("0"));
    core.provider_upsert(
        activity("windows:Fake", "50", State::Info),
        clock.advance(1),
    )
    .unwrap();
    assert!(
        core.store()
            .get(&("windows:Fake".into(), "0".into()))
            .is_none()
    );
    assert_eq!(
        pop_id(&core, clock.now()).as_deref(),
        Some("50"),
        "the evicted pop is gone, not queued behind"
    );
    core.dismiss_pop(clock.now());
    assert_eq!(pop_id(&core, clock.now()), None);
}

#[test]
fn updating_an_existing_provider_activity_at_the_limit_evicts_nothing() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    for i in 0..50 {
        core.provider_upsert(
            activity("windows:Fake", &i.to_string(), State::Info),
            clock.advance(1),
        )
        .unwrap();
    }
    core.provider_upsert(activity("windows:Fake", "0", State::Info), clock.advance(1))
        .unwrap();
    assert_eq!(core.store().len(), 50);
    assert!(
        core.store()
            .get(&("windows:Fake".into(), "0".into()))
            .is_some()
    );
}

fn list_of(core: &mut Core, now: Instant) -> Vec<Activity> {
    let ServerMessage::ListResult { activities } = core.handle(
        1,
        ClientMessage::List {
            source: Some("windows:X".into()),
        },
        now,
    ) else {
        panic!("expected a list");
    };
    activities
}

#[test]
fn list_shows_real_text_with_details_on_and_per_app_override_wins() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.provider_upsert(activity("windows:X", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(list_of(&mut core, clock.now())[0].title, "n1 title");

    let mut core = provider_core(
        "[notifications]\nwindows_apps = { details = false }\n\"windows:X\" = { details = true }\n",
        &clock,
    );
    core.provider_upsert(activity("windows:X", "n1", State::Info), clock.now())
        .unwrap();
    let listed = list_of(&mut core, clock.now());
    assert_eq!(listed[0].title, "n1 title");
    assert_eq!(listed[0].subtitle.as_deref(), Some("n1 subtitle"));
}

#[test]
fn provider_notifications_pop_but_never_stay_in_the_peek() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.provider_upsert(activity("windows:X", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));
    assert!(matches!(core.snapshot(clock.now()).peek, Peek::Idle));
    let later = clock.advance(POP_DURATION.as_millis() as u64 + 1);
    core.tick(later);
    assert_eq!(pop_id(&core, later), None);
    assert!(matches!(core.snapshot(later).peek, Peek::Idle));
    assert_eq!(core.store().len(), 1, "still in the store");

    upsert(&mut core, activity("other", "p1", State::Running), later);
    assert_eq!(peek_ids(&core, later), vec!["p1"]);
    core.provider_upsert(activity("windows:X", "n2", State::Info), later)
        .unwrap();
    assert!(matches!(core.snapshot(later).peek, Peek::One { .. }));
    assert_eq!(peek_ids(&core, later), vec!["p1"]);
    core.swap_peek(later);
    assert_eq!(peek_ids(&core, later), vec!["p1"]);
}

#[test]
fn reserved_sources_match_without_regard_to_case() {
    assert!(qapsule_protocol::is_reserved_source("Windows:WhatsApp"));
    assert_eq!(settings::category("WINDOWS:x"), "windows_apps");
    assert_eq!(view::source_label("Windows:Some-App"), "Some App");
}

#[test]
fn calendar_sources_have_their_own_category_label_icon_and_generic_title() {
    assert_eq!(settings::category("calendar:events"), "calendar");
    assert_eq!(settings::category("CALENDAR:events"), "calendar");
    assert_eq!(view::source_label("calendar:events"), "Calendar");
    assert!(qapsule_protocol::is_reserved_source("Calendar:events"));
    assert_eq!(
        view::generic_title("calendar:events", qapsule_protocol::State::Info),
        "Upcoming event"
    );
    assert_eq!(
        view::generic_title("windows:X", qapsule_protocol::State::Info),
        "New notification"
    );
}

// --- The expanded view -------------------------------------------------------

fn click_settings() -> Settings {
    Settings {
        expand_on: ExpandOn::Click,
        ..Settings::default()
    }
}

fn hover_settings() -> Settings {
    Settings {
        expand_on: ExpandOn::Hover,
        ..Settings::default()
    }
}

fn is_open(core: &Core, now: Instant) -> bool {
    core.snapshot(now).expanded.is_some()
}

#[test]
fn dwell_opens_the_view_at_180_ms() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(179));
    assert!(!is_open(&core, clock.now()));
    assert_eq!(
        core.next_deadline()
            .map(|d| d <= clock.now() + Duration::from_millis(1)),
        Some(true),
        "the dwell is a deadline"
    );
    core.tick(clock.advance(1));
    assert!(is_open(&core, clock.now()));
    assert!(!core.snapshot(clock.now()).focus, "hover never takes focus");
}

#[test]
fn a_quick_pass_does_not_open() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.set_pointer(false, clock.advance(170));
    core.tick(clock.advance(100));
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn slowing_down_opens_60_ms_later() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(20));
    core.pointer_slowed(clock.now());
    core.tick(clock.advance(59));
    assert!(!is_open(&core, clock.now()));
    core.tick(clock.advance(1));
    assert!(is_open(&core, clock.now()));
}

#[test]
fn slowing_down_without_the_pointer_over_does_nothing() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.take_update(clock.now());
    core.pointer_slowed(clock.now());
    core.tick(clock.advance(500));
    assert!(!is_open(&core, clock.now()));
    assert_eq!(core.next_deadline(), None, "no timer was started");
}

#[test]
fn leaving_collapses_after_300_ms_and_returning_cancels() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    assert!(is_open(&core, clock.now()));

    core.set_pointer(false, clock.advance(100));
    core.tick(clock.advance(299));
    assert!(is_open(&core, clock.now()));
    core.tick(clock.advance(1));
    assert!(!is_open(&core, clock.now()));

    core.set_pointer(true, clock.advance(1_000));
    core.tick(clock.advance(180));
    assert!(is_open(&core, clock.now()));
    core.set_pointer(false, clock.advance(10));
    core.tick(clock.advance(250));
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(5_000));
    assert!(
        is_open(&core, clock.now()),
        "re-entering cancelled the collapse"
    );
}

#[test]
fn click_mode_ignores_hover_and_opens_on_a_click() {
    let mut clock = Clock::new();
    let mut core = Core::new(click_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.pointer_slowed(clock.advance(10));
    core.tick(clock.advance(1_000));
    assert!(!is_open(&core, clock.now()));
    core.expand_click(clock.now());
    assert!(is_open(&core, clock.now()));
    // A click-opened view still closes when the pointer leaves.
    core.set_pointer(false, clock.advance(10));
    core.tick(clock.advance(300));
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn hover_mode_ignores_expand_click() {
    let clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.expand_click(clock.now());
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn switching_to_click_mode_ends_a_dwell_in_progress() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.set_settings(click_settings(), clock.advance(10));
    core.tick(clock.advance(500));
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn the_hotkey_pins_the_view_open_until_esc_or_the_hotkey() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    let state = core.snapshot(clock.now());
    assert!(state.expanded.is_some());
    assert!(state.focus);

    core.set_pointer(true, clock.advance(10));
    core.set_pointer(false, clock.advance(10));
    core.tick(clock.advance(10_000));
    assert!(
        is_open(&core, clock.now()),
        "leaving never collapses a pinned view"
    );

    core.close_expanded(clock.advance(10));
    let state = core.snapshot(clock.now());
    assert!(state.expanded.is_none());
    assert!(!state.focus);

    core.toggle_expanded(clock.advance(10));
    assert!(is_open(&core, clock.now()));
    core.toggle_expanded(clock.advance(10));
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn esc_closes_a_hover_opened_view_and_clears_its_timers() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    core.set_pointer(false, clock.advance(10));
    core.close_expanded(clock.advance(10));
    assert!(!is_open(&core, clock.now()));
    // No stale collapse or dwell deadline is left behind.
    core.take_update(clock.now());
    core.tick(clock.advance(400));
    assert!(core.next_deadline().is_none_or(|d| d > clock.now()));
}

#[test]
fn hover_and_click_never_open_over_a_pop_or_undo_but_the_hotkey_does() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    core.set_pointer(true, clock.now());
    core.pointer_slowed(clock.now());
    core.tick(clock.advance(500));
    assert!(!is_open(&core, clock.now()), "a pop is showing");
    core.set_pointer(false, clock.now());

    // The pop's own dwell is gone; so is a click.
    let mut clicker = Core::new(click_settings(), clock.now());
    upsert(
        &mut clicker,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    clicker.expand_click(clock.now());
    assert!(!is_open(&clicker, clock.now()));

    // The undo bar blocks it too.
    let mut core = Core::new(hover_settings(), clock.now());
    let mut dismissable = activity("claude-code", "d", State::Running);
    dismissable.actions = vec![action("dismiss", ActionKind::Dismiss, false)];
    upsert(&mut core, dismissable, clock.now());
    core.press("claude-code", "d", "dismiss", clock.now());
    assert!(core.snapshot(clock.now()).undo);
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    assert!(!is_open(&core, clock.now()), "the undo bar is showing");

    core.toggle_expanded(clock.now());
    let state = core.snapshot(clock.now());
    assert!(state.expanded.is_some());
    assert!(!state.undo, "the undo bar waits behind the view");
}

#[test]
fn a_pop_that_arrives_while_open_is_hidden_then_shows_for_five_seconds() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    core.provider_upsert(activity("windows:X", "n1", State::Info), clock.now())
        .unwrap();
    assert_eq!(pop_id(&core, clock.now()), None, "hidden while open");

    core.tick(clock.advance(20_000));
    core.take_update(clock.now());
    assert!(
        core.next_deadline().is_none_or(|d| d > clock.now()),
        "the paused pop is no deadline in the past"
    );
    core.close_expanded(clock.now());
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));
    core.tick(clock.advance(4_999));
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));
    core.tick(clock.advance(1));
    assert_eq!(pop_id(&core, clock.now()), None);
}

#[test]
fn a_needs_input_pop_stays_after_the_view_closes() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    assert_eq!(pop_id(&core, clock.now()), None);
    core.tick(clock.advance(30_000));
    core.close_expanded(clock.now());
    core.tick(clock.advance(60_000));
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("cc"));
}

#[test]
fn expanded_and_focus_follow_the_state() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_clock("09:41".into(), "Sat 31 Oct".into(), clock.now());
    assert!(core.snapshot(clock.now()).expanded.is_none());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    let state = core.snapshot(clock.now());
    let view = state.expanded.expect("open");
    assert_eq!(view.date, "Sat 31 Oct");
    assert_eq!(view.weather, WeatherView::NoCity);
    assert_eq!(view.events, EventsView::Loading);
    assert!(!state.focus, "only the hotkey asks for focus");
}

#[test]
fn the_weather_starts_loading_when_a_city_is_set() {
    let (settings, _) = Settings::from_toml(
        r#"
        [[screens]]
        widgets = [{ type = "agents" }]
        [[screens]]
        widgets = [{ type = "weather", city = "Pretoria" }]
        [[screens]]
        widgets = [{ type = "audio" }]
        "#,
    );
    let clock = Clock::new();
    let mut core = Core::new(settings, clock.now());
    core.toggle_expanded(clock.now());
    assert_eq!(
        core.snapshot(clock.now()).expanded.unwrap().weather,
        WeatherView::Loading
    );
}

fn weather_ready() -> WeatherView {
    WeatherView::Ready(WeatherReady {
        city: "Pretoria".into(),
        temp_c: 24,
        condition: "Sunny".into(),
        glyph: "sun".into(),
        high_c: 28,
        low_c: 14,
        hourly: vec![HourView {
            label: "Now".into(),
            temp_c: 24,
            glyph: "sun".into(),
        }],
        stale: false,
    })
}

#[test]
fn widget_updates_while_closed_send_nothing_but_show_once_opened() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    assert!(core.take_update(clock.now()).is_some(), "the first update");
    core.set_weather(weather_ready(), clock.advance(1_000));
    core.set_events(EventsView::NoLink, clock.now());
    assert!(core.take_update(clock.advance(1_000)).is_none());
    assert_eq!(core.next_deadline(), None, "nothing is owed");

    core.toggle_expanded(clock.advance(1_000));
    let state = core
        .take_update(clock.now())
        .expect("opening sends an update");
    let view = state.expanded.unwrap();
    assert_eq!(view.weather, weather_ready());
    assert_eq!(view.events, EventsView::NoLink);

    // While open, an update is sent (rate limited by the coalescer).
    core.set_events(EventsView::Unavailable, clock.advance(300));
    let state = core.take_update(clock.now()).expect("open: sent");
    assert_eq!(state.expanded.unwrap().events, EventsView::Unavailable);
}

#[test]
fn the_main_screen_is_reported_once_per_open_and_close() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    assert_eq!(core.take_main_screen(), None);
    core.toggle_expanded(clock.now());
    assert_eq!(core.take_main_screen(), Some(true));
    assert_eq!(core.take_main_screen(), None);
    core.close_expanded(clock.advance(10));
    assert_eq!(core.take_main_screen(), Some(false));
    assert_eq!(core.take_main_screen(), None);
    // Open and closed again between two calls is no change.
    core.toggle_expanded(clock.advance(10));
    core.close_expanded(clock.advance(10));
    assert_eq!(core.take_main_screen(), None);
}

#[test]
fn an_open_view_counts_as_shown_in_every_mode() {
    let mut clock = Clock::new();
    for mode in [
        VisibilityMode::Auto,
        VisibilityMode::Always,
        VisibilityMode::OnDemand,
    ] {
        let settings = Settings {
            visibility: mode,
            ..Settings::default()
        };
        let mut core = Core::new(settings, clock.now());
        core.set_fullscreen(true, clock.now());
        core.toggle_expanded(clock.now());
        assert!(core.snapshot(clock.now()).shown, "{mode:?}");
        core.close_expanded(clock.advance(10));
        let hidden = mode != VisibilityMode::Always;
        assert_eq!(!core.snapshot(clock.now()).shown, hidden, "{mode:?} closed");
    }
}

#[test]
fn the_widget_views_serialise_with_a_kind_tag() {
    // TOML stands in for JSON here: both go through serde the same way.
    let value = |v: &dyn ToValue| v.to_value();
    let weather = value(&weather_ready());
    assert_eq!(weather["kind"].as_str(), Some("ready"));
    assert_eq!(weather["city"].as_str(), Some("Pretoria"));
    assert_eq!(weather["temp_c"].as_integer(), Some(24));
    assert_eq!(weather["hourly"][0]["label"].as_str(), Some("Now"));
    assert_eq!(weather["stale"].as_bool(), Some(false));
    assert_eq!(
        value(&WeatherView::NoCity)["kind"].as_str(),
        Some("no_city")
    );
    let events = EventsView::Ready(EventsReady {
        now: Some(EventNow {
            title: "Standup".into(),
            until: "15:20".into(),
            location: None,
        }),
        next: vec![EventNext {
            title: "Lunch".into(),
            day: "Tomorrow".into(),
            time: "09:00".into(),
        }],
        stale: true,
    });
    let events = value(&events);
    assert_eq!(events["kind"].as_str(), Some("ready"));
    assert_eq!(events["now"]["until"].as_str(), Some("15:20"));
    assert_eq!(events["next"][0]["day"].as_str(), Some("Tomorrow"));
    assert_eq!(events["next"][0]["time"].as_str(), Some("09:00"));
    assert_eq!(value(&EventsView::NoLink)["kind"].as_str(), Some("no_link"));
}

trait ToValue {
    fn to_value(&self) -> toml::Value;
}

impl<T: serde::Serialize> ToValue for T {
    fn to_value(&self) -> toml::Value {
        toml::Value::try_from(self).unwrap()
    }
}

#[test]
fn expand_on_is_read_from_general() {
    assert_eq!(Settings::default().expand_on, ExpandOn::Click);
    let (settings, warnings) = Settings::from_toml("[general]\nexpand_on = \"hover\"");
    assert_eq!(settings.expand_on, ExpandOn::Hover);
    assert!(warnings.is_empty());
    let (settings, warnings) = Settings::from_toml("[general]\nexpand_on = \"tap\"");
    assert_eq!(settings.expand_on, ExpandOn::Click);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn default_screens_match_the_spec() {
    use WidgetKind::*;
    use WidgetSize::*;
    let kinds = |s: &Settings| -> Vec<Vec<(WidgetKind, WidgetSize)>> {
        s.screens
            .iter()
            .map(|screen| screen.widgets.iter().map(|w| (w.kind, w.size)).collect())
            .collect()
    };
    let expected = vec![
        vec![(Agents, Wide), (Timer, Small)],
        vec![(Weather, Small), (Time, Small), (Events, Small)],
        vec![(Audio, Wide), (Shortcuts, Small)],
    ];
    assert_eq!(kinds(&Settings::default()), expected);
    assert_eq!(Settings::default().weather_city(), None);

    let (settings, warnings) = Settings::from_toml(
        r#"
        [[screens]]
        widgets = [
          { type = "agents", size = "wide" },
          { type = "timer",  size = "small" },
        ]
        [[screens]]
        widgets = [
          { type = "weather", size = "small", city = "  Pretoria " },
          { type = "time",    size = "small" },
          { type = "events",  size = "small" },
        ]
        [[screens]]
        widgets = [
          { type = "audio",     size = "wide" },
          { type = "shortcuts", size = "small" },
        ]
        "#,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(kinds(&settings), expected);
    assert_eq!(settings.weather_city(), Some("Pretoria"));
}

#[test]
fn bad_screens_fall_back_to_the_defaults_and_warn() {
    // Not three screens.
    let (settings, warnings) = Settings::from_toml("[[screens]]\nwidgets = [{ type = \"time\" }]");
    assert_eq!(settings.screens, Settings::default().screens);
    assert!(!warnings.is_empty());

    // A screen that ends up empty after dropping an unknown widget.
    let (settings, warnings) = Settings::from_toml(
        r#"
        [[screens]]
        widgets = [{ type = "agents" }]
        [[screens]]
        widgets = [{ type = "bogus" }]
        [[screens]]
        widgets = [{ type = "audio" }]
        "#,
    );
    assert_eq!(settings.screens, Settings::default().screens);
    assert!(warnings.len() >= 2, "{warnings:?}");

    // Too many widgets.
    let (settings, warnings) = Settings::from_toml(
        r#"
        [[screens]]
        widgets = [{ type = "agents" }, { type = "timer" }, { type = "time" }, { type = "audio" }]
        [[screens]]
        widgets = [{ type = "weather" }]
        [[screens]]
        widgets = [{ type = "audio" }]
        "#,
    );
    assert_eq!(settings.screens, Settings::default().screens);
    assert!(!warnings.is_empty());
}

#[test]
fn an_unknown_widget_is_dropped_and_a_bad_city_ignored() {
    let (settings, warnings) = Settings::from_toml(
        r#"
        [[screens]]
        widgets = [{ type = "agents" }]
        [[screens]]
        widgets = [{ type = "bogus" }, { type = "weather", city = 5 }, { type = "time" }]
        [[screens]]
        widgets = [{ type = "audio" }]
        "#,
    );
    let centre = &settings.screens[1].widgets;
    assert_eq!(centre.len(), 2);
    assert_eq!(centre[0].kind, WidgetKind::Weather);
    assert_eq!(centre[0].city, None);
    assert_eq!(settings.weather_city(), None);
    assert_eq!(warnings.len(), 2, "{warnings:?}");
}

#[test]
fn calendar_minutes_before_is_read_and_range_checked() {
    assert_eq!(Settings::default().calendar_minutes_before, 10);
    let (settings, warnings) = Settings::from_toml(
        "[notifications]\ncalendar = { pop = false, details = false, minutes_before = 30 }",
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(settings.calendar_minutes_before, 30);
    assert_eq!(
        settings.notifications["calendar"],
        PopSetting {
            pop: false,
            details: false
        }
    );
    for bad in ["0", "121", "-5", "\"ten\"", "1.5"] {
        let (settings, warnings) = Settings::from_toml(&format!(
            "[notifications]\ncalendar = {{ minutes_before = {bad} }}"
        ));
        assert_eq!(settings.calendar_minutes_before, 10, "{bad}");
        assert_eq!(warnings.len(), 1, "{bad}: {warnings:?}");
    }
    for ok in ["1", "120"] {
        let (settings, warnings) = Settings::from_toml(&format!(
            "[notifications]\ncalendar = {{ minutes_before = {ok} }}"
        ));
        assert_eq!(settings.calendar_minutes_before.to_string(), ok);
        assert!(warnings.is_empty());
    }
}

#[test]
fn a_lost_webview_means_the_pointer_left() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    core.provider_upsert(activity("windows:X", "n1", State::Info), clock.now())
        .unwrap();
    assert!(is_open(&core, clock.now()));
    core.webview_gone(clock.advance(10));
    core.tick(clock.advance(300));
    assert!(!is_open(&core, clock.now()));
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("n1"));

    // A pinned view stays, and a pending dwell is dropped.
    core.toggle_expanded(clock.advance(10));
    core.webview_gone(clock.now());
    core.tick(clock.advance(1_000));
    assert!(is_open(&core, clock.now()));
    core.close_expanded(clock.now());
    core.set_pointer(true, clock.now());
    core.webview_gone(clock.advance(10));
    core.tick(clock.advance(500));
    assert!(!is_open(&core, clock.now()));
}

#[test]
fn dwell_opens_the_view_when_tucked_too() {
    let mut clock = Clock::new();
    let on_demand = Settings {
        visibility: VisibilityMode::OnDemand,
        ..hover_settings()
    };
    let mut core = Core::new(on_demand, clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(180));
    assert!(is_open(&core, clock.now()));

    let mut core = Core::new(hover_settings(), clock.now());
    core.set_fullscreen(true, clock.now());
    core.set_pointer(true, clock.now());
    core.pointer_slowed(clock.advance(10));
    core.tick(clock.advance(60));
    assert!(is_open(&core, clock.now()));
}

#[test]
fn the_undo_bar_waits_while_open_and_gets_its_time_after() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    let mut a = activity("claude-code", "d", State::Running);
    a.actions = vec![action("dismiss", ActionKind::Dismiss, false)];
    upsert(&mut core, a, clock.now());
    core.press("claude-code", "d", "dismiss", clock.now());
    core.toggle_expanded(clock.advance(1_000));
    core.take_update(clock.now());
    core.tick(clock.advance(20_000));
    core.take_update(clock.now());
    assert!(core.next_deadline().is_none_or(|d| d > clock.now()));
    core.close_expanded(clock.now());
    assert!(core.snapshot(clock.now()).undo);
    core.tick(clock.advance(3_999));
    assert!(core.snapshot(clock.now()).undo);
    core.tick(clock.advance(1));
    assert!(!core.snapshot(clock.now()).undo);
}

#[test]
fn a_full_pop_queue_keeps_the_pop_waiting_for_input() {
    let clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    upsert(
        &mut core,
        activity("claude-code", "cc", State::NeedsInput),
        clock.now(),
    );
    for i in 0..8 {
        core.provider_upsert(
            activity("windows:X", &format!("n{i}"), State::Info),
            clock.now(),
        )
        .unwrap();
    }
    core.close_expanded(clock.now());
    assert_eq!(core.pops.len(), MAX_POPS);
    assert!(core.pops.iter().any(|p| p.key.1 == "cc"));
    assert!(!core.pops.iter().any(|p| p.key.1 == "n0"));
}

#[test]
fn the_weather_view_resets_when_the_city_changes() {
    let clock = Clock::new();
    let with_city = |city: &str| {
        Settings::from_toml(&format!(
            "[[screens]]\nwidgets = [{{ type = \"agents\" }}]\n[[screens]]\nwidgets = [{{ type = \"weather\", city = \"{city}\" }}]\n[[screens]]\nwidgets = [{{ type = \"audio\" }}]"
        ))
        .0
    };
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    core.set_settings(with_city("Pretoria"), clock.now());
    assert_eq!(core.weather, WeatherView::Loading);
    core.set_weather(weather_ready(), clock.now());
    core.set_settings(with_city("Pretoria"), clock.now());
    assert_eq!(core.weather, weather_ready(), "same city keeps the data");
    core.set_settings(Settings::default(), clock.now());
    assert_eq!(core.weather, WeatherView::NoCity);
}

#[test]
fn a_city_over_100_characters_is_ignored() {
    let long = "x".repeat(101);
    let (settings, warnings) = Settings::from_toml(&format!(
        "[[screens]]\nwidgets = [{{ type = \"agents\" }}]\n[[screens]]\nwidgets = [{{ type = \"weather\", city = \"{long}\" }}]\n[[screens]]\nwidgets = [{{ type = \"audio\" }}]"
    ));
    assert_eq!(settings.weather_city(), None);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

#[test]
fn esc_while_the_pointer_is_over_is_not_undone_until_it_leaves() {
    let mut clock = Clock::new();
    let mut core = Core::new(hover_settings(), clock.now());
    core.set_pointer(true, clock.now());
    core.tick(clock.advance(200));
    assert!(is_open(&core, clock.now()));

    core.close_expanded(clock.advance(10));
    assert!(!is_open(&core, clock.now()));
    core.pointer_slowed(clock.advance(10));
    core.tick(clock.advance(1_000));
    assert!(!is_open(&core, clock.now()), "hovering can't reopen it");

    core.set_pointer(false, clock.advance(10));
    core.set_pointer(true, clock.advance(10));
    core.tick(clock.advance(200));
    assert!(is_open(&core, clock.now()), "a fresh hover opens it again");
}

#[test]
fn expired_timed_pops_are_dropped_when_the_view_closes_but_finished_ones_stay() {
    let mut clock = Clock::new();
    let mut core = Core::new(Settings::default(), clock.now());
    core.toggle_expanded(clock.now());
    let mut info = activity("calendar:events", "e1", State::Info);
    info.expires_in = Some(10);
    core.provider_upsert(info, clock.now()).unwrap();
    let done = activity("claude-code", "cc", State::Succeeded);
    upsert(&mut core, done, clock.now());
    core.tick(clock.advance(11_000));

    core.close_expanded(clock.now());
    core.tick(clock.advance(10));
    assert_eq!(pop_id(&core, clock.now()).as_deref(), Some("cc"));
    core.tick(clock.advance(5_000));
    assert_eq!(pop_id(&core, clock.now()), None);
    assert!(core.snapshot(clock.now()).pop.is_none());
}
