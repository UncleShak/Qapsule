import { useEffect, useRef, useState } from "react";
import { closeExpanded, dismissPop, pointerSlowed, press, setPointer, subscribe } from "./bridge";
import { Capsule } from "./capsule/Capsule";
import type { UiState } from "./types";

function App() {
  const [state, setState] = useState<UiState | null>(null);
  const [hovered, setHovered] = useState(false);

  useEffect(() => subscribe(setState), []);

  const expanded = state?.expanded != null;
  // The core decides when to expand; the UI only reports how the pointer moved.
  const open = useRef(expanded);
  open.current = expanded;

  // The window is clipped to the capsule, so entering the page means entering the capsule.
  useEffect(() => {
    const root = document.documentElement;
    // Speed between consecutive pointermove events, from their own timestamps. The first
    // time in a hover it drops under 0.25 px/ms, the core is told (once). No timers.
    let last: { x: number; y: number; t: number } | null = null;
    let slowed = false;
    const enter = () => {
      last = null;
      slowed = false;
      setHovered(true);
      setPointer(true);
    };
    const leave = () => {
      last = null;
      setHovered(false);
      setPointer(false);
    };
    const move = (e: PointerEvent) => {
      const previous = last;
      last = { x: e.clientX, y: e.clientY, t: e.timeStamp };
      // The first sample has nothing to compare with.
      if (!previous || slowed || open.current) return;
      const dt = e.timeStamp - previous.t;
      if (dt <= 0) return;
      if (Math.hypot(e.clientX - previous.x, e.clientY - previous.y) / dt < 0.25) {
        slowed = true;
        pointerSlowed();
      }
    };
    root.addEventListener("pointerenter", enter);
    root.addEventListener("pointerleave", leave);
    root.addEventListener("pointermove", move);
    return () => {
      root.removeEventListener("pointerenter", enter);
      root.removeEventListener("pointerleave", leave);
      root.removeEventListener("pointermove", move);
    };
  }, []);

  // Esc closes the expanded view.
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeExpanded();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded]);

  // Delete, Backspace or Esc dismisses the pop (DESIGN.md: undo instead of confirmation).
  // No pop is shown while the view is expanded.
  const pop = expanded ? null : state?.pop;
  useEffect(() => {
    if (!pop) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace" && e.key !== "Escape") return;
      const dismiss = pop.actions.find((a) => a.kind === "dismiss");
      if (dismiss && e.key !== "Escape") press(pop.source, pop.id, dismiss.id);
      else dismissPop();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pop]);

  return state ? <Capsule state={state} hovered={hovered} /> : null;
}

export default App;
