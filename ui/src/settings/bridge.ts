// The settings window's only link to Rust. Rust owns the values; these are requests,
// and what comes back is what the window shows. Not UI updates, so no coalescer.

import { invoke } from "@tauri-apps/api/core";

export type AccentSource = "wallpaper" | "windows" | "custom";
export type Motion = "calm" | "lively";
export type PageName = "home" | "control" | "focus" | "calendar" | "music";
export type OpenOn = "last" | "home";
export type ExpandOn = "hover" | "click";

export interface Appearance {
  accent_source: AccentSource;
  custom_accent: string;
  wallpaper_tint: boolean;
  music_tint: boolean;
  motion: Motion;
  translucent: boolean;
  pages: PageName[];
  open_on: OpenOn;
  expand_on: ExpandOn;
  hover_delay_ms: number;
  daily_goal_min: number;
}

export const getAppearance = () => invoke<Appearance>("get_appearance");
export const setAppearance = (patch: Partial<Appearance>) =>
  invoke<Appearance>("set_appearance", { patch });
