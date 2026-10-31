# 8. Now playing

Date: 2026-10-31 · Status: accepted

## Context

Milestone 6 puts the current song in the peek (spec, Peek view; DESIGN.md, Peek · music), with optional song-change pops and previous / pause / next controls. Windows exposes media through the Global System Media Transport Controls (`GlobalSystemMediaTransportControlsSessionManager`): sessions per app, with properties (title, artist, thumbnail), playback info and timeline, all with change events. Music differs from activities: it has cover art, a moving position and controls, and it isn't something an agent or app pushes.

## Options

1. **Music as a provider activity** under a reserved source, like Windows notifications. Reuses ranking and pops, but the activity model has no art, artist or timeline, so the protocol would grow fields no pipe client may use. Rejected.
2. **Music as its own core input**, like the weather and calendar views (decision 7), ranked into the peek by the core. Chosen.
3. **Art as a data URL in the UI state.** Simple, but every coalesced update (up to 4 a second) would resend tens of kilobytes. Rejected.
4. **Art through a custom URI scheme** served from memory by the app. Chosen.

## Choice

- **Input.** A media provider (`crates/providers/src/media/`) follows the session Windows calls current, using its change events only (no polling). It sends the core `set_media(Option<MediaState>)`: app name, title, artist, art reference, playing or paused, and an optional timeline (position, length, playback rate, and when the position was read). `None` when there is no session, or it is stopped or closed. Paused music stays.
- **Peek.** The core ranks music between "running with progress" and "running without progress" (spec, Peek view priority). It has the fixed key `media:` / `now-playing`, so the ¼ swap works as for activities. When music is the only item and the pointer is over the peek (and the expanded view is closed), the music view is in its hover form with controls (DESIGN.md, Music on hover).
- **Position.** The core sends the position as of the moment each snapshot is built (position plus the time since it was read times the rate, while playing), with the length and the rate (0 when paused). The UI animates the progress line with one CSS transition to the end, scaled by the rate; no timers. No timeline from the player: no line. Reports that move the position less than 2 s from what the UI already shows send no update.
- **Pops.** A song-change pop when a song plays whose title or artist differs from the last song that played. The first song reported after start is the baseline and never pops; pause, resume and seeking never pop; a song that arrives paused pops once it plays. Undo never brings back a pop for a song that is no longer current. It uses the `music` pop setting (`category` maps `media:` to `music`; default pop on, details on). Details off: "Now playing" and the app name, in the pop and in the peek, with no cover art. The pop is built from the media state and does not enter the activity store.
- **Controls.** The UI sends `press_action("media:", "now-playing", action)` with `media.play_pause`, `media.next` or `media.previous`. The core turns these into `Effect::Provider` for the media provider, which calls the session's `TryTogglePlayPauseAsync`, `TrySkipNextAsync` or `TrySkipPreviousAsync`. These are Qapsule's own controls, not pushed buttons (AGENTS.md rule 5 is unaffected).
- **Reserved prefix.** `media:` joins `windows:` and `calendar:` in `RESERVED_SOURCE_PREFIXES` (amends decision 6), so no pipe client can impersonate the player.
- **Art.** The provider reads the thumbnail stream (refused if its size is unknown or over 10 MB, and read in chunks capped at 10 MB; images over 2048 px on a side are refused), scales it to at most 256 × 256 with `Windows.Graphics.Imaging` and encodes PNG, keeping only the current image in memory. The view carries a URL on the `qapsule-art` scheme with a content hash in the path; the app answers that scheme from memory and refuses anything else (404). A recreated webview fetches it again.
- **A hung player.** Every call to the player has a 3 s deadline (cancelled on timeout, last known state kept); buttons run on their own thread; queued events are coalesced to one per kind.
- **Privacy.** Song titles, artists and app names are never logged; errors carry fixed reasons only.

## Consequences

- One new reserved prefix: pipe clients pushing to `media:` are refused. No other protocol change.
- The webview gets a second origin (the art scheme). It serves only the current image from memory, so it reads nothing from disk and nothing the page names.
- Players that don't report a timeline (many browsers) show no progress line; players that don't publish a media session don't appear.
- The media provider always runs (the peek shows music), but costs nothing between events.
