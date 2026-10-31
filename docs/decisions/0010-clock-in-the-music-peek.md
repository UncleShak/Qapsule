# 10. The clock in the music peek

Date: 2026-10-31 · Status: accepted · Supersedes the "Music at rest" part of decision 9

## Context

Decision 9 moved music to the ¼ bubble next to the time and added a 5-second song notice. In use the Archon found both worse than before: the split wastes the ¾ on the time, and the notice and spinning cover add motion for little. The time still has to be visible while music plays.

## Choice

- **The music peek keeps its shape and puts the clock in the middle:** cover (left) · clock (middle) · audio bars (right), 280 × 44. The song title is not in the peek at all; it shows only in the media view.
- **No song notice and no song-change pop.** A new song changes the cover; nothing else moves. (The `music` pop setting no longer has anything to pop; it is kept for later use.)
- **Dual peek** is for music together with downloads or running agents, ranked as in decision 8 and swappable with a click on the ¼, as for any dual peek. As the ¾, music keeps cover · clock · bars, narrower; as the ¼, a round still cover.
- **Clicking the cover** (in the full pill or the ¾) opens the **media view**: the 400 × 56 form from decision 8 with the larger cover, title and artist, previous / play-pause / next and the progress line (the progress line is in the media view only, not in the peek). The core owns it (`media_open`), like the expanded view: a second click on the cover closes it, and so does the pointer leaving it, after the 300 ms collapse grace, or the webview going away. It doesn't open over a pop or the undo bar, or while the expanded view is open. With music in the ¾ of a dual peek the media view replaces the dual shape while it is open.
- **Clicking the clock** (or anywhere in the pill that is not the cover) opens the expanded view, in click mode as in decision 7.
- **Hover** gives only the normal elastic stretch every peek has; it never opens the media view.
- The spinning cover is gone. The audio bars and progress line run only while playing, as before.

## Consequences

- The time is always in the peek, with or without music, unless an agent or download takes the ¾.
- The media view is one click away and never opens by accident.
- Decision 9's notification history is unaffected.
