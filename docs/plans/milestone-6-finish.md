# Plan: finish milestone 6

Status: built; phase E checks pending · 2026-10-31 · run phase by phase, the Archon checks each phase in the app before the next starts.

Branch `milestone-6` off `expanded-view`, one commit per phase. Merged into `main` with `expanded-view` once the Archon's checks pass.

## Goal

Close milestone 6, "My day at a glance" (docs/ROADMAP.md): now playing in the peek with song-change pops, the tray's Do Not Disturb and Start with Windows, config reload, the narrow-display fix, and the manual checks left from the expanded view.

## Decisions

1. **Now playing is not an activity.** Music never enters the activity store or the protocol. A media provider sends the core a `MediaView` (like weather), and the core places it in the peek at its spec priority: needs input, failed, running with progress, **music**, then running without progress, done, info, time. Only one session shows: the one Windows calls current. Paused music stays in the peek while the session exists; a stopped or closed session leaves it. Decision 0008.
2. **Event-driven only.** The provider uses `GlobalSystemMediaTransportControlsSessionManager` events (current session, media properties, playback info, timeline). No polling. Until something subscribes the provider costs nothing extra; it always runs, because the peek shows music.
3. **Progress without timers.** The view carries position, length, whether it is playing and the moment the position was read. The UI turns that into one linear CSS transition of `transform: scaleX` to the end of the song, restarted on each update; paused, it holds still. No timeline (many browsers): no progress line (spec, Media position).
4. **Cover art through a custom URI scheme.** The provider reads the thumbnail, scales it in Rust with WinRT imaging (`Windows.Graphics.Imaging`, no new crate) to at most 256 × 256 PNG, and keeps the latest one in memory. The view carries `qapsule-art://art/<hash>`; the app answers that scheme from memory. The UI state stays small; a recreated webview fetches it again. No art: DESIGN.md's placeholder (music glyph on `--fill-subtle`).
5. **Song-change pops** use the existing `music` pop setting (`pop` default true, `details` default true). A pop fires when the title or artist changes while playing, never on pause/resume or seeking, and not on the first song seen at start. Details off: "Now playing" and the app name. Held while the expanded view is open, like other pops; DND holds them.
6. **Controls are the app's own** (AGENTS rule 5 doesn't apply: nothing is pushed). Built-in actions `media.play_pause`, `media.next`, `media.previous` go from the UI through `press_action` with source `media:` to the provider. `media:` becomes a reserved source prefix (amend decision 0006).
7. **Music on hover** (DESIGN.md): when the peek shows music in the full pill and the pointer is over it, the core sets the peek to its 400 × 56 hover form with artist and previous / pause / next. The core decides from `pointer(over)`, not the UI. In the ¾ slot, music shows art and title only.
8. **Audio bars** are DESIGN.md's CSS keyframes, running only while playing, removed when the peek isn't showing music, stopped under reduced motion.
9. **Tray:** a Do Not Disturb checkbox (writes `notifications.do_not_disturb`, like the visibility mode) and a Start with Windows checkbox. Start with Windows writes the value `Qapsule` under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no admin) with the quoted path of the running exe; the checkbox shows whether that value exists and points at this exe. Off by default.
10. **Config reload:** the `notify` crate watches the config file's folder; changes to `config.toml` are debounced 300 ms with a single Rust timer, then the file is reloaded and applied: settings to the engine, the weather city and calendar settings to their providers, the tray checkboxes updated. A file that doesn't parse keeps the current settings and logs the warning (no content). Clears the Deferred "config reload" item.
11. **Narrow displays:** the window is at most the primary monitor's width. The UI lays the expanded view out at `min(1000, window width − 2 × 24 shoulder)`, the clock column keeps its width and the weather and events columns shrink, with ellipsis. Clears the Deferred item.
12. **Unchanged:** expanded-view behaviour, the calendar and weather providers (except reload), the protocol crate except the reserved prefix.

## Phases

Each phase ends green (`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `bun run --cwd ui build`), is committed, and waits for the Archon's check.

- **Phase A: tray and config reload.** Decisions 9 and 10. Aristotle. Socrates examines the reload (concurrency, persisted state).
- **Phase B: narrow displays.** Decision 11. Aristotle.
- **Phase C: media provider and core.** Decisions 1–7 in Rust: provider with a fake-session seam for tests, core `set_media`, peek ranking, song-change pops, media actions, art scheme, reserved `media:` prefix. Aristotle; Socrates examines (OS APIs, threading, a new URI scheme).
- **Phase D: music peek UI.** Decisions 3, 4, 7, 8: the 280 × 44 peek, ¾ form, hover controls, progress line, art and placeholder, audio bars, reduced motion. Aristotle.
- **Phase E: wrap-up.** Docs (DESIGN.md:159 heading, `--font-mono` in DESIGN.md, SPEC, ROADMAP ticks), the Archon's manual checklist below, merge.

## Manual checklist (the Archon)

Carried from docs/plans/expanded-view.md, plus this plan's items. Ticked as each phase is checked.

- [ ] Ctrl+Alt+Space opens and takes focus; Esc closes and focus returns. Hotkey, click another app, hotkey again: that app keeps focus.
- [ ] A "starts in 10 minutes" pop for a test event.
- [ ] Weather glyphs and the snowflake's size; the hourly row has no per-hour glyph.
- [x] `scripts/measure-idle.ps1` within budget (checked 31 Oct, release build, 120–300 s): idle 0.11% CPU / 14 MB avg; capsule shown with music playing 0.43% (0.88% before stepping the bars and progress line); paused 0.01%; memory up to 133 MB of 150.
- [x] 150% and 200% scaling; a display narrower than 1064 logical px (phase B). (checked 31 Oct)
- [x] Credential Manager: no leftover `Qapsule/test-*` entries (checked 31 Oct).
- [x] Phase A: DND holds pops and survives a restart; editing config.toml applies without a restart (checked 31 Oct).
- [ ] Start with Windows from a release build starts it after sign-in (dev builds grey it out: their UI needs the Vite server).
- [x] Phase C–D: Spotify and a browser video: title, art, progress (none for the browser if it reports none), controls, song-change pop, pause keeps it, closing the player removes it. (checked 31 Oct)
</content>
</invoke>
