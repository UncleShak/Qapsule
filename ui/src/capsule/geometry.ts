// Sizes from DESIGN.md section 3, in CSS pixels. The window is 1100 wide; the capsule hangs from its top centre.

export const WINDOW_WIDTH = 1100;
const CENTRE = WINDOW_WIDTH / 2;
export const SHOULDER = 16;
/** Room around the capsule for its shadow; clicks there are caught too, so keep it small. */
const SHADOW_SIDE = 8;
const SHADOW_BELOW = 14;
/** How much a shape stretches while the pointer is over it. The bubble stretches less. */
const HOVER = { w: 20, h: 6, bubble: 6 };
/** The strip at the top edge that brings a tucked capsule out on hover. */
const STRIP = { w: 120, h: 3 };

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
  left: boolean;
  right: boolean;
}

export type LayoutKind = "idle" | "one" | "dual" | "pop" | "undo";

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

function centred(w: number, h: number): Box {
  return { x: CENTRE - w / 2, w, h, r: h / 2 > 28 ? 28 : h / 2, left: true, right: true };
}

const SIZES: Record<Exclude<LayoutKind, "dual">, [number, number]> = {
  idle: [120, 36],
  one: [280, 44],
  pop: [340, 64],
  undo: [300, 44],
};

/** The shapes for a state. `hovered` stretches them a little wider and taller. */
export function layout(kind: LayoutKind, hovered = false): Layout {
  const dw = hovered ? HOVER.w : 0;
  const dh = hovered ? HOVER.h : 0;
  if (kind === "dual") {
    const pill = 240 + dw;
    const bubble = 44 + (hovered ? HOVER.bubble : 0);
    const h = 44 + dh;
    const x = CENTRE - (pill + 24 + bubble) / 2;
    return {
      kind,
      main: { x, w: pill, h, r: h / 2, left: true, right: false },
      side: { x: x + pill + 24, w: bubble, h: bubble, r: bubble / 2, left: false, right: true },
      archX: x + pill,
      dual: true,
    };
  }
  const [w, h] = SIZES[kind];
  const main = centred(w + dw, h + dh);
  return { kind, main, side: main, archX: CENTRE - 12, dual: false };
}

/** The area to keep clickable: the shapes at their hovered size, plus their shadow.
 *  Using the hovered size means hovering never has to change the window region. */
export function hitRects(kind: LayoutKind, tucked: boolean): Rect[] {
  if (tucked) return [{ x: CENTRE - STRIP.w / 2, y: 0, w: STRIP.w, h: STRIP.h }];
  const l = layout(kind, true);
  const around = (b: Box): Rect => {
    const x = b.x - (b.left ? SHOULDER : 0) - SHADOW_SIDE;
    const right = b.x + b.w + (b.right ? SHOULDER : 0) + SHADOW_SIDE;
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
