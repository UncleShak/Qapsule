# 7. The expanded view and widget providers

Date: 2026-10-31 · Status: accepted

## Context

Milestone 6 adds the expanded view: hovering the peek opens a 1000 × 176 capsule with the main screen (weather, clock, calendar). It brings the first widget providers, which fetch from the network, and the first secret (the iCal link). Four things aren't covered by earlier decisions:

- Who owns "expanded": the UI sees the pointer, but decision 2 says Rust owns all state, and the webview may be destroyed at any time.
- How widget data reaches the UI without breaking the coalescer (AGENTS.md rule 3) or the idle budget.
- The calendar's "starts in 10 minutes" pop needs a source pipe clients can't fake.
- Where the iCal link is stored and how the user enters it, with no settings UI yet.

`SPEC.md` (300 ms hover, 600 ms collapse) and `DESIGN.md` (pointer intent, 300 ms grace) also disagree on hover timing.

## Options

1. **The UI decides when to expand** and tells Rust. Simple, but the state is lost with the webview, and the timing can't be tested without a window. Rejected.
2. **Rust decides from raw pointer moves streamed from the UI.** Correct, but sends dozens of messages per second across IPC while the pointer moves. Rejected.
3. **The UI reports a few pointer facts (over, left, slowed, clicked); the core owns the decision and every timer.** Chosen.

For widget data: as activities in reserved sources (they would rank in the peek, expire and pop, none of which fits weather), or as **separate widget state in the core**. Widget state is chosen.

For the link: in `config.toml` (the spec forbids it), a settings window (no window code exists for it), or **a CLI command writing to Credential Manager**. The CLI is chosen.

## Choice

- **Hover follows DESIGN.md.** The peek expands when the pointer slows over it (under about 0.25 px/ms, then 60 ms), or after 180 ms of dwell; it collapses 300 ms after the pointer leaves. `expand_on = "click"` in `[general]` turns hover off. The UI measures speed from `pointermove` events and sends `pointer_slowed` once per hover; it runs no timers.
- **The core owns the expanded state.** Dwell, intent and grace are core timers driven by `now`. Hover and click don't expand over a pop or the undo bar. The hotkey opens the view and keeps it open until Esc or the hotkey again, replacing the old "pin the peek out" toggle, and asks the window to take keyboard focus; closing gives focus back. Pops that arrive while the view is open wait, and their timer starts when they show.
- **Widget state lives in the core**, set by providers through narrow engine commands, and is in `UiState` only while expanded. Updates while collapsed send nothing to the UI. They go through the coalescer like everything else.
- **The core tells providers when the main screen is visible.** Weather refreshes on open if older than 30 minutes and otherwise does nothing. The calendar refreshes on open if older than 15 minutes, and every 15 minutes in the background so it can pop. Providers wake only at their next real deadline.
- **New reserved prefix `calendar:`** (decision 0006). The calendar's pops are `Info` activities in `calendar:events`, through the provider path. A pipe `upsert` or `end` naming a `calendar:` source is refused with `invalid_message`; `clear` and `list` still accept it. The settings category is `calendar`, with `minutes_before` (default 10) alongside `pop` and `details`.
- **The iCal link lives in Windows Credential Manager** under `Qapsule/calendar-ical`, written by `qapsule calendar set-link` (from stdin, never argv), removed by `clear-link`; `status` reports only whether it is set. A small `crates/secrets` crate is shared by the CLI and the provider. The link and event text are never logged.
- **Network calls added:** Open-Meteo geocoding and forecast (only with a city set), and the user's iCal link (only with a link set). No other hosts.

## Consequences

- Pipe clients see one new refusal: pushing to a `calendar:` source.
- The hotkey's meaning changes: it now opens the expanded view instead of only bringing the peek out. In Peek on demand mode the result is the same, one step further.
- `UiState` grows an `expanded` part. It is internal to the app (not the pipe protocol), so no protocol version change.
- Hover timing is now tested in Rust; the speed threshold itself lives in the UI and needs a manual check.
- Later providers (media, timer, downloads) reuse the visibility signal and the widget-state path. A provider that pops picks its own reserved prefix and is noted in 0006.
- `SPEC.md`'s hover timings are updated to match this record.
