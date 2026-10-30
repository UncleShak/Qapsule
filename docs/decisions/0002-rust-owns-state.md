# 2. Rust owns all state; the UI is a disposable view

Date: 2026-10-23 · Status: accepted

## Context

The whole pitch rests on idle CPU under 1%. A WebView2 costs tens of megabytes and some CPU just by existing, so Qapsule must be able to destroy it whenever the capsule is out of sight (milestone 5), and recreate it later without losing anything.

## Options

1. **State in React**, with Rust as a thin bridge. Natural for a web UI, but destroying the webview would lose activities, timers and pops.
2. **State in Rust**, with the UI rendering whatever Rust tells it.

## Choice

State lives in Rust. `crates/core` holds the activity store, priority, expiry, pops, undo and visibility, and produces a complete `UiState`. The UI asks for a snapshot on start, then receives whole `UiState` updates, at most 4 a second through the core's coalescer. Button presses go back to Rust as Tauri commands; the UI never decides what a press does. Even the clock and "Working · 6 min" are computed in Rust, so the UI needs no timers.

## Consequences

- The webview can be killed at any moment with nothing lost.
- The core is plain Rust with time passed in, so it is tested without a window or a real clock (`crates/core/src/tests.rs`).
- Sending the whole state each time is simple and small (well under a kilobyte); if it ever grows large, diffs can come later without changing who owns the state.
