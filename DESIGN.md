# Qynetik Capsule — Design System

> The visual and interaction language for Qynetik Capsule. Read this before building or changing any UI.
> Design source of truth: the "Qynetik Capsule Test Design" canvas (Claude Design). When this file and the canvas disagree, ask before guessing.

**Direction in one line:** Windows 11, redesigned. Keep what makes it recognisably Windows (Segoe UI Variable, the Windows accent colour, Fluent icons, toast structure, subtle buttons), and fix what makes it feel flat: deeper surfaces, stronger hierarchy, more space, fewer boxes.

---

## 1. Principles

1. **One hero per surface.** Each widget has one big thing (a number, a title, album art). Everything else is small and quiet.
2. **Restraint over decoration.** No gradients, no cards inside the capsule, no outlines on rows, no icon tiles for decoration. Space separates things, not borders.
3. **Colour means something.** White text at four strengths does the work. The accent marks *now*, *progress*, *active* and *needs you*. Album art is the only other source of colour.
4. **Glow is a whisper.** Allowed only on slider handles (soft) and behind album art (blurred art colour). Nothing else glows.
5. **Whole things are buttons.** A notification, an agent row or an event row is one large target. Avoid small pill buttons inside them.
6. **It belongs to Windows.** If Windows 11 has a pattern (toast, subtle button, toggle switch, pips pager), use its structure and refine its look. Never invent a new pattern where Windows has one.

---

## 2. Design tokens

Define these once as CSS custom properties in `ui/src/styles/tokens.css`. Components use tokens only — no raw hex values in components.

```css
:root {
  /* Surface */
  --cap-bg: rgba(14, 14, 18, 0.94);        /* the capsule fill; solid CSS, NOT Mica/Acrylic */
  --cap-edge: rgba(255, 255, 255, 0.16);    /* 0.5px hairline on the bottom edge */
  --cap-shadow: 0 20px 50px rgba(0, 0, 0, 0.45);

  /* Text (Fluent-derived levels on dark) */
  --text-1: #FFFFFF;                        /* primary, heroes, titles */
  --text-2: rgba(255, 255, 255, 0.786);     /* secondary, body */
  --text-3: rgba(255, 255, 255, 0.544);     /* tertiary, captions, metadata */
  --text-4: rgba(255, 255, 255, 0.36);      /* disabled, finished items */

  /* Fills */
  --fill-subtle: rgba(255, 255, 255, 0.06); /* shortcut tiles, quiet buttons */
  --fill-hover: rgba(255, 255, 255, 0.07);  /* hover on subtle/glyph buttons */
  --fill-hover-strong: rgba(255, 255, 255, 0.10);
  --track: rgba(255, 255, 255, 0.16);       /* slider and progress tracks */
  --ring-track: rgba(255, 255, 255, 0.14);
  --divider: rgba(255, 255, 255, 0.16);     /* 0.5px only */

  /* Accent — set at runtime from Windows (UISettings accent colour) */
  --accent: #60CDFF;                        /* fallback: Windows default dark-mode accent */
  --selection: color-mix(in srgb, var(--accent) 14%, transparent); /* flat selection */
  --selection-strong: color-mix(in srgb, var(--accent) 18%, transparent); /* active toggle tile, app tile */

  /* Semantic */
  --warning: #FCE100;                       /* weather lightning only */

  /* Type */
  --font-display: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
  --font-text: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
  --font-icons: "Segoe Fluent Icons", "Segoe MDL2 Assets";

  /* Shape */
  --r-capsule: 32px;   /* expanded capsule bottom corners */
  --r-shoulder: 24px;  /* drooping shoulders on the expanded capsule */
  --r-peek-shoulder: 16px;
  --r-control: 10px;   /* glyph buttons, tiles, inputs */
  --r-row: 14px;       /* selection rows */
  --r-art: 14px;       /* large album art; 8px for small art in the peek */

  /* Motion (Fluent curves and durations) */
  --ease-out: cubic-bezier(0, 0, 0, 1);      /* entering, growing */
  --ease-in: cubic-bezier(0.33, 0, 0.67, 1); /* leaving, shrinking (Fluent easy-ease; a pure accelerate curve looks like a snap on short shrinks) */
  /* Spring curves: generate linear(...) values from the recipe in section 7 */
  --spring-expand: /* damping 0.75, 280ms */;
  --spring-pop: /* damping 0.8, 240ms */;
  --spring-settle: /* critically damped; 180–220ms */;
  --dur-ack: 100ms;
  --dur-fast: 160ms;
  --dur-normal: 220ms;
  --dur-slow: 280ms;   /* the longest UI transition allowed; nothing exceeds 300ms */
}
```

### Typography scale

| Role | Font | Size / weight | Colour | Used for |
|---|---|---|---|---|
| Hero | Display | 72 / 600, letter-spacing −0.03em | text-1 | Clock |
| Hero small | Display | 48 / 600, −0.02em | text-1 | Temperature |
| Ring value | Display | 26 / 600 | text-1 | Timer inside its ring |
| Title | Display | 15–18 / 600 | text-1 | Song title, notification title, widget titles |
| Body | Text | 13–14 / 400–600 | text-2 | Event names, row titles |
| Caption | Text | 12 / 400 | text-3 | Metadata, times, app names |
| Accent caption | Text | 12 / 600 | accent | "Now", "Now · until 15:20" |

Rules: always `font-variant-numeric: tabular-nums`. No uppercase labels. No more than three sizes inside one widget. Sentence case everywhere.

### Spacing

8px grid. Common values: 4, 8, 12, 14 (row padding), 16, 24, 40.
- Expanded capsule padding: 24px vertical, 40px horizontal.
- Gap between widgets: 40px. No dividers between widgets.

---

## 3. The capsule

### Shape
- Hangs flush from the top edge of the primary monitor: square top, rounded bottom.
- **Shoulders:** the top corners curve outward into the screen edge (like Bloom). Draw each shoulder as a small square of `--cap-bg` with a radial mask so it is seamless:

```css
.shoulder { position: absolute; top: 0; width: var(--r-shoulder); height: var(--r-shoulder); background: var(--cap-bg); }
.shoulder.left  { left: calc(-1 * var(--r-shoulder));
  mask: radial-gradient(circle at 0 100%, transparent calc(var(--r-shoulder) - 0.5px), #000 var(--r-shoulder)); }
.shoulder.right { right: calc(-1 * var(--r-shoulder));
  mask: radial-gradient(circle at 100% 100%, transparent calc(var(--r-shoulder) - 0.5px), #000 var(--r-shoulder)); }
```

- Edge: `box-shadow: var(--cap-shadow), inset 0 -0.5px 0 var(--cap-edge);` — bottom hairline only (side lines would show a seam at the shoulders).
- **Material: solid CSS fill, no `backdrop-filter`.** Windows Mica/Acrylic fills the whole window rectangle and can't follow the pill shape. Do not enable `window-vibrancy` effects for the capsule.

### Sizes

| State | Size | Bottom radius | Shoulder |
|---|---|---|---|
| Peek · idle | 120 × 36 | 18 | 16 |
| Peek · music | 280 × 44 | 22 | 16 |
| Peek · dual | 240 × 44 + 44 × 44 bubble, 24px apart, joined by an arch | 22 each | 16 outer · 12 arch |
| Pop · agent | 340 × 64 | 28 | 16 |
| Pop · undo | 300 × 44 | 22 | 16 |
| Power menu | 220 × 232 | 24 | 16 |
| Power countdown | 340 × 60 | 28 | 16 |
| Pop · message | 360 × 64–84 | 28 | 16 |
| Expanded | 1000 × 176 | 32 | 24 |

Widget slots inside the expanded capsule: small = **280 × 128**, wide = **600 × 128**. A screen holds 1–3 widgets (e.g. 280 + 280 + 280, or 600 + 280).

---

## 4. States

### Peek (always visible in Auto mode)
- **Idle:** time only, 15/600 Display.
- **One thing:** it takes the whole pill. Music = 28px art (radius 8) + title + audio bars. No controls in the peek.
- **Dual:** main pill plus a 44px bubble on the right with a progress ring (accent, 2.5px stroke). Clicking the bubble swaps the two.
  - The two shapes are joined by **one round arch**, not two touching shoulders (those meet in a sharp point). Put a 24 × 22 piece of `--cap-bg` in the 24px gap, with a semicircle cut from its bottom. The arch's sides meet the pills' straight sides at the point where their bottom corners start, so the outline flows as one curve:

```css
.dual-arch { position: absolute; top: 0; width: 24px; height: 22px; background: var(--cap-bg);
  mask: radial-gradient(circle at 50% 100%, transparent 11.5px, #000 12px); }
```
  - The pill keeps only its left outer shoulder and the bubble only its right one.
- **Music on hover:** pill grows to 400 × 56 and shows artist plus previous / pause / next as glyph buttons.

### Pops (grow out of the peek, about 5s, then shrink back)
- **Agent needs you:** app tile (36px, radius 10, `--selection-strong` background, accent icon) + title + detail + chevron. The whole card is the button and brings the terminal forward. Stays until handled.
- **Message (other apps):** avatar (36px circle) + sender + "App · now" + 2-line preview. Whole card opens the **app** (not a specific chat — Windows doesn't allow that).
- **Details off:** app tile + app name + "New message".
- **No quick reply.** Windows doesn't let one app use another app's reply action. Never design a reply box for third-party notifications.

### Expanded (hover the peek 300ms; collapse 600ms after leaving)
Three screens: **Your widgets** (left) · **Main** (centre, default) · **System** (right). Pager: 4px dots at `--text-3` opacity 0.36, active = 16 × 4 bar in white, centred 14px below the capsule.

---

## 5. Components

### Glyph button (Fluent subtle button, refined)
36 × 36, radius 10, transparent, icon `--text-2`. Hover: `--fill-hover`, icon white. The primary glyph in a group (e.g. Pause) may sit on `--fill-hover` permanently. Icons 16–18px.

### Selection row
The "needs attention / now" row. Flat `--selection` background, radius 14, height 48, padding 0 14. No border, no shadow, no dot. Accent icon on the left, accent action text on the right ("Open terminal ›"). The entire row is a link/button.

### Slider
- Track 2px, `--track`; fill 2px accent.
- Handle: **20 × 12 pill**, accent, `box-shadow: 0 0 10px color-mix(in srgb, var(--accent) 35%, transparent)`. Nothing inside the handle.
- Value and icon sit **on the right side** (icon `--text-3` 14px, value `--text-2` 12px).
- Use a real `<input type="range">` laid over the visuals (opacity 0) for keyboard and screen readers.
- Song position: right side shows `1:18 / 3:24` (elapsed in text-2, length in text-3). **Hide the whole slider if the player doesn't report position/length.**

### Toggle switch (dark mode)
Fluent switch: 36 × 20 track, radius 10. On = accent track with dark 12px knob on the right. Label to the right in 13/600 text-2. Real `<input type="checkbox" role="switch">`.

### Shortcut tile (opens Windows Settings)
40 × 36, radius 10, `--fill-subtle`, icon 17px text-2, plus a tiny ↗ badge (7px, top-right, text-3) so it never looks like a toggle. Hover `--fill-hover-strong`.
- Wi-Fi, Bluetooth, Display → open the matching Settings page.
- Power → **button** with a tiny ▾ badge. It opens the **power menu**, which grows out of the capsule (220 × 232):
  - **Lock** and **Sign out**, then a divider, then **Sleep**, **Restart** and **Shut down**. Rows are 36px high, radius 10, with a 16px icon and a 13px label.
  - **Lock** and **Sleep** happen at once, because nothing is lost.
  - **Sign out**, **Restart** and **Shut down** close your apps, so they don't ask "Are you sure?". Instead the capsule morphs into a 5-second countdown ("Signing out in 5 · Save your work, or cancel") with a white **Cancel** button. Esc also cancels.
  - Windows calls: lock `LockWorkStation`, sign out `ExitWindowsEx(EWX_LOGOFF)`, sleep `SetSuspendState`, restart and shut down `ExitWindowsEx` or `InitiateShutdown`. Never force-close apps; let Windows ask them to save.

### Undo instead of confirmation
Dismissing a pop removes it at once and morphs the capsule into "Notification dismissed · **Undo**" (300 × 44) for 4 seconds. Delete or Backspace dismisses with the keyboard. Only actions that can't be undone use a countdown (see the power menu).

### Progress
- **Determinate ring** (timer, builds with real progress): accent stroke, round caps, track `--ring-track`. 3px on large rings, 2.5px on small.
- **Indeterminate bar** (AI agents — they report no percentage): 2px track with a short accent segment that slides; always paired with elapsed time ("Working · 6 min"). Never invent a percentage.

### Audio bars
Four 3px white bars, radius 2, heights 6/14/9/16 at rest. CSS keyframe animation that runs **only while playing** (`animation-play-state: paused` otherwise) and is removed entirely when the peek isn't showing music. Never analyse real audio. If milestone 5 shows a cost, replace with a still icon.

### Album art
Real cover image, radius 14 (large) / 8 (small). Large art gets a glow: a copy of the art (or its dominant colour) behind it, `filter: blur(24px); opacity: 0.45; inset: 8px`. Placeholder when no art: flat dominant colour or `--fill-subtle` with a music glyph.

### Divider
`height: 0.5px; background: var(--divider)`. Only inside a widget (e.g. calendar between "now" and "up next"). Never between widgets.

---

## 6. Widgets

| Widget | Slot | Contents |
|---|---|---|
| Weather | small | Icon 40px (white, warning-yellow lightning) + 48px temp + condition / "City · H / L"; hourly row of 5 (caption time, 15/600 temp; "Now" in accent). |
| Clock | small | 72px time, 13px date in text-3. |
| Calendar | small | Accent caption "Now · until 15:20", 16/600 title, location caption; divider; 2 upcoming rows (name text-2, time text-3). |
| Agent board | wide | Selection row for the session needing you; working sessions with indeterminate bar + elapsed time; finished sessions dimmed to text-3/text-4. |
| Focus timer | small | 108px ring with time inside; "Focus" + "Session 2 of 4" (Pomodoro mode); pause / reset glyphs. |
| Now playing | wide | 104px art with glow; title / "Artist · App"; prev / pause / next glyphs; song slider. |
| System | small | Volume and brightness sliders; dark mode switch; Wi-Fi, Bluetooth, Display shortcuts; Power menu button. |

Default screens: **Main** = Weather · Clock · Calendar. **Your widgets** = Agent board + Focus timer. **System** = Now playing + System.

---

## 7. Motion

All motion is CSS `transform` / `opacity` / `clip-path` inside one fixed, transparent window (see the spec: one window with a click-through region). No JavaScript animation loops.

The reference implementation is the **"Motion v2"** artboard on the design canvas. It supersedes the first "Motion prototype".

### Rules

1. **Every animation has a stated purpose:** feedback, orientation or continuity. If you can't name one, don't animate.
2. **Under 300 ms.** No UI transition runs longer.
3. **Interruptible and redirectable.** A new state change mid-animation retargets from the current position; it never waits for the old animation to finish. CSS transitions do this natively, so never queue state changes behind `transitionend`.
4. **Springs for shape changes.** Shapes move on spring curves expressed as CSS `linear()` easing generated from a damped spring (fallback `cubic-bezier` where `linear()` is unsupported). Growing may overshoot up to 3%; shrinking never overshoots.
5. **Spatial consistency.** Everything grows from, and shrinks back to, the top centre along the same path. A pop returns to the exact peek it grew from.
6. **Grow toward the final state.** Entering content starts at `translateY(-6px) scale(0.96)` and settles to `none`. Never animate from `scale(0)`.
7. **Acknowledge at once, elastically.** Hovering the capsule stretches the shape itself (20 px wider and 6 px taller; the ¼ bubble 6 px each way) on an elastic spring that overshoots by a few pixels and settles, while text and icons keep their size. Never scale the whole capsule like an image. Leaving eases back without a bounce. Buttons react on press, not release.
8. **Respond to intent, not timers.** The peek expands when the pointer *slows* over it (speed under about 0.25 px/ms, then 60 ms), or after 180 ms of dwell. A pointer passing through quickly does nothing. Leaving the expanded view collapses it after a 300 ms grace period.
9. **Frequent means fast.** Switching screens happens often, so it is a 160 ms slide of 16 px in the direction of the dots, with no spring.
10. **Continuity between pops.** A second pop arriving while one is showing morphs the shape in place; it never collapses to the peek first.
11. **Reduce motion.** When Windows "Animation effects" is off (`prefers-reduced-motion`), shapes change instantly, content cross-fades in 120 ms, and the audio bars stop.

### Timings

| Transition | Duration | Curve | Purpose |
|---|---|---|---|
| Hover stretch | 300 ms | Spring, damping 0.45 (about 20% of the stretch, 4 px) | Feedback |
| Hover release | 220 ms | Critically damped spring | Continuity |
| Idle → music | 220 ms | Spring, damping 0.9 (no visible overshoot) | Feedback |
| Music → idle | 180 ms | Critically damped spring | Continuity |
| Peek → pop | 240 ms | Spring, damping 0.8 (about 2% overshoot) | Feedback |
| Pop → pop (replace) | 220 ms | Spring, damping 0.85 | Continuity |
| Pop → peek | 180 ms | Critically damped spring | Continuity |
| Peek → expanded | 280 ms | Spring, damping 0.75 (about 3% overshoot) | Orientation |
| Expanded → peek | 220 ms | Critically damped spring | Continuity |
| Screen switch | 160 ms | `--ease-out`, slide 16 px | Orientation |
| Content in | 150 ms fade, 220 ms settle, starting at 30% of the shape's duration | `--ease-out` | Continuity |
| Content out | 80 ms | linear | — |

Spring curve recipe (generate once at build time, store as CSS variables): for damping ζ < 1, `x(t) = 1 − e^(−ζωt)(cos ω_d t + (ζω/ω_d) sin ω_d t)` with `ω = 5.3 / (ζT)`; for critical damping, `x(t) = 1 − e^(−ωt)(1 + ωt)` with `ω = 7.4 / T`. Sample 29 points over the duration T into `linear(...)`.

Pops return to the peek they came from after about 5 seconds (agent pops stay until handled). Shoulders scale with the shape so the capsule always reads as one piece.

---

## 8. Icons

- In the app, use **Segoe Fluent Icons** (ships with Windows 11) via `--font-icons`, so icons match the OS exactly. Common glyphs: Play `\E768`, Pause `\E769`, Previous `\E892`, Next `\E893`, Volume `\E767`, Brightness `\E706`, Wi-Fi `\E701`, Bluetooth `\E702`, Power `\E7E8`. Verify each code against Microsoft's Segoe Fluent Icons reference before using it.
- One weight everywhere. Icons inherit `currentColor`.
- Never draw third-party logos. App icons for notifications come from Windows with the notification.

---

## 9. Accessibility

- Real elements: `<button>`, `<a>`, `<input type="range">`, `<input type="checkbox" role="switch">`. Never clickable `<div>`s.
- Every icon-only control has an `aria-label`. Decorative SVGs get `aria-hidden="true"`.
- Text contrast: text-3 (0.544) is the minimum for any readable text; text-4 is only for finished or disabled items.
- Visible focus: `box-shadow: 0 0 0 2px rgba(255,255,255,0.5)` on `:focus-visible`.
- Everything reachable by keyboard: hotkey opens expanded, arrows switch screens, Tab moves, Enter activates, Esc closes.
- Render all third-party text (titles, notifications) as plain text, never HTML.

---

## 10. Performance rules (the <1% idle budget)

- No `backdrop-filter` anywhere in the capsule.
- No JS timers or `requestAnimationFrame` in the UI. The clock updates via a Rust event once a minute.
- Only `transform` and `opacity` animate. The audio-bar animation is the only looping animation and runs only while music plays.
- Blur is allowed only on the album-art glow (static, not animated).
- Rust coalesces UI updates to at most 4 per second.

---

## 11. Do and don't

| Do | Don't |
|---|---|
| Let space separate widgets | Add cards, borders or dividers between widgets |
| Use the accent for now / progress / active / needs you | Use the accent for decoration or labels |
| Make the whole row or card the button | Put small pill buttons inside notifications |
| Show elapsed time for agents | Show a fake percentage |
| Draw Wi-Fi/Bluetooth as shortcuts with ↗ | Draw them like toggles |
| Hide the song bar if position is unknown | Show a bar that never moves |
| Keep one hero per widget | Make several things big |
| Use flat translucent accent for selection | Use outlines, dots, gradients or glows for selection |

---

## 12. Copy

- Sentence case, short, no exclamation marks.
- Say what's happening and what to do: "Claude needs you" / "Approve file edit · capsule".
- Times: 24-hour (`14:40`, `Thu 07:30`). Separators: ` · `.
- Placeholders in mockups use `[Song title]`, `[Artist]`; never invent real-looking data in shipped UI.

---

## 13. Known platform constraints

- The pill covers a small strip of maximised windows (e.g. the middle of a browser tab bar). Auto visibility and Peek on demand are the mitigation; keep the idle peek as narrow as possible.
- Other apps' notifications still show Windows' own toast too, unless the user turns off banners for that app. The setup guide must say so.
- Windows notification access needs package identity (MSIX). If unavailable, hide message pops entirely rather than showing a broken state.