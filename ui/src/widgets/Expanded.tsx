// The expanded capsule (the overhaul): the pages the settings show, side by side on a
// track that slides between them, with tabs along the bottom and two quick actions in the
// corner. The core decides which pages show and which one is showing. Plain text only.

import type { CSSProperties, WheelEvent } from "react";
import { useRef } from "react";
import { openSettings, setScreen, stepScreen, toggleDnd } from "../bridge";
import { Glyph } from "../capsule/Glyph";
import { PAGE_LABELS, type ExpandedView, type PageName, type UiState } from "../types";
import { Clock } from "./Clock";
import { Control } from "./Control";
import { Events } from "./Events";
import { Focus } from "./Focus";
import { Home } from "./Home";
import { MusicPage } from "./MusicPage";
import { Weather } from "./Weather";

/** Wheel steps closer together than this are one gesture. Compared on event timestamps: no timers. */
const WHEEL_GAP_MS = 400;

function Page({ name, state, expanded }: { name: PageName; state: UiState; expanded: ExpandedView }) {
  switch (name) {
    case "home":
      return <Home state={state} expanded={expanded} />;
    case "control":
      return <Control expanded={expanded} />;
    case "focus":
      return <Focus focus={expanded.focus} />;
    case "calendar":
      return (
        <div className="main-screen">
          <Weather weather={expanded.weather} />
          <Clock time={state.clock} date={expanded.date} history={expanded.history} />
          <Events events={expanded.events} />
        </div>
      );
    case "music":
      return <MusicPage music={expanded.music} />;
  }
}

export function Expanded({ state, expanded }: { state: UiState; expanded: ExpandedView }) {
  const pages = expanded.pages;
  const at = Math.max(0, pages.indexOf(expanded.screen));
  const lastWheel = useRef(-Infinity);
  const onWheel = (e: WheelEvent) => {
    // A slider or a scrolling list keeps its own wheel.
    if ((e.target as HTMLElement).closest("input, .scrolls")) return;
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (Math.abs(delta) < 4) return;
    const gap = e.timeStamp - lastWheel.current;
    lastWheel.current = e.timeStamp;
    if (gap < WHEEL_GAP_MS) return;
    stepScreen(delta > 0 ? 1 : -1);
  };
  return (
    <div className="screens" onWheel={onWheel}>
      <div className="screen-track" style={{ "--at": at, "--count": pages.length } as CSSProperties}>
        {pages.map((name) => (
          <div key={name} className="screen-page" aria-hidden={name !== expanded.screen} inert={name !== expanded.screen}>
            <Page name={name} state={state} expanded={expanded} />
          </div>
        ))}
      </div>
      <div className="corner-actions">
        <button
          type="button"
          className={`corner-button ${expanded.do_not_disturb ? "on" : ""}`}
          aria-label="Do not disturb"
          aria-pressed={expanded.do_not_disturb}
          onClick={() => toggleDnd()}
        >
          <Glyph name="moon" />
        </button>
        <button type="button" className="corner-button" aria-label="Settings" onClick={() => openSettings()}>
          <Glyph name="settings" />
        </button>
      </div>
      {pages.length > 1 && (
        <nav className="page-tabs" aria-label="Pages">
          {pages.map((name) => (
            <button
              key={name}
              type="button"
              className="page-tab"
              aria-current={name === expanded.screen ? "page" : undefined}
              onClick={() => setScreen(name)}
            >
              {PAGE_LABELS[name]}
            </button>
          ))}
        </nav>
      )}
    </div>
  );
}
