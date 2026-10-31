// The capsule: peek, pop and undo states drawn from the core's UiState.
// It holds no state of its own beyond what animation needs.

import {
  type AnimationEvent,
  type CSSProperties,
  type ReactNode,
  type TransitionEvent,
  useEffect,
  useRef,
  useState,
} from "react";
import { dismissPop, expandClick, press, setHitRects, swapPeek, undo } from "../bridge";
import type { ItemView, PeekItem, PopView, UiState } from "../types";
import { Arch, Shape } from "./Shape";
import { Glyph, stateGlyph } from "./Glyph";
import { MusicBubble, MusicMain, MusicPeek } from "./Music";
import { Expanded } from "../widgets/Expanded";
import {
  MUSIC_FRAME,
  type Box,
  type Layout,
  type LayoutKind,
  type Rect,
  hitRects,
  layout,
  union,
  windowWidth,
} from "./geometry";

const reducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function kindOf(state: UiState): LayoutKind {
  if (state.expanded) return "expanded";
  if (state.pop) return "pop";
  if (state.undo) return "undo";
  if (state.peek.kind === "one") {
    const main = state.peek.main;
    if (main.kind === "music") return main.controls ? "music_hover" : "music";
    return "one";
  }
  if (state.peek.kind === "dual") return "dual";
  return "idle";
}

/** The tint of the cover when the music holds the peek's main shape (the full pill, the ¾
 *  or the media view); null under a pop, the undo bar or the expanded view, with the music
 *  as the ¼, or when the core sent none. */
function glowTint(state: UiState): string | null {
  if (state.expanded || state.pop || state.undo || state.peek.kind === "idle") return null;
  const main = state.peek.main;
  return main.kind === "music" ? main.art_tint : null;
}

/** The glow layers to draw. A new tint stacks over the old one and fades in over 400 ms (a
 *  CSS animation); once it has finished, `done` drops the old layer. No timers. */
function useGlow(tint: string | null): [string[], () => void] {
  const [glow, setGlow] = useState<{ tint: string | null; layers: string[] }>({
    tint,
    layers: tint ? [tint] : [],
  });
  if (glow.tint !== tint) {
    // Derived during render: no tint, or reduced motion, is instant; else keep one old layer.
    const layers = !tint ? [] : reducedMotion() ? [tint] : [...glow.layers.slice(-1), tint];
    setGlow({ tint, layers });
  }
  return [glow.layers, () => setGlow((g) => (g.layers.length > 1 ? { ...g, layers: g.layers.slice(-1) } : g))];
}

/** The pill the plain glow fits: the lone pill, or the ¾ of a dual. */
function plainGlowBox(state: UiState): Box {
  return state.peek.kind === "dual" ? layout("dual").main : layout("music").main;
}

/** One glow element for one form: the pill's own box and radius (never per shape piece),
 *  a static gradient centred on the cover. It follows the shape by translating; neither the
 *  element nor its gradient is ever scaled. The two forms cross-fade by opacity. */
function Glow({ form, rest, x, shown }: { form: "plain" | "media"; rest: Box; x: number | null; shown: boolean }) {
  const media = form === "media";
  const cover = media ? MUSIC_FRAME.art : MUSIC_FRAME.art * MUSIC_FRAME.artScale;
  const cx = MUSIC_FRAME.pad + cover / 2;
  // The media view also has the cover's own glow behind the larger cover: less colour here.
  const strength = media ? 34 : 42;
  return (
    <div
      className={`glow ${shown ? "on" : ""}`}
      style={{
        width: rest.w,
        height: rest.h,
        borderRadius: `0 0 ${rest.r}px ${rest.r}px`,
        transform: `translateX(${x ?? rest.x}px)`,
        background: `radial-gradient(ellipse 96px 60px at ${cx}px ${rest.h / 2}px, color-mix(in srgb, var(--art-tint) ${strength}%, transparent), transparent 100%)`,
      }}
    />
  );
}

const isMusic = (k: LayoutKind) => k === "music" || k === "music_hover";

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
  // The media view opening over a dual peek: the elastic grow; closing settles back.
  if (from?.kind === "dual" && to.kind === "music") {
    return to.hovered ? ["--spring-hover", 300] : ["--spring-settle", 180];
  }
  return change(from?.kind ?? null, to.kind, from?.kind === "pop" && from.seq !== to.seq);
}

function change(from: LayoutKind | null, to: LayoutKind, replaced: boolean): [string, number] {
  if (from === to) return replaced ? ["--spring-replace", 220] : ["--spring-settle", 180];
  if (to === "expanded") return ["--spring-expand", 280];
  if (from === "expanded") return ["--spring-collapse", 220];
  if (to === "pop") return from === "undo" ? ["--spring-replace", 220] : ["--spring-pop", 240];
  const size = (k: LayoutKind) =>
    ({ idle: 0, one: 1, music: 1, music_hover: 1, dual: 1, undo: 2, pop: 3, expanded: 4 })[k];
  if (from !== null && size(to) > size(from)) return ["--spring-grow", 220];
  return ["--spring-settle", 180];
}

/** `hovered` is the pointer being over the capsule: a purely visual stretch, so it lives here. */
export function Capsule({ state, hovered }: { state: UiState; hovered: boolean }) {
  const kind = kindOf(state);
  // A new webview first paints the capsule tucked (with nothing clickable), then slides
  // it in, so a recreated webview never flashes into place half-drawn.
  const [entered, setEntered] = useState(false);
  useEffect(() => {
    let inner = 0;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => setEntered(true));
    });
    return () => {
      cancelAnimationFrame(outer);
      cancelAnimationFrame(inner);
    };
  }, []);
  const tucked = !state.shown || !entered;
  const [glowLayers, glowDone] = useGlow(glowTint(state));
  // The window is narrower on a narrow monitor and Rust may resize it at any time; the
  // layout follows its width. A resize event, not a timer.
  const [width, setWidth] = useState(windowWidth);
  useEffect(() => {
    const onResize = () => setWidth(windowWidth());
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);
  // The expanded capsule never stretches.
  const stretched = hovered && !tucked && kind !== "expanded";
  // The shape stretches on hover; the content keeps its size and place.
  const l = layout(kind, stretched);
  // A song's content is laid out in its hover frame; the plain form is that under transforms.
  const base = layout(isMusic(kind) ? "music_hover" : kind);

  // Remember the last shape, to pick the spring and to cover both shapes while changing.
  // The spring is picked only when the shape changes, so an unrelated update (the
  // clock, a progress value) can't swap the curve under a running animation.
  // A song's plain pill and its media view are one shape that grows, so they share a kind
  // and the media view counts as the grown state.
  const now: Shown = {
    kind: isMusic(kind) ? "music" : kind,
    seq: state.pop?.seq ?? null,
    hovered: kind === "music_hover" || (!isMusic(kind) && stretched),
  };
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
  // The region grows when the media view opens and shrinks when its collapse settles,
  // like any other shape change.
  const hitKind: LayoutKind = kind;
  const shownRects = useRef<Rect[]>([]);
  const pending = useRef<Rect[] | null>(null);
  useEffect(() => {
    const next = hitRects(hitKind, tucked);
    const first = shownRects.current.length === 0;
    if (first || reducedMotion()) {
      setHitRects(next);
      pending.current = null;
    } else {
      setHitRects(union(shownRects.current, next));
      pending.current = next;
    }
    shownRects.current = next;
  }, [hitKind, tucked, width]);
  const settle = () => {
    if (pending.current) {
      setHitRects(pending.current);
      pending.current = null;
    }
  };
  const onTransitionEnd = (e: TransitionEvent) => {
    if (e.propertyName === "transform") settle();
  };
  // A backstop for when the webview skips frames and transitionend never arrives. One
  // timer per shape change, not a polling loop.
  useEffect(() => {
    if (!pending.current) return;
    const timer = window.setTimeout(settle, duration + 100);
    return () => window.clearTimeout(timer);
  }, [kind, tucked, width, duration]);

  const style = {
    "--shape-ease": `var(${curve})`,
    "--shape-dur": `${duration}ms`,
    "--content-delay": `${Math.round(duration * 0.3)}ms`,
    "--tuck": `${-(l.main.h + 24)}px`,
  } as CSSProperties;

  return (
    <div className={`capsule ${tucked ? "tucked" : ""}`} style={style} onTransitionEnd={onTransitionEnd}>
      <div className="lift">
        <div className="fill" aria-hidden="true">
          <Shape box={l.side} />
          <Arch x={l.archX} />
          <Shape box={l.main} />
          {/* The album colour glow (DESIGN.md section 3): one static element per form, in the
              fill group (so the bars and the clock, in the content layer, never repaint it). */}
          {glowLayers.map((tint) => (
            <div
              key={tint}
              className="glow-layer"
              style={{ "--art-tint": tint } as CSSProperties}
              onAnimationEnd={(e: AnimationEvent) => e.target === e.currentTarget && glowDone()}
            >
              <Glow form="plain" rest={plainGlowBox(state)} x={kind === "music_hover" ? null : l.main.x} shown={kind !== "music_hover"} />
              <Glow form="media" rest={layout("music_hover").main} x={l.main.x} shown={kind === "music_hover"} />
            </div>
          ))}
        </div>
        <Content state={state} rest={base} now={l} />
      </div>
    </div>
  );
}

/** Holds content over one shape. The content keeps the shape's resting size and only
 *  moves to stay centred while the shape stretches, so text never scales or drifts. */
function Slot({ rest, now, className = "", children }: { rest: Box; now: Box; className?: string; children: ReactNode }) {
  const dx = now.x + now.w / 2 - (rest.x + rest.w / 2);
  const dy = (now.h - rest.h) / 2;
  return (
    <div
      className="slot"
      style={{ left: rest.x, width: rest.w, height: rest.h, transform: `translate(${dx}px, ${dy}px)` }}
    >
      <div className={`content ${className}`}>{children}</div>
    </div>
  );
}

function Content({ state, rest, now }: { state: UiState; rest: Layout; now: Layout }) {
  // A new key remounts the content, which replays its entrance.
  if (state.expanded) {
    return (
      <Slot rest={rest.main} now={now.main} className="expanded" key="expanded">
        <Expanded state={state} expanded={state.expanded} />
      </Slot>
    );
  }
  if (state.pop) {
    return (
      <Slot rest={rest.main} now={now.main} key={`pop-${state.pop.seq}`}>
        <Pop pop={state.pop} />
      </Slot>
    );
  }
  if (state.undo) {
    return (
      <Slot rest={rest.main} now={now.main} className="undo" key="undo">
        <span className="undo-text">Notification dismissed</span>
        <button type="button" className="undo-button" onClick={() => undo()}>
          Undo
        </button>
      </Slot>
    );
  }
  if (state.peek.kind === "idle") {
    return (
      <Slot rest={rest.main} now={now.main} key="idle">
        <button type="button" className="clock" onClick={() => expandClick()}>
          {state.clock}
        </button>
      </Slot>
    );
  }
  const main = state.peek.main;
  const side = state.peek.kind === "dual" ? state.peek.side : null;
  const dual = state.peek.kind === "dual";
  const keyOf = (item: PeekItem) => `${item.source}/${item.id}`;
  return (
    <>
      {/* One key for a song's plain and hover forms, so the hover form doesn't replay the entrance. */}
      <Slot rest={rest.main} now={now.main} key={`${dual ? "dual" : "one"}-main-${keyOf(main)}`}>
        {main.kind === "music" ? (
          dual ? (
            <MusicMain music={main} clock={state.clock} />
          ) : (
            <MusicPeek music={main} clock={state.clock} open={main.controls} />
          )
        ) : (
          <Item item={main} />
        )}
      </Slot>
      {side && (
        <Slot rest={rest.side} now={now.side} key={`side-${keyOf(side)}`}>
          {side.kind === "music" ? (
            <MusicBubble music={side} />
          ) : (
            <Bubble item={side} />
          )}
        </Slot>
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
    // No action of its own: a click opens the expanded view (the core ignores it in hover mode).
    <button type="button" className="item" onClick={() => expandClick()}>
      {inner}
    </button>
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
