# Plan: the expanded view (milestone 6)

Status: approved · 2026-10-31 · Built overnight by the Academy, branch `expanded-view` (off `live-notifications`)

Background: `docs/SPEC.md` (Expanded view, Widgets and providers, Configuration, Security and privacy), `DESIGN.md` (sections 3, 4, 6, 7, 9, 10), `docs/decisions/0007-expanded-view-and-widget-providers.md`.

## Goal

Hovering the peek (or clicking it, or the hotkey) opens the expanded capsule on the main screen: weather, a large clock with the date, and the current and next calendar events, with a "starts in 10 minutes" pop. Leaving collapses it; Esc closes a hotkey-opened view. Everything stays inside the idle budget: nothing runs while the view is closed except the calendar's 15-minute refresh for pops.

**Out of scope tonight:** the left and right screens, the pager, edit mode, now playing, timer, system controls, config reload, the tray items of milestone 6. Ideas found along the way go to the "Deferred" list at the bottom, not into the work.

## Decisions (settled with the Archon on 2026-10-31)

Settled by the Archon:

1. **Scope is milestone 6:** main screen only. No pager until a second screen exists.
2. **Hover follows DESIGN.md, not the spec's fixed delays.** The peek expands when the pointer slows over it (under about 0.25 px/ms, then 60 ms) or after 180 ms of dwell. A quick pass does nothing. Leaving collapses it after a 300 ms grace period. `SPEC.md` is updated to match.
3. **The iCal link is entered with the CLI:** `qapsule calendar set-link` reads it from stdin (never argv, so it stays out of shell history) and stores it in Windows Credential Manager. Also `qapsule calendar clear-link` and `qapsule calendar status` (says whether a link is set, never prints it).
4. **Git:** one local commit per phase on `expanded-view`, after its gate passes. Nothing is pushed.

Settled by Plato (the Archon may override in the morning):

5. **Rust owns the expanded state.** The core decides expanded or not. The UI only reports what the pointer did: `pointer(over)` (exists), `pointer_slowed` (sent once per hover, when the measured speed first drops under 0.25 px/ms) and `expand_click`. Dwell (180 ms), intent delay (60 ms) and collapse grace (300 ms) are core timers driven by `now`, tested with hand-driven time. The UI measures speed from `pointermove` events only, with no timers or animation frames.
6. **`expand_on = "hover" | "click"`** in `[general]` (spec). In click mode, hover never expands; a click on the peek that isn't on a button with its own action expands.
7. **The hotkey opens the expanded view and keeps it open until Esc or the hotkey again** (spec). It replaces today's "pin the peek out" toggle. Opening by hotkey gives the window keyboard focus; closing gives focus back to the window that had it. Hover-opening never takes focus.
8. **No expanding over a pop or the undo bar.** While a pop or "Notification dismissed" shows, hover and click don't expand (the user is about to press it); the hotkey still does.
9. **Pops wait while the view is open.** A pop that arrives while expanded is queued; its 5-second timer starts only when it is shown after the view collapses. Needs-input pops stay until handled, as today.
10. **Widget data is not activities.** Weather and calendar state live in the core as widget state, set by providers through narrow engine commands, and appear in `UiState` only while expanded. A widget update while collapsed changes nothing on screen and sends no UI update.
11. **Provider visibility comes from the core** (spec, Widgets and providers): when the main screen opens or closes, the engine tells providers. Weather: refresh on open if older than 30 minutes, otherwise nothing. Calendar: refresh on open if older than 15 minutes, and every 15 minutes in the background for pops. Rust timers at the next real deadline only; never a fixed-rate loop.
12. **The calendar's pops use a new reserved source prefix, `calendar:`** (source `calendar:events`), following decision 0006. Pipe clients can't push to it. Settings category `calendar`, with `minutes_before` (default 10). This is a protocol-visible behaviour change, recorded in decision 0007.
13. **Layout config follows the spec's `[[screens]]` format**, parsed now so edit mode can use it later. Tonight only the centre screen is used, and it always shows weather, time and events; the parsed list supplies the weather `city`. No city set: the widget says "Add your city to config.toml".
14. **Network and data:** weather from Open-Meteo (geocoding once per city, then forecast; metric units). HTTP with `reqwest` using the Windows TLS stack (no OpenSSL), 10-second timeouts, no retry loops; a failure keeps the last good data and shows it as stale. iCal parsing is our own small parser plus `rrule` (recurrences) and `chrono-tz` (time zones). `https://` and `webcal://` links only (webcal becomes https).
15. **Secrets:** a new crate, `crates/secrets`, wraps Credential Manager (`CredWriteW`, `CredReadW`, `CredDeleteW`) for the CLI and the provider. The link and event text are never logged; HTTP errors log the status code only, never the URL.
16. **Times and copy:** 24-hour times, date as `Sat 31 Oct`, sentence case, DESIGN.md section 12.

## Phases

Each phase ends at a gate: the full checks pass, then the `archon` agent approves. Then Plato commits the phase. Routing follows `.claude/academy.md`.

Full checks (every gate):

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bun run --cwd ui build
```

### Phase 0 — Paper (Plato) · done before the night starts

- This plan; `docs/decisions/0007-expanded-view-and-widget-providers.md`; an amendment line in 0006 for the new prefix.
- `.claude/agents/archon.md`, the Archon's stand-in for the night.
- Clear `.claude/agora.md` and point it here.
- **Gate:** the Archon approves this plan (in person, before sleeping).

### Phase 1 — Expanded state in the core (aristotle; socrates examines)

`crates/core`, `crates/server/src/engine.rs`, tests.

- Settings: `expand_on` (hover | click, default hover), the `[[screens]]` layout (types and `city` only; unknown widget types warn and are dropped; each screen keeps 1–3 widgets, otherwise defaults), `calendar.minutes_before`.
- Core inputs: `pointer_slowed`, `expand_click`, `toggle_expanded` (hotkey), `close_expanded` (Esc). The existing `set_pointer` starts the dwell and collapse timers. Decisions 5–9 hold.
- Every new timer is added to `next_deadline` and cleared in `tick` (architecture review, item 2: this is where bugs live).
- Expanded counts as brought out for presence (`shown`), in every visibility mode.
- `UiState.expanded: Option<ExpandedView>` with the clock, the date and the widget state (weather and events views, empty at first). `UiState.focus` (or similar): whether the window should hold keyboard focus.
- Widget state setters for providers (`set_weather`, `set_events`): mark the coalescer only while expanded.
- Provider visibility output: the engine learns when the main screen opens and closes and hands it to a new `Shell` method.
- Tests with hand-driven time: dwell expands at 180 ms, a pass under 180 ms doesn't; slowing expands 60 ms later; leaving collapses after 300 ms, re-entering cancels; click mode ignores hover; hotkey opens and stays open while the pointer leaves, Esc closes; no expand over a pop or undo (hotkey still does); a pop arriving while open waits and gets its full 5 seconds after collapse; widget updates while collapsed send no UI update; visibility changes are emitted once per open and close.

**Gate:** checks green; socrates satisfied or every question answered; archon approves.

### Phase 2 — The expanded capsule on screen (aristotle)

`src-tauri/src/{lib,watcher,window}.rs`, `ui/src/**`.

- Tauri commands and bridge functions for the new inputs. The hotkey calls `toggle_expanded`.
- Focus: when `UiState` asks for focus, the shell brings the window forward and remembers the previous foreground window; when it no longer does, focus goes back to that window if it still exists.
- Geometry: the expanded layout (1000 × 176, bottom radius 32, shoulders 24) and its hit rect. `Box` gets its own shoulder size. The window (1100 × 260) already fits; the existing width test keeps passing.
- Motion: peek → expanded on `--spring-expand` (280 ms), back on `--spring-collapse` (220 ms), content in at 30% of the shape's duration (DESIGN.md section 7). Reduced motion: instant shape, 120 ms cross-fade.
- Pointer: speed from `pointermove` (distance over the events' own timestamps), `pointer_slowed` once per hover; click mode sends `expand_click` from a click on the peek outside any action button.
- Keyboard: Esc sends `close_expanded`; Tab moves through the widgets' controls; visible focus ring (DESIGN.md section 9).
- Main screen: three 280 × 128 slots, 40 px apart, 24 px / 40 px padding. Clock widget (72 px time, 13 px date in text-3). Weather and events widgets render their empty and "not set up" states from `UiState`; real data comes in phases 3 and 4.
- No `requestAnimationFrame` loops, `setInterval` or polling `setTimeout`. The existing double-frame entrance and the transition backstop are the only timers allowed.

**Gate:** checks green; archon approves (including a read of every new UI timer and listener).

### Phase 3 — Weather (aristotle)

`crates/providers/src/weather/`, `crates/providers/Cargo.toml`, `crates/core` (weather view type only if phase 1 left it open), `src-tauri/src/lib.rs` (start the provider), `ui/src/widgets/Weather.tsx` (or similar), fixtures.

- Pure, tested parsing of saved Open-Meteo geocoding and forecast responses (fixtures in `crates/providers/samples/weather/`, captured once from the live API; no network in tests).
- WMO weather code → condition text and Fluent glyph; lightning in `--warning`.
- View: current temperature (48 px), condition, `City · H / L`, an hourly row of 5 (first is "Now" in accent).
- Refresh rules from decision 11. Failure keeps the last data, marked stale.
- UI widget per DESIGN.md section 6.

**Gate:** checks green; archon approves.

### Phase 4 — Calendar (aristotle; socrates examines)

`crates/secrets/` (new), `crates/cli` (`calendar` subcommand), `crates/protocol` (reserved `calendar:` prefix, samples), `crates/providers/src/calendar/`, `crates/core` (category mapping), `src-tauri/src/lib.rs`, `ui/src/widgets/Events.tsx` (or similar), fixtures, `docs/integrations/calendar.md`.

- `crates/secrets`: set, get, clear for the target `Qapsule/calendar-ical`. A unit test round-trips a throwaway target name and deletes it.
- CLI: `qapsule calendar set-link` (stdin, validates `https://` or `webcal://`), `clear-link`, `status`. End-to-end test for the parts that don't need the app.
- Protocol: `calendar:` added to `RESERVED_SOURCE_PREFIXES`; invalid sample `upsert-reserved-calendar-source.json`; test.
- Parser: line unfolding, `VEVENT` with `SUMMARY`, `LOCATION`, `DTSTART`/`DTEND` (UTC, floating and `TZID`), all-day events, `RRULE`, `EXDATE`, `RECURRENCE-ID` overrides, cancelled events dropped. Fixtures in `crates/providers/samples/calendar/`, including a Google Calendar style export.
- View: now (accent caption "Now · until 15:20", title, location) and the next two (name, time; `Tomorrow 09:00` / `Thu 07:30` for later days). All-day events show as "All day" and never as "now".
- Pops: `minutes_before` minutes before each event starts, an `Info` activity in `calendar:events` (source label "Calendar"), with no buttons, expiring when the event starts. Pop and details settings from `[notifications] calendar`. Event boundaries and pop times scheduled as single Rust deadlines.
- Refresh rules from decision 11. No link: the widget says "Add a calendar with `qapsule calendar set-link`".
- Guide in `docs/integrations/calendar.md` (Google: Settings → Integrate calendar → secret address in iCal format).

**Gate:** checks green; socrates satisfied or every question answered; archon approves.

### Phase 5 — Examination, docs and the morning report (socrates; aristotle; diogenes; archon)

- socrates examines the whole branch (`git diff live-notifications...expanded-view`).
- aristotle answers the questions that matter.
- diogenes: `SPEC.md` (hover timings, `expand_on`, `minutes_before`, the `calendar:` prefix), the README's network-calls list, the release checklist's manual items for the expanded view, and `ROADMAP.md` (tick only what is done).
- archon: final verification against this plan, then the morning report below.

**Gate:** checks green; archon approves.

## Rules for the night

- **Failure:** diogenes fails twice → aristotle. aristotle fails twice → Plato re-plans once. Still failing → the archon decides whether to cut the part. Cut work is stashed (`git stash push -m "cut: …"`), never deleted, and the branch is left building.
- **Independence:** phases 3 and 4 need only phases 1 and 2. If one is cut, the other still runs.
- **No scope growth.** Anything new goes to Deferred.
- **No pushes, no PRs, no changes outside the repo, nothing that needs admin.**
- The archon can't see the screen. Anything that needs eyes or hands goes on the morning checklist.

## Morning checklist (for the Archon)

- [ ] `qapsule calendar set-link` with your Google secret iCal address; add your city under `[[screens]]` in `config.toml`.
- [ ] `bun run tauri dev`: hover the peek slowly (expands), sweep past it quickly (doesn't), leave (collapses after a moment).
- [ ] Ctrl+Alt+Space opens it and takes focus; Esc closes it and your previous window has focus again.
- [ ] Weather and events look right next to the design canvas; reduced motion (Windows Animation effects off) works.
- [ ] A "starts in 10 minutes" pop for a test event.
- [ ] `scripts/measure-idle.ps1`: idle CPU and memory still within budget.
- [ ] 150% and 200% scaling.
- [ ] Hotkey: open, click into another app, press the hotkey again: that app keeps focus. Open, press Esc: focus returns to where you were.
- [ ] Weather glyphs are approximations (Segoe Fluent Icons has no weather set): clear = Brightness, one Cloud for partly cloudy/cloudy/fog, Drop for rain, a Unicode snowflake. Check them and the snowflake's size and baseline.
- [ ] The hourly weather row has no per-hour glyph (to fit 128 px), unlike DESIGN.md's table. Confirm against the canvas.
- [ ] The clock and actionless peek items are now buttons (Tab reaches them; in click mode they expand). Confirm.
- [ ] New `--font-mono` token (for the `qapsule calendar set-link` hint): add it to DESIGN.md or drop it.
- [ ] Read decision 0007; push the branch when happy.
- [ ] Note: the phase 1 commit alone (without phase 2) opens the view in the core but the old UI ignores it; test from the branch tip.

## Night log

Filled in by Plato as gates pass.

- **Phase 1 passed.** Socrates asked 5 questions, all fixed: a lost webview counts as the pointer leaving; hover behaves the same in every mode; the undo bar pauses while open; needs-input pops survive the pop-queue cap; a city change resets the weather view (cities over 100 characters are ignored). Archon approved; 91 core tests.
- **Phase 2 passed** on the second gate. The first was rejected: closing could take focus from a window clicked since opening (now only gives focus back if the capsule still has it), and a reported glyph table was missing. Archon approved the fixes.

## Deferred

Ideas found during the night that are not part of milestone 6.
