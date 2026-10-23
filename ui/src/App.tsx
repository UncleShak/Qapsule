import { useEffect, useState } from "react";
import { dismissPop, press, setPointer, subscribe } from "./bridge";
import { Capsule } from "./capsule/Capsule";
import type { UiState } from "./types";

function App() {
  const [state, setState] = useState<UiState | null>(null);

  useEffect(() => subscribe(setState), []);

  // The window is clipped to the capsule, so entering the page means entering the capsule.
  useEffect(() => {
    const root = document.documentElement;
    const enter = () => setPointer(true);
    const leave = () => setPointer(false);
    root.addEventListener("pointerenter", enter);
    root.addEventListener("pointerleave", leave);
    return () => {
      root.removeEventListener("pointerenter", enter);
      root.removeEventListener("pointerleave", leave);
    };
  }, []);

  // Delete, Backspace or Esc dismisses the pop (DESIGN.md: undo instead of confirmation).
  const pop = state?.pop;
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

  return state ? <Capsule state={state} /> : null;
}

export default App;
