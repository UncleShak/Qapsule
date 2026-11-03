// The expanded capsule: the Focus screen on the left and the main screen (weather, clock,
// events) in the centre, side by side on a track that slides between them (decision 0012).
// Plain text only.

import type { CSSProperties, WheelEvent } from "react";
import { useRef } from "react";
import { setScreen, stepScreen } from "../bridge";
import { SCREENS, type ExpandedView, type UiState } from "../types";
import { Clock } from "./Clock";
import { Events } from "./Events";
import { Focus } from "./Focus";
import { Weather } from "./Weather";

/** Wheel steps closer together than this are one gesture. Compared on event timestamps: no timers. */
const WHEEL_GAP_MS = 400;

export function Expanded({ state, expanded }: { state: UiState; expanded: ExpandedView }) {
  const at = Math.max(0, SCREENS.findIndex((s) => s.id === expanded.screen));
  const lastWheel = useRef(-Infinity);
  const onWheel = (e: WheelEvent) => {
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (Math.abs(delta) < 4) return;
    const gap = e.timeStamp - lastWheel.current;
    lastWheel.current = e.timeStamp;
    if (gap < WHEEL_GAP_MS) return;
    stepScreen(delta > 0 ? 1 : -1);
  };
  return (
    <div className="screens" onWheel={onWheel}>
      <div className="screen-track" style={{ "--at": at, "--count": SCREENS.length } as CSSProperties}>
        <div className="screen-page" aria-hidden={expanded.screen !== "focus"} inert={expanded.screen !== "focus"}>
          <Focus focus={expanded.focus} />
        </div>
        <div className="screen-page" aria-hidden={expanded.screen !== "main"} inert={expanded.screen !== "main"}>
          <div className="main-screen">
            <Weather weather={expanded.weather} />
            <Clock time={state.clock} date={expanded.date} history={expanded.history} />
            <Events events={expanded.events} />
          </div>
        </div>
      </div>
      <nav className="pips" aria-label="Screens">
        {SCREENS.map((s) => (
          <button
            key={s.id}
            type="button"
            className={`pip ${s.id === expanded.screen ? "on" : ""}`}
            aria-label={s.label}
            aria-current={s.id === expanded.screen ? "page" : undefined}
            onClick={() => setScreen(s.id)}
          />
        ))}
      </nav>
    </div>
  );
}
