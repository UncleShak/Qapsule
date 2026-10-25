# Architecture review: deepening opportunities

Date: 2026-10-25 · Branch: `milestone-5-lean-idle` · Status: open, candidate 1 done

Six refactors that would turn shallow modules into deep ones: a small interface with the behaviour behind it, tested through that interface. They came from a review of the files that changed most in the last five commits (`src-tauri`, `ui/src/capsule`, `crates/core`, `crates/server`). None needs a protocol change, and none contradicts a decision record; where a decision constrains a candidate, the candidate says so.

Each candidate ends with **open questions** to settle before starting. Settle them, then write the plan; if a candidate is rejected for a lasting reason, record why here (or in `docs/decisions/` if it is architectural) so it isn't suggested again.

Terms: a **module** is anything with an interface and an implementation; it is **deep** when the interface is small next to what it does, **shallow** when the two are nearly the same size. A **seam** is a place where behaviour can be swapped without editing callers; an **adapter** is what plugs into it. One adapter makes a seam hypothetical; two (say Win32 in the app, a recording fake in tests) make it real.

## Summary

| # | Candidate | Strength | Depends on |
| --- | --- | --- | --- |
| 1 | [Presence module for window show/hide](#1-presence-module-for-window-showhide) | Done | — |
| 2 | [Coalescer compares state instead of counting marks](#2-coalescer-compares-state-instead-of-counting-marks) | Strong | pairs with 1 |
| 3 | [Pop buttons resolved in Rust](#3-pop-buttons-resolved-in-rust) | Strong | — |
| 4 | [One input interface for the engine](#4-one-input-interface-for-the-engine) | Worth exploring | easier after 1 |
| 5 | [One owner for settings; one sources module](#5-one-owner-for-settings-one-sources-module) | Worth exploring | — |
| 6 | [Action router for focus resolution](#6-action-router-for-focus-resolution) | Speculative | — |

Suggested order: 1, then 2 (Presence's destroy timer is the first user of the timer collection), then 3. Candidate 4 is cheaper once 1 has added a second kind of engine output. 5 and 6 can go any time.

## 1. Presence module for window show/hide

**Status:** done. Settled: (1) Presence lives in `crates/core`, pure, with `now` passed in. (2) `reveal_from_edge` stays a core input that changes `shown`. (3) Rust clears the region 1 s after `shown` turns false (the longest CSS transition is 280 ms), cancelled if shown again. (4) The 1100 px width is checked by a test in `src-tauri/src/window.rs`, not generated. (5) The cold pop is out of scope.

**Files:** `crates/core/src/lib.rs` (`snapshot` computes `shown`), `src-tauri/src/lifecycle.rs`, `src-tauri/src/watcher.rs` (`STRIP`, `STRIP_WANTED`, `CONTEXT`, edge strip placement, `WM_DPICHANGED`), `src-tauri/src/window.rs` (`LAST_REGION`, `place`, `ScaleFactorChanged`), `src-tauri/src/lib.rs` (`WindowShell::update`), `ui/src/capsule/Capsule.tsx` and `geometry.ts` (empty hit rects when tucked).

**Problem.** The core decides whether the capsule is shown, but acting on that is spread across five files:

- `Lifecycle` keeps its own copy of `shown` and its own timer (a `tokio::time::sleep` of `DESTROY_AFTER` guarded by an epoch counter), outside the core's `next_deadline`. None of it is tested; it needs an `AppHandle`.
- The edge strip is switched through a global (`STRIP_WANTED`) that the watcher thread polls.
- Hiding relies on the UI sending `[]` to `set_hit_rects`, so a webview that is slow, broken or being destroyed leaves the old region clickable.
- `shown` reaches `Lifecycle` inside the coalesced `UiState`, so creating the webview can wait up to 250 ms behind the 4-per-second limit, on top of the 590–690 ms cold pop recorded in decision 5.
- `apply(true)` checks `get_webview_window("main")` and then spawns `window::create`. Two calls before the window exists can both spawn a create.
- The top centre of the primary monitor is worked out twice: `window::place` uses Tauri's `primary_monitor`, and the strip uses `MonitorFromPoint(0,0)`. DPI changes are handled on two paths too. The 1100 px window width appears in `tauri.conf.json`, `geometry.ts` (`WINDOW_WIDTH`) and `index.css` (`transform-origin: 550px`).

**Direction.** A `Presence` module in core: `observe(shown, now) -> Vec<PresenceEffect>`, where the effects are `CreateWebview`, `DestroyWebview`, `EdgeStrip(bool)` and `ClearRegion`. It owns `destroy_at`, which joins `next_deadline`, so the 10-second delay is driven by the same clock as everything else. The engine passes presence effects straight to the shell, not through the coalescer. `src-tauri` keeps only the Win32 side, with one helper for the primary monitor's top edge used by both the window and the strip.

**What gets tested.** Core tests drive `now` by hand: hide, then show within 10 s (no destroy); hide for 10 s (destroy); show twice quickly (one create); show while a destroy is pending (cancelled). The race and the timer stop being Tauri-only.

**Constraints.**
- Decision 3: the UI still reports the union of the old and new rects during a transition, because only it knows when the CSS transition ends. Rust takes over only "empty when hidden".
- Decision 5: keep the 10-second delay, the native edge strip and the "start with an empty region, paint tucked, slide in" sequence.
- Rule 3 (every UI update goes through the coalescer) is about webview updates. Presence effects are window commands, not UI updates, but say so in a comment, or add a line to AGENTS.md, so nobody "fixes" it later.

**Open questions.**
1. Does Presence live in `crates/core` (pure, time passed in) or `crates/server` (next to the engine)? Core keeps one clock and one `next_deadline`.
2. Should the 2-second edge reveal (`reveal_from_edge`) move into Presence, or stay a core input that changes `shown`?
3. Does Rust clear the region the moment `shown` turns false, or only after the tuck animation? Clearing at once cuts the animation short, because the region clips drawing.
4. One source for the 1100 px width: generate it into the UI from `tauri.conf.json`, or check it in a test?
5. Is fixing the cold pop part of this work (for example, starting the WebView2 browser process early), or a separate task?

## 2. Coalescer compares state instead of counting marks

**Files:** `crates/core/src/lib.rs` (15 `coalescer.mark(now)` calls, `next_deadline`, `take_update`), `crates/core/src/timing.rs` (`Coalescer`).

**Problem.** `Coalescer` is small and well tested, but bugs live where it is called: every mutator must remember to call `mark`, and every new timer must be added to `next_deadline` by hand (`store.next_expiry`, pop `until`, undo `until`, `hover_until`, `coalescer.due`). Planned work adds more of both: hover-to-expand delays and provider visibility timers.

**Direction.** `take_update` builds `snapshot(now)` and sends it only if it differs from `last_sent` (`UiState` already derives `PartialEq`). The `mark` calls go away. Separately, collect the deadlines into one `Timers` value that both `tick` and `next_deadline` read, so adding a timer is one change.

**Caveat.** Comparing decides *whether* to send, not *when* to wake. Fields that change with time alone (the clock, "Working · 6 min", expiry) still need a deadline so the loop wakes up to notice them; that is what `Timers` is for. The 250 ms rate limit stays.

**What gets tested.** "A mutation that forgot to mark" stops being possible rather than merely tested. The existing coalescer tests in `crates/core/src/tests.rs` mostly survive; mark-specific ones become "changed state is sent, unchanged state is not".

**Open questions.**
1. Is building a snapshot on every loop turn cheap enough? It is under a kilobyte (decision 2), but measure with `scripts/measure-idle.ps1`, because idle CPU is the budget.
2. Does the comparison include the clock string, or does the clock get its own minute deadline so `snapshot` can be compared without it?
3. Should `Timers` hold named fields or a small keyed collection? Named fields are clearer; a collection suits providers added later.

## 3. Pop buttons resolved in Rust

**Files:** `crates/core/src/view.rs` (`primary_action`, `ActionView.kind: &'static str`), `ui/src/capsule/Capsule.tsx` (`PopView` finds primary, secondary and dismiss, falling back to `dismissPop`), `ui/src/App.tsx` (the keyboard handler repeats the dismiss lookup and fallback), `ui/src/types.ts` (a hand-copied mirror of the view types), `crates/cli/src/lib.rs` (its own "first non-dismiss is primary" rule).

**Problem.** What each button does is decided in four places, and the TypeScript copies are untested because there is no Vitest yet. `ActionView.kind` re-spells the protocol's serde names as strings. Nothing checks `types.ts` against the Rust types, so the two can drift silently.

**Direction.** `PopView` carries resolved slots: `primary`, `secondary`, `dismiss` (an action id, or "close the pop"). `ItemView` carries its click action the same way. The UI renders the slots and sends back the id; the keyboard handler uses `dismiss` directly. Then either generate `types.ts` (ts-rs or specta), or add a test that serialises sample `UiState`s to JSON that the UI build type-checks against.

**What gets tested.** Core tests cover the slot rules (no primary, a dismiss action, two actions), and the TS has no logic left to test.

**Constraints.** Strengthens decision 2: the UI never decides what a press does. Rule 5 is unaffected. `UiState` is not part of the protocol, so no decision record is needed. Adding a code generator is a dependency choice worth a line in the PR.

**Open questions.**
1. Generate types (a new dependency) or check JSON samples (no dependency, weaker)?
2. Does the CLI's "primary" rule describe the same thing as the UI's? If so, it should use the core's function. If it means something different, rename it.

## 4. One input interface for the engine

**Files:** `crates/server/src/engine.rs` (`Command` enum, one method and one match arm per intent, the empty `Effect::Callback` arm in `run_effect`), `src-tauri/src/lib.rs` (one `#[tauri::command]` per intent, `generate_handler!`), `ui/src/bridge.ts` (one function per intent), `crates/core/src/lib.rs` (`Effect::Callback { conn, .. }`).

**Problem.** The engine is shallow: its interface is as wide as the core's. One new UI intent means six edits: a bridge function, a Tauri command, a `generate_handler!` entry, an engine method, a `Command` variant and a match arm. `Effect::Callback` carries a pipe connection id out of the core, so the engine has to catch it before the shell sees it (hence the empty arm). The engine has no tests of its own; it is covered only by `crates/cli/tests/end_to_end.rs`, which uses real time and `thread::sleep`.

**Direction.** `core.apply(Input, now) -> Vec<Output>`, where `Input` is one serde enum (UI intents, pipe messages, watcher events) and `Output` separates `Reply`/`Callback` (to connections), `Effect` (to the shell) and, after candidate 1, presence effects. The engine routes outputs by type. Optionally, a single generic Tauri command carries UI inputs. Test the engine in its own crate using `tokio::time::pause()`.

**Open questions.**
1. One generic Tauri command, or keep one per intent for clearer IPC permissions? Tauri's capability system scopes per command.
2. Does `Input` include pipe messages, or do those keep their own path because they need a reply?
3. Is this worth doing before the planned providers land? They add the next batch of inputs.

## 5. One owner for settings; one sources module

**Files:** `src-tauri/src/lib.rs` (`settings: Mutex<Settings>` beside the core's copy; the tray reads the shell's copy), `crates/server/src/config.rs` (saves visibility only), `crates/core/src/settings.rs` (`category`), `crates/core/src/view.rs` (`source_label`, `icon_for`), `crates/cli/src/hook.rs` (`CLAUDE_CODE`). Also `pipe_name()` in both `crates/server/src/lib.rs` and `crates/cli/src/client.rs`, and `parent_pid` in `crates/cli/src/lib.rs` beside the process walk in `crates/server/src/system.rs`.

**Problem.** Settings have two owners that can drift: the tray's check marks come from the shell's copy, not from `UiState.mode`. Knowledge about each source (category, label, icon, the "claude-code / codex / wrap" names) is repeated across crates, so adding a source means finding every list.

**Direction.** A `SettingsStore` in `crates/server` owns load, change and save, and tells the engine when settings change; the tray reads `UiState`. One `sources` module holds categories, labels and icons. `pipe_name` comes from `crates/protocol` (it is part of the contract already, per decision 1) and both sides use it. The process-tree walk lives in one place.

**Open questions.**
1. Does `pipe_name` belong in `crates/protocol`? Moving a function is not a wire change, but AGENTS.md treats the protocol crate as the public contract, so decide whether a decision record is needed.
2. Should the `sources` module live in protocol (shared by the CLI) or core (UI concerns like icons)? Probably names in protocol and labels and icons in core.
3. Should every setting be saved, not just visibility? That depends on which settings exist by milestone 8.

## 6. Action router for focus resolution

**Files:** `crates/server/src/pipe.rs` (`resolve_focus_pids`), `crates/server/src/system.rs` (`window_owner`, `find_window_owner`, `focus_process_window`).

**Problem.** The pipe server, which should only move messages, resolves the window that owns a pid on every upsert with a focus action: a `CreateToolhelp32Snapshot` plus `EnumWindows`, blocking on a tokio worker. Claude Code's hooks send these often. At press time the process tree is walked again. The pure `find_window_owner` is tested, but the rule "resolve while the client is still alive" sits in `pipe.rs`, covered only end to end with a fake pid.

**Direction.** An action router in `crates/server` owns both steps, binding at push and running at press, with the process table behind a port: Win32 in the app, a fake in tests. Do the Win32 work in `spawn_blocking`, or cache it per connection.

**Why speculative.** No bug has shown up from this, and the blocking cost hasn't been measured. Measure upsert latency under a burst of hook calls first; if it is negligible, the gain is mostly tidiness.

**Open questions.**
1. Is the blocking call measurable? Time 20 upserts per second (the per-connection limit) with focus actions.
2. Cache per connection, or per pid with a short lifetime?

## Not candidates

`crates/protocol`, `crates/server/src/framing.rs` and `crates/core/src/store.rs` are already deep and well tested. The fullscreen check in `watcher.rs` (`foreground_is_fullscreen`) could become a pure function of the window rect and class so it can be tested; that is small enough to do alongside candidate 1.
