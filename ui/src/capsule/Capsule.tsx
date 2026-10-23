// The capsule: peek, pop and undo states drawn from the core's UiState.
// It holds no state of its own beyond what animation needs.

import { type CSSProperties, type TransitionEvent, useEffect, useRef } from "react";
import { dismissPop, press, setHitRects, swapPeek, undo } from "../bridge";
import type { ItemView, PopView, UiState } from "../types";
import { Arch, Shape } from "./Shape";
import { Glyph, stateGlyph } from "./Glyph";
import { type Layout, type LayoutKind, type Rect, hitRects, layout, union } from "./geometry";

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function kindOf(state: UiState): LayoutKind {
  if (state.pop) return "pop";
  if (state.undo) return "undo";
  if (state.peek.kind === "one") return "one";
  if (state.peek.kind === "dual") return "dual";
  return "idle";
}

interface Shown {
  kind: LayoutKind;
  seq: number | null;
  hovered: boolean;
}

/** The spring for a change of shape (DESIGN.md section 7). */
function motion(from: Shown | null, to: Shown): [string, number] {
  if (from && from.kind === to.kind && from.seq === to.seq && from.hovered !== to.hovered) {
    // Stretch elastically toward the pointer; ease back without bouncing.
    return to.hovered ? ["--spring-hover", 300] : ["--spring-collapse", 220];
  }
  return change(from?.kind ?? null, to.kind, from?.kind === "pop" && from.seq !== to.seq);
}

function change(from: LayoutKind | null, to: LayoutKind, replaced: boolean): [string, number] {
  if (from === to) return replaced ? ["--spring-replace", 220] : ["--spring-settle", 180];
  if (to === "pop") return from === "undo" ? ["--spring-replace", 220] : ["--spring-pop", 240];
  const size = (k: LayoutKind) => ({ idle: 0, one: 1, dual: 1, undo: 2, pop: 3 })[k];
  if (from !== null && size(to) > size(from)) return ["--spring-grow", 220];
  return ["--spring-settle", 180];
}

/** `hovered` is the pointer being over the capsule: a purely visual stretch, so it lives here. */
export function Capsule({ state, hovered }: { state: UiState; hovered: boolean }) {
  const kind = kindOf(state);
  const tucked = !state.shown;
  const stretched = hovered && !tucked;
  // The shape stretches on hover; the content keeps its size and place.
  const l = layout(kind, stretched);
  const base = layout(kind);

  // Remember the last shape, to pick the spring and to cover both shapes while changing.
  // The spring is picked only when the shape changes, so an unrelated update (the
  // clock, a progress value) can't swap the curve under a running animation.
  const now: Shown = { kind, seq: state.pop?.seq ?? null, hovered: stretched };
  const previous = useRef<Shown | null>(null);
  const chosen = useRef<[string, number]>(["--spring-settle", 180]);
  const p = previous.current;
  if (!p || p.kind !== now.kind || p.seq !== now.seq || p.hovered !== now.hovered) {
    chosen.current = motion(p, now);
  }
  const [curve, duration] = chosen.current;
  useEffect(() => {
    previous.current = now;
  });

  // The clickable region covers the old and new shapes until the change finishes.
  const shownRects = useRef<Rect[]>([]);
  const pending = useRef<Rect[] | null>(null);
  useEffect(() => {
    const next = hitRects(kind, tucked);
    const first = shownRects.current.length === 0;
    if (first || reducedMotion()) {
      setHitRects(next);
      pending.current = null;
    } else {
      setHitRects(union(shownRects.current, next));
      pending.current = next;
    }
    shownRects.current = next;
  }, [kind, tucked]);
  const onTransitionEnd = (e: TransitionEvent) => {
    if (e.propertyName === "transform" && pending.current) {
      setHitRects(pending.current);
      pending.current = null;
    }
  };

  const style = {
    "--shape-ease": `var(${curve})`,
    "--shape-dur": `${duration}ms`,
    "--content-delay": `${Math.round(duration * 0.3)}ms`,
    "--tuck": `${-(l.main.h + 24)}px`,
    // Keeps the content centred in the taller, stretched shape.
    "--nudge": `${(l.main.h - base.main.h) / 2}px`,
  } as CSSProperties;

  return (
    <div className={`capsule ${tucked ? "tucked" : ""}`} style={style} onTransitionEnd={onTransitionEnd}>
      {tucked && <div className="edge-strip" />}
      <div className="lift">
        <div className="fill" aria-hidden="true">
          <Shape box={l.side} />
          <Arch x={l.archX} />
          <Shape box={l.main} />
        </div>
        <div className="nudge">
          <Content state={state} layout={base} />
        </div>
      </div>
    </div>
  );
}

function Content({ state, layout: l }: { state: UiState; layout: Layout }) {
  const area = (box = l.main): CSSProperties => ({ left: box.x, width: box.w, height: box.h });
  const kind = l.kind;
  // A new key remounts the content, which replays its entrance.
  if (state.pop) {
    return (
      <div className="content" style={area()} key={`pop-${state.pop.seq}`}>
        <Pop pop={state.pop} />
      </div>
    );
  }
  if (state.undo) {
    return (
      <div className="content undo" style={area()} key="undo">
        <span className="undo-text">Notification dismissed</span>
        <button type="button" className="undo-button" onClick={() => undo()}>
          Undo
        </button>
      </div>
    );
  }
  if (state.peek.kind === "idle") {
    return (
      <div className="content" style={area()} key="idle">
        <span className="clock">{state.clock}</span>
      </div>
    );
  }
  const side = state.peek.kind === "dual" ? state.peek.side : null;
  return (
    <>
      <div className="content" style={area()} key={`${kind}-main-${state.peek.main.source}/${state.peek.main.id}`}>
        <Item item={state.peek.main} />
      </div>
      {side && (
        <div className="content" style={area(l.side)} key={`side-${side.source}/${side.id}`}>
          <Bubble item={side} />
        </div>
      )}
    </>
  );
}

function caption(item: ItemView): string | null {
  if (item.progress !== null) return `${Math.round(item.progress * 100)}%`;
  if (item.elapsed_min !== null && item.state === "running") return `${item.elapsed_min} min`;
  return null;
}

/** One thing happening, filling the pill (or its ¾). */
function Item({ item }: { item: ItemView }) {
  const text = caption(item);
  const inner = (
    <>
      <Glyph name={stateGlyph(item.state, item.icon)} className={`item-icon state-${item.state}`} />
      <span className="item-title">{item.title}</span>
      {text && <span className="item-caption">{text}</span>}
      {item.indeterminate && item.state === "running" && <span className="bar indeterminate" />}
      {item.progress !== null && (
        <span className="bar">
          <span className="bar-fill" style={{ transform: `scaleX(${item.progress})` }} />
        </span>
      )}
    </>
  );
  const action = item.primary_action;
  return action ? (
    <button type="button" className="item" onClick={() => press(item.source, item.id, action)}>
      {inner}
    </button>
  ) : (
    <div className="item">{inner}</div>
  );
}

/** The ¼ bubble: a progress ring, or the item's icon. Clicking swaps it with the ¾. */
function Bubble({ item }: { item: ItemView }) {
  const R = 12;
  const C = 2 * Math.PI * R;
  return (
    <button type="button" className="bubble" onClick={() => swapPeek()} aria-label={`Show ${item.title}`}>
      <svg viewBox="0 0 28 28" width="28" height="28" aria-hidden="true">
        <circle className="ring-track" cx="14" cy="14" r={R} />
        {item.progress !== null && (
          <circle
            className="ring-fill"
            cx="14"
            cy="14"
            r={R}
            strokeDasharray={C}
            strokeDashoffset={C * (1 - item.progress)}
          />
        )}
      </svg>
      <Glyph name={stateGlyph(item.state, item.icon)} className={`bubble-icon state-${item.state}`} />
    </button>
  );
}

function Pop({ pop }: { pop: PopView }) {
  const primary = pop.actions.find((a) => a.primary);
  const secondary = pop.actions.find((a) => !a.primary && a.kind !== "dismiss");
  const dismiss = pop.actions.find((a) => a.kind === "dismiss");
  const run = (id: string) => press(pop.source, pop.id, id);
  const detail = [pop.source_label, pop.subtitle].filter(Boolean).join(" · ");
  return (
    <div className="pop">
      <button
        type="button"
        className="pop-card"
        onClick={() => (primary ? run(primary.id) : dismissPop())}
        aria-label={primary ? `${pop.title}: ${primary.label}` : pop.title}
      >
        <span className={`app-tile state-${pop.state}`}>
          <Glyph name={stateGlyph(pop.state, pop.icon)} />
        </span>
        <span className="pop-text">
          <span className="pop-title">{pop.title}</span>
          <span className="pop-detail">{detail}</span>
        </span>
        {primary && <Glyph name="chevron" className="pop-chevron" />}
      </button>
      {secondary && (
        <button type="button" className="pop-secondary" onClick={() => run(secondary.id)}>
          {secondary.label}
        </button>
      )}
      <button
        type="button"
        className="glyph-button"
        aria-label={dismiss ? dismiss.label : "Close"}
        onClick={() => (dismiss ? run(dismiss.id) : dismissPop())}
      >
        <Glyph name="close" />
      </button>
    </div>
  );
}
