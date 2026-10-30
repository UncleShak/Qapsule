# Plan: live Windows notifications

Status: approved · 2026-10-30 · Build window 18–30 Nov (roadmap, Phase 2)

Background: `docs/spikes/windows-notifications.md` (why identity is needed), `docs/SPEC.md` (provider table, pop settings, privacy rules, known limits).

## Goal

Toasts from other apps (WhatsApp, Teams, Outlook…) pop in the pill as they arrive, event-driven, on the development machine with a self-signed identity package. Per-app pop and details settings. Notification text is never stored on disk or logged. Shipping to other people (SignPath or the Store) is out of scope here.

## Decisions (settled by Plato unless the Archon overrides)

1. **Activities, not a new pipeline.** Each toast becomes an `Activity` with `state = Info` inside the core, so pops, priority, Do Not Disturb, expiry and the coalescer all work unchanged.
2. **Source per app: `windows:<App-Display-Name>`**, whitespace turned into `-` because protocol keys can't contain spaces (for example `windows:WhatsApp`, `windows:Microsoft-Teams`; the label shows spaces again). `settings::category` maps any `windows:` source to `windows_apps`, so the existing lookup gives per-app overrides for free:
   ```toml
   [notifications]
   windows_apps = { pop = true, details = false }
   "windows:WhatsApp" = { pop = true, details = true }
   "windows:Microsoft-Teams" = { pop = false }
   ```
3. **`windows:` is a reserved source prefix.** The pipe server rejects pushes from clients that use it, so no script can fake an "Outlook" pop. This is a visible protocol behaviour, so it gets a decision record.
4. **Provider activities have no pipe owner and their buttons go to the provider.** The core gets a narrow provider input path (upsert/remove with no `ConnId`), and a new internal `Effect::Provider { source, id, action }`. The notification provider's `act` opens the app via `shell:AppsFolder\<AUMID>`. This is not an `ActionKind`, so pushed clients can never use it (AGENTS.md rule 5).
5. **No big engine refactor first.** Architecture review item 4 (`core.apply`) is deferred; we add one input path now and revisit when the second provider lands.
6. **Privacy at ingest.** No `tracing`/`println` of notification text anywhere. `qapsule list` redacts `windows:` activities to the app name when details are off. Nothing is written to disk.
7. **History never pops.** Toasts already in the notification centre at startup are ignored; only `NotificationChanged` *Added* pops. *Removed* (the user cleared it in Windows) removes the activity.
8. **No app logos in v1.** Icon is a generic `app` icon with the app name as the source label. Logos need an image pipeline; later.

## Phases

Each phase lists who does what. Routing follows `.claude/academy.md`.

### Phase 0 — Decisions on paper (Plato) · ~1 hour

- Write `docs/decisions/0006-providers-and-reserved-sources.md`: decisions 2–4 and 6 above.
- Clear `.claude/agora.md`, add a line pointing students at this plan.
- **Gate:** the Archon approves this plan and decision 0006.

### Phase 1 — Development identity (aristotle; socrates reviews) · 18–20 Nov

Commission → aristotle:
- `packaging/identity/AppxManifest.xml`: sparse package `Qynetik.Qapsule.Dev`, `uap10:AllowExternalContent`, `uap10:RuntimeBehavior="win32App"`, `uap10:TrustLevel="mediumIL"`, capability `userNotificationListener`.
- `src-tauri/qapsule.exe.manifest` with the matching `<msix>` element, keeping Tauri's default common-controls and DPI entries; embed it via `tauri_build::WindowsAttributes::app_manifest` in `src-tauri/build.rs`.
- `scripts/identity/new-dev-cert.ps1` (self-signed cert, exported `.cer`), `build-identity.ps1` (MakeAppx `pack /nv` + SignTool), `register-identity.ps1 -ExternalLocation <exe folder>` and `unregister-identity.ps1`.
- A `--identity-check` debug path (or test binary) that prints `Package::Current()` and whether `NotificationChanged` subscribes.

Checks the student must report (these are the spike's open risks):
1. With the package registered: identity present, `NotificationChanged` subscribes without `0x80070490`.
2. With it **unregistered**: the app still starts and runs normally (the GitHub installer must not break).
3. `%APPDATA%\Qynetik\Capsule\config.toml` is still read and written at the real path (no virtualisation).

Archon does by hand (needs admin): import the `.cer` into `LocalMachine\TrustedPeople` once.

socrates: examine the scripts and manifests (cert scope, what's trusted, ExternalLocation path).

**Gate:** checks 1–3 pass on the Archon's laptop.

### Phase 2 — Core and engine plumbing (aristotle; diogenes; socrates) · 20–23 Nov

Commission → aristotle (`crates/core`, `crates/server`, tests):
- `Core::provider_upsert(activity, now)` and `Core::provider_remove(source, id, now)`: owner `None`, same pop rules.
- `settings::category`: `windows:*` → `windows_apps`. `view::source_label` / `icon_for`: app name and `app` icon for `windows:` sources.
- Pipe path rejects reserved `windows:` sources with a protocol error.
- `Effect::Provider { source, id, action }` from `Core::press`; engine routes it to a provider sink.
- `list` redaction rule (decision 6).
- Engine: `Engine::provider_upsert` / `provider_remove` commands.
- Tests with hand-driven time: pop on add, no pop when `pop = false`, details off hides text in the view and in `list`, DND holds, reserved prefix rejected, press yields `Effect::Provider`.

Commission → diogenes: the `[notifications]` example in `docs/SPEC.md` gains the per-app override lines from decision 2.

socrates: examine (public behaviour change + privacy).

**Gate:** `cargo test --workspace` green; `qapsule push --source windows:Fake` is refused.

### Phase 3 — The notification provider (aristotle; socrates) · 23–27 Nov

Commission → aristotle (new crate `crates/providers`, module `notifications`):
- Pure, tested mapper: `ToastFields { aumid, app_name, texts: Vec<String>, id }` → `Activity` (first text = title, rest joined = subtitle, truncated to protocol limits, control characters stripped, one "Open" button).
- WinRT listener behind `cfg(windows)` (`windows` crate features `UI_Notifications_Management`, `UI_Notifications`, `Foundation`, `ApplicationModel`): `RequestAccessAsync`, subscribe to `NotificationChanged`, on *Added* fetch that one notification and send `provider_upsert`; on *Removed* send `provider_remove`. Handlers run on WinRT threads, so they only push into a channel; never block them.
- Without identity or with access denied: provider logs one line (no text), stays idle, costs nothing.
- `act("open")`: `ShellExecuteW("shell:AppsFolder\<AUMID>")`.
- Wire it in `src-tauri/src/lib.rs`: spawn at startup, connect the `Effect::Provider` sink.

socrates: examine (FFI, threading, privacy, idle cost).

**Gate:** a real WhatsApp/Teams message pops in the pill within a second; clicking Open brings the app forward; turning `details = false` shows only the app name.

### Phase 4 — Polish and measure (aristotle; diogenes) · 27–29 Nov

- aristotle: tray entry "Windows notifications" showing On / Needs permission (opens `ms-settings:privacy-notifications`) / Unavailable (no identity).
- aristotle or diogenes (runs it): `scripts/measure-idle.ps1` with the listener running; budgets unchanged.
- diogenes: `docs/integrations/windows-notifications.md` setup guide: dev identity steps, Windows still shows its own banner unless turned off per app, Open launches the app not the chat.

**Gate:** idle CPU/memory within the spec's budgets; full checks green.

### Phase 5 — Daily use (the Archon; Plato) · from 30 Nov

- The Archon uses it daily and notes misses in `.claude/agora.md` or issues.
- Plato tick the roadmap's 18–30 Nov item, clear the agora, open the PR.
- Next (not in this plan): SignPath application on 22 Dec; Store build in parallel.

## Done means

`cargo fmt --all`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` and `bun run --cwd ui build` are green; decision 0006 is committed with the protocol-visible change; socrates' questions for phases 1–3 are answered.
