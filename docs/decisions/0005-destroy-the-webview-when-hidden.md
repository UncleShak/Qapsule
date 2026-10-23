# 5. Destroy the webview when the capsule is out of sight

Date: 2026-10-23 · Status: accepted

## Context

The budget for a hidden Qapsule (tucked away in Peek on demand, or behind a fullscreen app in Auto) is under 0.5% CPU and 30 MB of memory. Measured before this change, a tucked-away Qapsule used 115 MB in 7 processes: the Rust process plus WebView2's browser, GPU, renderer and utility processes. Hiding the capsule with CSS saved nothing.

## Options

1. **Keep the webview and hide it.** Instant to bring back, but about 115 MB forever.
2. **Suspend the webview** (`ICoreWebView2_3::TrySuspend`). Lowers CPU, but the processes and most of their memory stay, so it misses the 30 MB budget by a wide margin.
3. **Destroy the webview** after a delay, and recreate it when the capsule is needed.

## Choice

Option 3. The webview is destroyed 10 seconds after the capsule goes out of sight (`src-tauri/src/lifecycle.rs`); the delay avoids churn when it comes straight back. Because Rust owns all state ([decision 2](0002-rust-owns-state.md)), nothing is lost: a new webview asks the core for a snapshot.

Two pieces make it work:

- **A native edge strip.** The hover strip at the top edge that brings a tucked capsule out can't live in a webview that may not exist. It is now a 120 × 3 pixel native window, nearly transparent and never taking focus, shown only while the capsule is tucked away (`watcher.rs`). Touching it reveals the capsule for 2 seconds, long enough for a new webview to appear under the pointer.
- **No flash on recreate.** A new webview starts with an empty window region (nothing drawn or clickable), paints the capsule tucked, and then slides it in. That replaces the spec's "render off-screen", with the same effect.

## Consequences

Measured on the release build (`scripts/measure-idle.ps1`):

| | Before | After |
| --- | --- | --- |
| Hidden, memory | 115 MB, 7 processes | 2.7 MB, 1 process |
| Hidden, CPU | 0.01% | 0.00% |
| Pop while hidden, push to first frame | about 140 ms | 590–690 ms |

The cost is the cold pop: bringing back a destroyed webview takes 590–690 ms, over the spec's 400 ms. A pop within 10 seconds of the capsule hiding is still warm (about 140 ms). If the cold time matters in daily use, the options are a longer delay before destroying, starting the webview's browser process early, or drawing the first frame of a pop natively.
