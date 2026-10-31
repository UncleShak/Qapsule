// A rounded-bottom shape drawn from solid pieces that only move and scale, so a
// size change is a compositor-only transform (AGENTS.md rule 2) and the corners
// never stretch. The pieces overlap by a pixel and sit in one group with the
// capsule's opacity, so the seams can't show.

import type { CSSProperties } from "react";
import type { Box } from "./geometry";

const BASE = 100; // Body and strip are BASE×BASE squares, scaled to size.
const CORNER = 32; // Corner pieces are CORNER×CORNER, scaled by r / CORNER.
const SHOULDER_BASE = 16; // Shoulder pieces are drawn for this radius and scaled by shoulder / SHOULDER_BASE.

const at = (x: number, y: number, sx = 1, sy = sx): CSSProperties => ({
  transform: `translate(${x}px, ${y}px) scale(${sx}, ${sy})`,
});

/** DESIGN.md section 3, "Album colour wash": `radial-gradient(120% 140% at 12% 50%, ...)`
 *  over the pill. The pieces are scaled boxes, so each piece gets that one gradient mapped
 *  into its own coordinates: the pieces together draw exactly the pill's gradient, and
 *  the gradient follows the shape wherever it morphs. Static: nothing here animates. */
function washOf(box: Box, px: number, py: number, sx: number, sy: number): CSSProperties {
  const cx = box.x + 0.12 * box.w;
  const cy = box.h / 2;
  const f = (n: number) => Math.round(n * 100) / 100;
  const gradient =
    `radial-gradient(${f((1.2 * box.w) / sx)}px ${f((1.4 * box.h) / sy)}px ` +
    `at ${f((cx - px) / sx)}px ${f((cy - py) / sy)}px, ` +
    `color-mix(in srgb, var(--art-tint) 26%, transparent), transparent 70%)`;
  return { backgroundImage: gradient };
}

/** `wash` draws the same silhouette as a clear layer carrying the album wash instead of
 *  the solid fill. It stacks over the solid shape, inside the same group. */
export function Shape({ box, wash = false }: { box: Box; wash?: boolean }) {
  const { x, w, h, r, shoulder } = box;
  const k = shoulder / SHOULDER_BASE;
  // A hidden shoulder tucks inside the body, where it can't be seen.
  const leftShoulder = box.left ? x - shoulder : x + 1;
  const rightShoulder = box.right ? x + w - k : x + w - shoulder - 2;
  const piece = (name: string, px: number, py: number, sx: number, sy = sx, hidden = false) => (
    <div
      className={`piece ${wash ? "wash-piece " : ""}${name}`}
      style={{
        ...at(px, py, sx, sy),
        ...(wash ? washOf(box, px, py, sx, sy) : null),
        // A tucked shoulder would double the tint over the body.
        ...(wash && hidden ? { opacity: 0 } : null),
      }}
    />
  );
  return (
    <>
      {piece("body", x, 0, w / BASE, (h - r + 1) / BASE)}
      {piece("strip", x + r - 1, h - r, (w - 2 * r + 2) / BASE, r / BASE)}
      {piece("corner-left", x, h - r, r / CORNER)}
      {piece("corner-right", x + w - r, h - r, r / CORNER)}
      {piece("shoulder-left", leftShoulder, 0, k, k, !box.left)}
      {piece("shoulder-right", rightShoulder, 0, k, k, !box.right)}
    </>
  );
}

/** The single round arch joining the dual peek's pill and bubble (DESIGN.md section 4).
 *  It overlaps each neighbour by a pixel so their edges can't leave a seam. With `wash`
 *  it carries the ¾ pill's wash (`pill`), which has faded out by here, so no seam shows. */
export function Arch({ x, wash = false, pill }: { x: number; wash?: boolean; pill?: Box }) {
  return (
    <div
      className={`piece ${wash ? "wash-piece " : ""}arch`}
      style={{ ...at(x - 1, 0), ...(wash && pill ? washOf(pill, x - 1, 0, 1, 1) : null) }}
    />
  );
}
