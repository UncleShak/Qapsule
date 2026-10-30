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
    core.toggle_pinned(clock.now());
    assert!(shown(&core, clock.now()));
    core.toggle_pinned(clock.now());
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
    core.toggle_pinned(clock.advance(10));
    assert!(core.take_update(clock.now()).is_none());
    assert_eq!(
        core.take_presence(clock.now()),
        vec![
            PresenceEffect::EdgeStrip(false),
            PresenceEffect::CreateWebview
        ]
    );
    core.toggle_pinned(clock.advance(10));
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
