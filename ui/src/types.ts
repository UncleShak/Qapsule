// Mirrors `UiState` in crates/core/src/view.rs. The UI holds a copy of this and nothing else.

export type ActivityState = "running" | "needs_input" | "succeeded" | "failed" | "info";
export type VisibilityMode = "auto" | "always" | "on_demand";

export interface ItemView {
  source: string;
  id: string;
  source_label: string;
  title: string;
  subtitle: string | null;
  state: ActivityState;
  progress: number | null;
  indeterminate: boolean;
  icon: string;
  elapsed_min: number | null;
  primary_action: string | null;
}

/** The song playing now (decision 0008). Its key is always `media:` / `now-playing`. */
export interface MusicView {
  source: string;
  id: string;
  app: string;
  title: string;
  artist: string;
  /** A URL on the `qapsule-art` scheme. */
  art: string | null;
  playing: boolean;
  /** As of this snapshot; the UI animates from here. */
  position_ms: number | null;
  length_ms: number | null;
  /** How fast the position moves: 1 is normal, 0 when paused or not moving. */
  rate: number;
  controls: boolean;
}

export type PeekItem = ({ kind: "activity" } & ItemView) | ({ kind: "music" } & MusicView);

export type Peek =
  | { kind: "idle" }
  | { kind: "one"; main: PeekItem }
  | { kind: "dual"; main: PeekItem; side: PeekItem };

export interface ActionView {
  id: string;
  label: string;
  kind: "open_url" | "focus_window" | "callback" | "dismiss";
  primary: boolean;
}

export interface PopView {
  source: string;
  id: string;
  source_label: string;
  title: string;
  subtitle: string | null;
  state: ActivityState;
  icon: string;
  actions: ActionView[];
  sticky: boolean;
  seq: number;
}

export interface UiState {
  clock: string;
  mode: VisibilityMode;
  shown: boolean;
  peek: Peek;
  pop: PopView | null;
  undo: boolean;
  expanded: ExpandedView | null;
  focus: boolean;
}

export interface ExpandedView {
  date: string;
  weather: WeatherView;
  events: EventsView;
  /** The last notifications that left the screen, newest first. */
  history: HistoryView[];
}

export interface HistoryView {
  source: string;
  id: string;
  source_label: string;
  title: string;
  subtitle: string | null;
  state: ActivityState;
  icon: string;
  /** The local time it arrived, like `09:41`. */
  at: string;
  /** Pressing it does something. */
  actionable: boolean;
}

export interface HourView {
  label: string;
  temp_c: number;
  glyph: string;
}

export interface WeatherReady {
  city: string;
  temp_c: number;
  condition: string;
  glyph: string;
  high_c: number;
  low_c: number;
  hourly: HourView[];
  stale: boolean;
}

export type WeatherView =
  | { kind: "no_city" }
  | { kind: "loading" }
  | ({ kind: "ready" } & WeatherReady)
  | { kind: "unavailable" };

export interface EventNow {
  title: string;
  until: string;
  location: string | null;
}

export interface EventNext {
  title: string;
  day: string;
  time: string;
}

export interface EventsReady {
  now: EventNow | null;
  next: EventNext[];
  stale: boolean;
}

export type EventsView =
  | { kind: "no_link" }
  | { kind: "loading" }
  | ({ kind: "ready" } & EventsReady)
  | { kind: "unavailable" };
