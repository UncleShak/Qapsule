# 12. A Focus screen and lively motion

Date: 2026-11-04 · Status: proposed (demo on the `fork-integration` branch)

## Context

Tanatswa's earlier notch app has a study timer that knows which module you are working on, a per-module study log with a week chart and a streak, and springier motion. Qapsule has the engine it lacks: Rust owns the state, the webview is disposable and the idle cost is close to zero. The demo asks what Qapsule looks like with that feel on top, without breaking AGENTS.md:

- Rust owns all state, so the timer and the log can't live in the UI.
- No JS timers or animation loops, so the UI can't count seconds itself.
- Every UI update goes through the coalescer.
- A new reserved source is a protocol change.

## Options

1. **Port the old React timer as it is.** It keeps time in the UI with `setInterval` and saves to `localStorage`. Breaks rules 1 and 2, and the log would be lost with the webview. Rejected.
2. **A provider that pushes the timer as an ordinary activity.** Peek and pops come for free, but provider activities are pop-only and never reach the peek (decision 0006), and the log and the module chip still need a home. Rejected.
3. **A focus module in the core, alongside music.** The timer runs on the `now` callers pass in, like every other timer in the core. It is a peek candidate the way music is, and the expanded view gets a second screen. Chosen.

## Choice

- **Core** (`crates/core/src/focus.rs`):
  - The timer counts down from 15, 25 or 50 minutes (default 25), or counts up. Start, pause, reset and length are commands; the length can't change while a session runs.
  - The phases are idle, running, paused and done. A finished countdown pops "Focus session done" with a **Start another** button. A count-up of at least 5 minutes counts as a session when it is reset.
  - The module chip cycles through modules seen before, then module codes found in the calendar card, then no module. On start, the class on now (or starting within 10 minutes) picks the module, unless one was picked by hand in the last 10 minutes.
  - Module codes are 3 or 4 capitals, an optional space or dash, 3 digits and an optional capital (`COS 214`, `WTW-285`, `FRN 104Y`). Matching is a hand-written scanner, so no regex crate is needed.
- **Peek:** while running or paused, the timer is a peek candidate placed just before music: `Focus · COS 214` with the time as its caption and a progress line. `ItemView` gains an optional `caption` for this.
- **Seconds:** while the capsule is shown and the timer runs, the core marks the coalescer once per whole second (1 update a second, under the 4-a-second limit). While tucked away it wakes only for the finish and the minute save, so a hidden Qapsule stays asleep.
- **Holding pops:** while a session runs, pops other than `needs_input` go straight into the notification history. An agent that needs you still interrupts.
- **The log:** focused seconds are credited by day and module, and kept for 180 days. The engine saves it to `%APPDATA%\Qapsule\study.toml` once a minute while running and on every change (written beside the file, then renamed over it). The stats are this week's bars (Monday to Sunday), up to four modules ranked by time, today's total, sessions today, and a streak of days with at least 5 minutes. Module colours come from Rust in first-seen order.
- **Screens:** `ExpandedView` gains `screen` (`focus` or `main`) and `focus`. The left and right arrows, the wheel and the pips move between screens. The view opens on the screen it was last left on, and clicking the timer in the peek opens it on Focus.
- **Protocol:** `focus:` joins the reserved source prefixes, so a pipe client can't pose as the timer. No new message or error code.
- **Motion:** `[appearance] motion = "calm" | "lively"`, default calm, also in the settings window. Lively keeps every duration and swaps the spring curves for bouncier ones (`scripts/springs.ts` generates both sets). The UI sets `data-motion` on `<html>`, and reduced motion still wins.

## Consequences

- Qapsule gains these study features at about the same idle cost: nothing runs while no session is going, and a running session hidden from view wakes the engine once a minute.
- `study.toml` is the first file Qapsule writes besides the config. It holds module codes and seconds only, never notification text.
- Two screens instead of the spec's three for now. The right-hand System screen is still to come and slots onto the same track.
- Open for Shaq: whether lively motion belongs in Qapsule at all, the default countdown length, and whether holding pops during focus should be a setting.
