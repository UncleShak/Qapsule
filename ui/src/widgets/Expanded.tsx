// The main screen of the expanded capsule: weather, clock, events. Plain text only.

import type { ExpandedView, UiState } from "../types";
import { Clock } from "./Clock";
import { Events } from "./Events";
import { Weather } from "./Weather";

export function Expanded({ state, expanded }: { state: UiState; expanded: ExpandedView }) {
  return (
    <div className="main-screen">
      <Weather weather={expanded.weather} />
      <Clock time={state.clock} date={expanded.date} history={expanded.history} />
      <Events events={expanded.events} />
    </div>
  );
}
