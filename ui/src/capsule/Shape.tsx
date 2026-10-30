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

export function Shape({ box }: { box: Box }) {
  const { x, w, h, r, shoulder } = box;
  const k = shoulder / SHOULDER_BASE;
  // A hidden shoulder tucks inside the body, where it can't be seen.
  const leftShoulder = box.left ? x - shoulder : x + 1;
  const rightShoulder = box.right ? x + w - k : x + w - shoulder - 2;
  return (
    <>
      <div className="piece body" style={at(x, 0, w / BASE, (h - r + 1) / BASE)} />
      <div className="piece strip" style={at(x + r - 1, h - r, (w - 2 * r + 2) / BASE, r / BASE)} />
      <div className="piece corner-left" style={at(x, h - r, r / CORNER)} />
      <div className="piece corner-right" style={at(x + w - r, h - r, r / CORNER)} />
      <div className="piece shoulder-left" style={at(leftShoulder, 0, k)} />
      <div className="piece shoulder-right" style={at(rightShoulder, 0, k)} />
    </>
  );
}

/** The single round arch joining the dual peek's pill and bubble (DESIGN.md section 4).
 *  It overlaps each neighbour by a pixel so their edges can't leave a seam. */
export function Arch({ x }: { x: number }) {
  return <div className="piece arch" style={at(x - 1, 0)} />;
}
