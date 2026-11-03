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
  /** Replaces the percentage or minutes, like the focus timer's `18:42`. */
  caption: string | null;
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
  /** The cover's dominant colour as `#rrggbb`, for the colour wash; null when there is none. */
  art_tint: string | null;
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
  /** The capsule's music tint as #rrggbb, or null. */
  music_tint: string | null;
  /** The theme provider's colours, or null to keep the built-in ones. */
  palette: Palette | null;
  /** Qapsule's calm springs, or Bloom's livelier ones (decision 0012). */
  motion: Motion;
}

export type Motion = "calm" | "lively";

/** Colours from Rust, all #rrggbb. The accents are already safe to read on the capsule. */
export interface Palette {
  accent: string;
  accent_2: string;
  /** The wallpaper's second colour at low chroma; null when the tint is off. */
  tint: string | null;
  source: "wallpaper" | "wallpaper_engine" | "capture" | "windows" | "custom";
}

export interface ExpandedView {
  date: string;
  weather: WeatherView;
  events: EventsView;
  /** The last notifications that left the screen, newest first. */
  history: HistoryView[];
  /** The screen showing (decision 0012). */
  screen: ScreenName;
  focus: FocusView;
}

export type ScreenName = "focus" | "main";
export const SCREENS: { id: ScreenName; label: string }[] = [
  { id: "focus", label: "Focus" },
  { id: "main", label: "Today" },
];

export type FocusPhase = "idle" | "running" | "paused" | "done";

export interface ModuleChip {
  code: string;
  color: string;
}

export interface DayBar {
  label: string;
  height: number;
  today: boolean;
  time: string;
}

export interface ModuleRow {
  code: string;
  color: string;
  time: string;
  share: number;
}

export interface StudyStats {
  today: string;
  week: string;
  streak: number;
  sessions_today: number;
  bars: DayBar[];
  modules: ModuleRow[];
}

/** Mirrors `FocusView` in crates/core/src/focus.rs. */
export interface FocusView {
  phase: FocusPhase;
  count_up: boolean;
  length_min: number;
  lengths: number[];
  display: string;
  caption: string;
  progress: number;
  module: ModuleChip | null;
  stats: StudyStats;
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

export type WeatherTone = "sun" | "cloud" | "rain" | "snow" | "storm";

export interface WeatherReady {
  city: string;
  temp_c: number;
  condition: string;
  glyph: string;
  tone: WeatherTone;
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

export interface EventHead {
  caption: string;
  title: string;
  location: string | null;
}

export interface EventRow {
  title: string;
  time: string;
  color: string | null;
  past: boolean;
}

export interface EventDay {
  label: string | null;
  now_line: number | null;
  rows: EventRow[];
}

export interface EventsReady {
  head: EventHead | null;
  days: EventDay[];
  more: string | null;
  stale: boolean;
}

export type EventsView =
  | { kind: "no_link" }
  | { kind: "loading" }
  | ({ kind: "ready" } & EventsReady)
  | { kind: "unavailable" };
