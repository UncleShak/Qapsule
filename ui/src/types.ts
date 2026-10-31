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

export type Peek =
  | { kind: "idle" }
  | { kind: "one"; main: ItemView }
  | { kind: "dual"; main: ItemView; side: ItemView };

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
