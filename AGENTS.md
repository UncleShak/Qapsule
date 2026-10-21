# AGENTS.md

Rules for AI coding agents working in this repo. The full design lives in `docs/SPEC.md`; read it before changing architecture, the protocol or the window code.

## Commands

Run from the repo root. Rust is a Cargo workspace; the UI is a Bun workspace (`ui/`).

```sh
bun install                         # install JS deps (root + ui workspace)
bun run tauri dev                   # run the app (starts the Vite dev server on :1420)
bun run tauri build                 # release build and installer
cargo build --workspace             # build all Rust crates
cargo test --workspace              # run all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run -p qapsule-cli -- <args>  # run the CLI (binary: qapsule)
bun run --cwd ui build              # type-check (tsc) and build the UI
```

UI component tests will use Vitest (not yet set up). Performance budgets are checked with `scripts/measure-idle.ps1` before each release.

## Non-negotiables

1. **Rust owns all state; the UI is disposable.** The webview can be destroyed at any moment and nothing may be lost. Never make the UI the source of truth for anything.
2. **No JS animation loops or UI polling timers.** No `requestAnimationFrame` loops, `setInterval`, or polling `setTimeout`. Animate with CSS transform and opacity transitions only, and honour reduce-motion.
3. **Every UI update goes through the core's coalescer** (at most 4 updates per second). Never emit to the webview directly from a provider, the pipe server or a client.
4. **Protocol changes need a decision record** in `docs/decisions/` (context, options, choice, consequences), committed with the change.
5. **Never add a pushed button that runs commands.** Buttons pushed by clients may open links, focus windows or send a callback to the pushing app; they never execute commands.

## Folders

- `crates/protocol/` — The public contract: the activity model and pipe messages (MIT). Everything must round-trip through serde, ignore unknown fields and enforce limits. Changing it requires a decision record.
- `crates/cli/` — The `qapsule` command-line client. It talks to the app only through the named pipe, using the protocol crate.
- `crates/core/` *(planned)* — Activity store, priority, expiry, widget layout, event bus and the UI update coalescer. Plain Rust, tested with a fake clock and no window.
- `crates/providers/` *(planned)* — One provider per data source (weather, calendar, media, timer, GitHub, dev servers, downloads), each running only as often as its visibility allows.
- `src-tauri/` — The app shell: pipe server, window manager (peek/pop/expanded, webview lifetime, click-through region), tray, hotkey and action router. Keep logic thin here; put testable code in `crates/`.
- `ui/` — React + TypeScript + Tailwind view (Vite, Bun) rendered in WebView2. It requests a snapshot from the core on start, then applies updates; button presses go back as Tauri commands.
- `integrations/` *(planned)* — Hook configs for Claude Code, Codex and shell recipes that push activities via the CLI.
- `docs/` — `SPEC.md`, `ROADMAP.md`, decision records in `docs/decisions/`, and integration guides.
- `scripts/` *(planned)* — Tooling such as `measure-idle.ps1` for the performance budgets.
