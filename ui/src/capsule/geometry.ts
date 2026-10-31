// Sizes from DESIGN.md section 3, in CSS pixels. The window is 1100 wide; the capsule hangs from its top centre.

export const WINDOW_WIDTH = 1100;
/** The window's real width: Rust makes it narrower on a monitor narrower than WINDOW_WIDTH. */
export function windowWidth(): number {
  return window.innerWidth > 0 ? window.innerWidth : WINDOW_WIDTH;
}
export const SHOULDER = 16;
/** Room around the capsule for its shadow; clicks there are caught too, so keep it small. */
const SHADOW_SIDE = 8;
const SHADOW_BELOW = 14;
/** How much a shape stretches while the pointer is over it. The bubble stretches less. */
const HOVER = { w: 20, h: 6, bubble: 6 };

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One rounded-bottom shape. Shoulders curve its top corners into the screen edge. */
export interface Box {
  x: number;
  w: number;
  h: number;
  r: number;
  /** The radius of the curve joining a top corner to the screen edge. */
  shoulder: number;
  left: boolean;
  right: boolean;
}

export type LayoutKind = "idle" | "one" | "music" | "music_hover" | "dual" | "pop" | "undo" | "expanded";

export interface Layout {
  kind: LayoutKind;
  /** The pill, or the ¾ part of a dual peek. */
  main: Box;
  /** The ¼ bubble of a dual peek; otherwise hidden behind `main`. */
  side: Box;
  /** Where the arch joining the two sits; hidden behind `main` unless dual. */
  archX: number;
  dual: boolean;
}

function centred(w: number, h: number, r = h / 2 > 28 ? 28 : h / 2, shoulder = SHOULDER): Box {
  return { x: windowWidth() / 2 - w / 2, w, h, r, shoulder, left: true, right: true };
}

const EXPANDED_SHOULDER = 24;
const EXPANDED_FULL = 1000;

/** The expanded capsule's width: 1000, or less if the window is too narrow to hold it
 *  with its shoulders and shadow on both sides. */
export function expandedWidth(width: number): number {
  return Math.min(EXPANDED_FULL, width - 2 * EXPANDED_SHOULDER - 2 * SHADOW_SIDE);
}

const SIZES: Record<Exclude<LayoutKind, "dual" | "expanded">, [number, number]> = {
  idle: [120, 36],
  one: [280, 44],
  music: [280, 44],
  music_hover: [400, 56],
  pop: [340, 64],
  undo: [300, 44],
};

/** The shapes for a state. `hovered` stretches the peek shapes a little wider and taller;
 *  the expanded capsule never stretches. */
export function layout(kind: LayoutKind, hovered = false): Layout {
  const CENTRE = windowWidth() / 2;
  // A lone song doesn't stretch: under the pointer it becomes the 400 × 56 hover form.
  if (kind === "music" || kind === "music_hover") {
    const [w, h] = SIZES[hovered ? "music_hover" : kind];
    const main = centred(w, h);
    return { kind, main, side: main, archX: CENTRE - 12, dual: false };
  }
  if (kind === "expanded") {
    const main = centred(expandedWidth(windowWidth()), 176, 32, EXPANDED_SHOULDER);
    return { kind, main, side: main, archX: CENTRE - 12, dual: false };
  }
  const dw = hovered ? HOVER.w : 0;
  const dh = hovered ? HOVER.h : 0;
  if (kind === "dual") {
    const pill = 240 + dw;
    const bubble = 44 + (hovered ? HOVER.bubble : 0);
    const h = 44 + dh;
    const x = CENTRE - (pill + 24 + bubble) / 2;
    return {
      kind,
      main: { x, w: pill, h, r: h / 2, shoulder: SHOULDER, left: true, right: false },
      side: { x: x + pill + 24, w: bubble, h: bubble, r: bubble / 2, shoulder: SHOULDER, left: false, right: true },
      archX: x + pill,
      dual: true,
    };
  }
  const [w, h] = SIZES[kind as Exclude<LayoutKind, "dual" | "expanded">];
  const main = centred(w + dw, h + dh);
  return { kind, main, side: main, archX: CENTRE - 12, dual: false };
}

/** The area to keep clickable: the shapes at their hovered size, plus their shadow.
 *  Using the hovered size means hovering never has to change the window region.
 *  A tucked capsule has none: a small native strip at the top edge (Rust) brings it out,
 *  and keeps working while this webview doesn't exist. */
export function hitRects(kind: LayoutKind, tucked: boolean): Rect[] {
  if (tucked) return [];
  const l = layout(kind, true);
  const around = (b: Box): Rect => {
    const x = b.x - (b.left ? b.shoulder : 0) - SHADOW_SIDE;
    const right = b.x + b.w + (b.right ? b.shoulder : 0) + SHADOW_SIDE;
    return { x, y: 0, w: right - x, h: b.h + SHADOW_BELOW };
  };
  // One rect across both parts of a dual peek, so the gap under the arch isn't cut away.
  return l.dual ? union([around(l.main)], [around(l.side)]) : [around(l.main)];
}

/** The smallest rect list covering both, used while a shape is changing size. */
export function union(a: Rect[], b: Rect[]): Rect[] {
  const all = [...a, ...b];
  const x = Math.min(...all.map((r) => r.x));
  const right = Math.max(...all.map((r) => r.x + r.w));
  const h = Math.max(...all.map((r) => r.y + r.h));
  return [{ x, y: 0, w: right - x, h }];
}
