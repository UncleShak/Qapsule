# 11. Accent from the wallpaper, computed in Rust

Date: 2026-11-01 · Status: accepted

## Context

Qapsule's accent was a fixed fallback (#60CDFF). The colour pass wants the capsule to match the desktop: an accent, a second accent and a faint capsule tint taken from the wallpaper, including Wallpaper Engine wallpapers, which Windows doesn't report. Colour work must stay off the UI, cost nothing at idle and never produce an unreadable accent.

## Options

1. **Extract in the UI** from an image Rust hands over. This puts decoding and k-means in the webview, which can be destroyed at any time, and breaks "Rust owns state".
2. **Use only the Windows accent.** Cheap and safe, but it ignores the wallpaper and Wallpaper Engine.
3. **A Rust theme provider** that finds the image, extracts a palette on its own thread and sends it through the core.

## Choice

Option 3, in `crates/providers/src/theme/` and `palette.rs`.

- **Sources, in order:** the Wallpaper Engine preview (only while its process runs), then `IDesktopWallpaper` for the primary monitor, then one GDI capture of the primary monitor, then the Windows accent from `UISettings`. `accent_source = windows | custom` skips the image for the accent; the tint still comes from the wallpaper while `wallpaper_tint` is on. If a source yields an image with no usable colour (a grey wallpaper), the Windows accent is used.
- **Extraction:** box downscale to 64 px, OKLab, k-means with k = 6, a fixed k-means++ seed and at most 16 iterations, so the same wallpaper always gives the same palette. Score = area × chroma × a near-black/near-white penalty; clusters under 2% area or chroma 0.04 are ignored. `accent_2` is the next cluster at least 30° of hue away. `tint` is the second colour at L 0.55, chroma ≤ 0.08.
- **Safety:** the accent and `accent_2` are clamped (chroma ≤ 0.20, then lightness raised) to at least 4.5:1 against `--cap-bg` composited over black.
- **Cost:** the worker blocks on a channel. It recomputes only on events: start-up, `WM_SETTINGCHANGE` with `SPI_SETDESKWALLPAPER`, `ColorValuesChanged`, a settings change, a Wallpaper Engine `config.json` change (watched with `notify` only while Wallpaper Engine runs), and the expanded view opening. Results are cached by path and mtime. There is no timer, so Wallpaper Engine starting is noticed at the next of those events.
- **Transport:** the palette (`accent`, `accent_2`, `tint`, `source`) is part of `UiState` and goes through the coalescer, sent only when it changes. The protocol crate is unchanged.
- **UI:** `--accent`, `--accent-2` and `--tint` are swapped as plain variables. Only opacity and transform may animate, and fading the accent would mean drawing every accented element twice. The capsule tint cross-fades over 600 ms through the two tint layers from the music tint, and music wins while active.

## Consequences

- The accent follows the wallpaper with no idle cost, and stays readable on any wallpaper.
- Wallpaper Engine's config format is undocumented. Parsing lives in one module with tests (`docs/integrations/wallpaper-engine.md`); on failure it falls through and logs why.
- The accent changes instantly rather than fading; only the tint cross-fades.
- The album-art tint (`media/art.rs`) still uses its own HSL method. Moving it onto the shared extractor, as SPEC.md asks, is left open because it would change existing covers' colours.
- Multi-monitor setups use the primary monitor only.
