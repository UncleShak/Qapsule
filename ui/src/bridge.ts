// The only way the UI talks to Rust. Every press is a request; Rust decides what happens.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PageName, UiState } from "./types";
import type { Rect } from "./capsule/geometry";

/** Calls `apply` with the current state, then with every update. Returns an unsubscribe function. */
export function subscribe(apply: (state: UiState) => void): () => void {
  let updated = false;
  let stop: (() => void) | null = null;
  let stopped = false;
  listen<UiState>("ui-state", (event) => {
    updated = true;
    apply(event.payload);
  }).then((unlisten) => (stopped ? unlisten() : (stop = unlisten)));
  // An update that arrives first is newer than the snapshot.
  invoke<UiState>("get_snapshot").then((state) => {
    if (!updated) apply(state);
  });
  return () => {
    stopped = true;
    stop?.();
  };
}

export const press = (source: string, id: string, action: string) =>
  invoke("press_action", { source, id, action });
export const pressHistory = (source: string, id: string) => invoke("press_history", { source, id });
export const dismissPop = () => invoke("dismiss_pop");
export const undo = () => invoke("undo");
export const swapPeek = () => invoke("swap_peek");
export const setPointer = (over: boolean) => invoke("set_pointer", { over });
export const pointerSlowed = () => invoke("pointer_slowed");
export const expandClick = () => invoke("expand_click");
export const mediaClick = () => invoke("media_click");
export const closeExpanded = () => invoke("close_expanded");
export const setHitRects = (rects: Rect[]) => invoke("set_hit_rects", { rects });

// The Focus screen (decision 0012).
export const focusToggle = () => invoke("focus_toggle");
export const focusReset = () => invoke("focus_reset");
export const focusLength = (minutes: number) => invoke("focus_length", { minutes });
export const focusModule = () => invoke("focus_module");
export const setScreen = (screen: PageName) => invoke("set_screen", { screen });

// The Control page and the dashboard's corner (the overhaul).
export const setVolume = (level: number) => invoke("set_volume", { level: Math.round(level) });
export const toggleMute = () => invoke("toggle_mute");
export type SystemPage =
  | "ms-settings:network-wifi"
  | "ms-settings:bluetooth"
  | "ms-settings:nightlight"
  | "ms-settings:display"
  | "ms-settings:batterysaver";
export const openSystemPage = (page: SystemPage) => invoke("open_system_page", { page });
export const toggleDnd = () => invoke("toggle_dnd");
export const openSettings = () => invoke("open_settings");
export const stepScreen = (delta: number) => invoke("step_screen", { delta });
