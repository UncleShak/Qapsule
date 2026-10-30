# 3. One window with a click-through region

Date: 2026-10-23 · Status: accepted

## Context

The capsule changes size often: idle peek (120 × 36), one item, dual status, pops (340 × 64) and later the expanded view (1000 × 176). Resizing a native window on every change flickers and cannot follow a spring curve smoothly. But a large transparent window would swallow clicks meant for the apps below it.

## Options

1. **Resize the window** for every state. Simple hit-testing, but flicker, and animations jump.
2. **One fixed window** at the largest size, with the shape animated in CSS, and a window region (`SetWindowRgn`) matching what is drawn, so clicks elsewhere pass through.
3. **Per-pixel alpha (layered) window**, which passes clicks through transparent pixels. WebView2 does not render into a layered window this way.

## Choice

Option 2. The window is 1100 × 260 (logical pixels), frameless, transparent, always on top, a tool window (out of the taskbar and Alt+Tab), centred against the top edge of the primary monitor. The UI knows the shape it draws and reports its rectangles (plus a small margin for the shadow) with `set_hit_rects`; Rust turns them into the window region. While the shape changes, the region covers both the old and new shapes, and shrinks to the new one when the transition ends. When the capsule is tucked away, the region is a 120 × 3 strip at the top edge, which brings it back on hover.

Shapes animate with transforms only (AGENTS.md rule 2): each shape is built from solid pieces that move and scale, so the rounded corners never stretch.

## Consequences

- No flicker; growing from peek to pop is a CSS transition.
- A few pixels around the capsule (its shadow) still catch clicks. The shadow is smaller than DESIGN.md's token so the region can stay tight.
- The region clips drawing too, so nothing may be drawn outside the reported rectangles.
- Fallback if regions misbehave on some machines: resize the window instead (option 1).
