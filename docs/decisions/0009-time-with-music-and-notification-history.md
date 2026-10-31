# 9. The time with music, and a notification history

Date: 2026-10-31 · Status: accepted

## Context

Two things the Archon found in daily use:

- While a song plays, the peek shows only the song (decision 8), so the time disappears from the peek for hours.
- A dismissed or expired notification is gone for good; there is nowhere to look it up again. Meanwhile the expanded view's centre column spends its whole height on the clock.

## Options

For the time:

1. Put the clock inside the music pill, between the title and the bars. Cheap, but crowds a 280 px pill.
2. **At rest, music moves to the ¼ bubble as a spinning cover and the ¾ pill shows the time; the full music pill appears for a few seconds when a song starts or changes.** Chosen.

For the history:

1. A separate history screen. Needs the pager (milestone 8).
2. **The last three notifications in the bottom half of the main screen's clock column.** Chosen.

## Choice

> **"Music at rest" below is superseded by decision 10** (clock in the music peek). The notification history stands.

**Music at rest** (amends decision 8, Peek):

- When a song starts playing or changes, and music would be the peek's top item, the peek shows the full music pill for 5 s (a "song notice"). It replaces the song-change pop card: the notice follows the `music` pop setting (pop off: no notice) and Do Not Disturb, waits while the expanded view is open, and never shows over a pop or the undo bar. The first song seen after start gets no notice, as before.
- Otherwise music is always the ¼: the ¾ is the top activity, or the time when there is none. Music with the time is `Peek::Dual` with an idle-time main and a music side.
- The ¼ cover is round and turns slowly (one turn per 8 s) while playing; still when paused; never under reduced motion. Like the audio bars it is a CSS loop that runs only while playing.
- Hovering a peek whose only item is music (time + cover at rest, or the notice) grows it into the 400 × 56 music form with controls, as in decision 8. Clicking still opens the expanded view; the ¼ swap does not apply to the time.

**Notification history:**

- The core keeps the last three notifications that left the screen (dismissed, expired or handled), newest first. A pop that replaces one with the same key replaces its history entry. Song notices are not kept; pops that never showed (pop off) are not kept.
- Each entry keeps what the pop showed (honouring `details`), its icon and state, the local time it arrived, and the pop's primary action. Clicking an entry runs that action through the same routes as the pop: open the app for Windows notifications, focus the window or open the link for agents. Entries without an action (calendar) do nothing. A callback to a client that has disconnected is dropped. No new action kinds (AGENTS.md rule 5).
- Memory only, like all notification text (decision 6): never written to disk, cleared on restart, not returned by `list`.
- The main screen's centre column is split: the clock and date in the top half, the three entries below (icon, title, time), or "No notifications yet".

## Consequences

- The time is visible in the peek whenever nothing more important is happening, even with music.
- One more looping animation while music plays (the cover), to be measured against the budget with the bars.
- The song-change pop card goes away; users who want no song notice turn the `music` pop off as before.
- No protocol change.
