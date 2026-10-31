# Qapsule Technical Spec

Oct 18, 2026 · @Nadiya

## Summary

Qapsule is a command hub that hangs from the top centre of the screen: always there at a glance, with quick access to your day, your work and your system.

- **Peek view** is the small pill you always see. It shows the time, or what's happening right now (music, a download, an agent), and briefly grows to pop notifications you've allowed.
- **Expanded view** opens when you hover the peek. It has three screens: your widgets on the left, your main dashboard in the centre, and system controls on the right.
- **Live activities** are pushed in by apps, scripts and AI agents through a local API, with action buttons. This is what no other Windows island offers, so it leads every launch post.

**Who it's for:** first you, a developer who runs AI agents and long tasks between lectures. Then other developers and power users on Windows.

**Goals**

1. One place for everything you check during the day, without opening apps.
2. Never miss an agent waiting for your input.
3. You decide what may interrupt you, and how much detail it shows.
4. Lean when idle: under 1% CPU, with memory measured and published.

**Non-goals for v1.0**

- An app launcher or search (Raycast and PowerToys already do this well).
- Phone integration, cloud accounts or sync.
- Buttons pushed by other apps that run arbitrary commands (security risk).
- Multi-monitor placement beyond the primary screen.

## Feature list

Twenty features across the peek view and three screens, delivered in three releases: the v0.1 preview (22 Dec), the public launch (19 Jan) and v1.0 (30 Mar).

| Feature | Where | Release | How it works | Buttons |
| --- | --- | --- | --- | --- |
| Time | Peek, when nothing else is happening | v0.1 | System clock | None |
| Dual status: ¾ main item, ¼ ring or icon | Peek | v0.1 | Activity store priority | Click the ¼ to swap |
| Visibility modes: Auto, Always show, Peek on demand | Peek | v0.1 | Settings | None |
| **AI agent status** (your top priority) | Peek pop; agent board | v0.1 | Claude Code and Codex hooks call the CLI | Open terminal, dismiss |
| Command progress (`qapsule wrap`) | Peek ¼ ring | v0.1 | CLI | Dismiss |
| Custom pushes from any app | Peek | v0.1 | Named-pipe API | Open link, callback |
| Now playing | Peek; system screen | v0.1 | Windows media session | Play/pause, previous, next |
| Pop notifications from Qapsule's sources, each with on/off and show-details settings | Peek | v0.1 | Built in | Per source |
| Weather (left slot) | Main screen | v0.1 | Open-Meteo, city in settings | Open forecast |
| Time and date (middle slot) | Main screen | v0.1 | System clock | None |
| Current event and the coming week, scrollable (right slot) | Main screen | v0.1 | Calendar iCal link | Open event |
| Volume, mute, per-app volume | System screen | Launch | Windows Core Audio | Sliders |
| Brightness | System screen | Launch | WMI for laptop screens; DDC/CI for external monitors where supported | Slider |
| Dark mode toggle | System screen | Launch | Registry setting plus a settings-changed broadcast | Toggle |
| Wi-Fi, Bluetooth, display, sound and power shortcuts | System screen | Launch | Open the matching Windows Settings page | Tap to open |
| Agent board and timer | Your widgets screen | Launch | Activity store; built-in timer | Jump to terminal; start, pause, reset |
| Notifications from other Windows apps | Peek | Launch, through a signed identity package (SignPath) or the Store build; otherwise right after | Windows notification listener; needs signed package identity | Open, dismiss |
| Experimental toggles: Do Not Disturb, night light, energy saver | System screen | v1.0 | Undocumented Windows settings, falling back to the Settings page | Toggle |
| GitHub CI and PRs; dev servers | Your widgets screen; pop on failure or crash | v1.0 | GitHub API plus ntfy; Windows TCP table | Open, re-run |
| Downloads | Your widgets screen; peek ¼ ring | v1.0 | Watching the Downloads folder | Open folder |

**Defaults for new users:** main screen is weather, time and events. Your widgets screen starts with the agent board and timer. Every widget can be removed, but each screen keeps at least one and at most three.

## Behaviour

The capsule hangs from the top edge of the screen, like Bloom: a small peek most of the time, a brief pop when something you allowed happens, and the full expanded view when you hover.

**The look**

- Flush with the top edge of the primary monitor: no gap, square top, rounded bottom corners, so it reads as part of the screen.
- It slides down out of the edge when it appears, and grows wider and taller, never jumping, when it pops or expands.
- A solid, slightly translucent dark fill drawn in CSS (about 94% opaque), not Mica or Acrylic: system blur fills the whole window rectangle and cannot follow the curved pill shape. The visual language is defined in DESIGN.md in the repo.

**Visibility modes** (settings)

| Mode | On the desktop | In a fullscreen app |
| --- | --- | --- |
| Auto (default) | Peek always visible | Hidden; mouse to the top centre or the hotkey brings it out |
| Always show | Peek always visible | Peek stays visible |
| Peek on demand | Tucked into the edge | Same; appears only for allowed pops, top-centre hover or the hotkey |

**Peek view**

- Nothing happening: the time.
- One thing happening: it takes the full pill. Music is cover · clock · audio bars; a click on the cover opens the media view (title, artist, previous / pause / next, progress line) and a click elsewhere opens the expanded view (decision 10).
- Two or more: dual status. The highest-priority item takes ¾, the next takes ¼ as a progress ring or icon. Clicking the ¼ swaps them.
- Priority: needs your input, failed, running with progress, music, time. Ties go to the newest.

**Pop notifications**

- The pill grows for about 5 seconds to show the notification with its buttons, then shrinks back to peek. "Needs your input" stays until handled or dismissed.
- Every source (agents, music, downloads, timers, calendar, other apps) has its own settings: pop or not, and show details or just the source name.
- Do Not Disturb in the tray menu holds all pops until you turn it off.

**Expanded view**

- The peek expands to the main screen when the pointer slows over it (under about 0.25 px/ms, then 60 ms) or after 180 ms of hovering; moving away collapses it after 300 ms (decision 7). The hotkey (default Ctrl+Alt+Q; `hotkey = "off"` turns it off) opens it and keeps it open until Esc. A setting can require a click instead of hover.
- Three screens: your widgets (left), main (centre) and system (right). Switch with the mouse wheel, arrow keys, the dots, or a touchpad swipe.
- Each screen holds one to three widgets. Right-click → Edit screens: drag to swap, × to remove, + to add. A screen never goes below one widget.
- The main screen's clock column shows the time and date above, and below them the last three notifications that left the screen (title and arrival time; click one to run its main action and remove it). They clear once you have seen them, when the view closes, and are kept in memory only, never written to disk (decision 9).

**Action buttons**

- Each activity or pop shows up to two buttons, one primary. Widgets have their own, such as play/pause or a volume slider.
- Buttons pushed by other apps can open a link, bring a window forward, or send a callback to that app; never run a command. Your own system-screen controls act directly.
- Every control is reachable by keyboard: Tab to move, Enter to press.

## Architecture

One Rust process owns all state; the React UI is a disposable view that can be destroyed at any moment without losing anything.

&#91;embedded content: Qapsule architecture · 7 components\]

Clients and providers feed the core; the core tells the window what to show and streams updates to the UI whenever it exists.

**Components**

- **Clients:** the `qapsule` CLI, agent hooks and any other app. They talk to Qapsule only through the named pipe `\\.\pipe\qapsule`.
- **Pipe server:** accepts connections, checks messages against the protocol crate, and hands them to the core. A client stays connected to receive callbacks for its own activities.
- **Core:** the heart. It holds the activity store (with priority and expiry), the widget layout, and an event bus. It decides what the window should show and coalesces UI updates to at most 4 per second.
- **Providers:** one per data source (weather, calendar, media, timer, audio and brightness, Windows notifications, experimental toggles, GitHub, dev servers, downloads). Each runs only as often as its visibility allows (see Widgets and providers).
- **Window manager:** owns the visibility modes and the peek, pop and expanded states, keeps the window flush with the top edge, creates and destroys the webview, watches for fullscreen apps, and handles the hotkey and tray icon.
- **Action router:** runs button presses: opens links, brings windows to the front, forwards built-in actions to providers, and sends callbacks back to pipe clients.
- **UI:** React in WebView2. On start it asks the core for a full snapshot, then receives updates. Button presses go back as Tauri commands.

**The one rule:** nothing in the UI is the source of truth. You can kill the webview at any moment and lose nothing.

**Provider interface** (a sketch to refine in Phase 1):

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    /// Runs until shutdown. Reads visibility changes, publishes widget state.
    async fn run(&self, ctx: ProviderCtx) -> anyhow::Result<()>;
    /// Handles a built-in action from a button, e.g. "media.next".
    async fn act(&self, action: &str, args: serde_json::Value) -> anyhow::Result<()>;
}

pub enum Visibility { Hidden, Background, Visible }
```

## Protocol v1

Clients send newline-delimited JSON over the named pipe. Every message carries `"v": 1`. This is the public contract, defined once in `crates/protocol` and shared by the app and the CLI.

**Activity fields**

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `id` | string | Yes | Unique within its source; pushing the same id updates it |
| `source` | string | Yes | Who sent it, e.g. `claude-code`, `wrap`, `my-script` |
| `title` | string, max 80 characters | Yes | Main line in the pill |
| `subtitle` | string, max 120 characters | No | Second line in the stack |
| `state` | `running` · `needs_input` · `succeeded` · `failed` · `info` | Yes | Drives colour, priority and auto-removal |
| `progress` | number 0–1, or `"indeterminate"` | No | Omitted means no progress bar |
| `icon` | built-in icon name | No | e.g. `agent`, `build`, `download` |
| `expires_in` | seconds | No | Default 1 hour while running; finished states leave after 4 seconds |
| `actions` | up to 2 action objects | No | See below |

**Action kinds:** `open_url` (http/https only), `focus_window` (by process id), `callback` (sent back to the pushing client), `dismiss`. One action may be marked `primary`.

**Messages**

- Client to Qapsule: `hello`, `upsert`, `end`, `clear`, `list`.
- Qapsule to client: `ok`, `error` (with a code), `list_result`, `action` (a callback: which activity and which button).

**Example:** an agent that needs your input.

```json
{"v":1,"type":"upsert","activity":{"id":"cc-4821","source":"claude-code","title":"Claude needs your input","subtitle":"qapsule repo, session 4821","state":"needs_input","icon":"agent","actions":[{"id":"go","label":"Open terminal","kind":"focus_window","pid":4821,"primary":true},{"id":"x","label":"Dismiss","kind":"dismiss"}]}}
```

The same from the CLI:

```powershell
qapsule push --id cc-4821 --source claude-code --title "Claude needs your input" --state needs_input --focus-pid 4821
qapsule done --id cc-4821
qapsule wrap -- cargo build --release
```

**Agent mapping.** Hooks call the CLI. Check each tool's current hook names before building; they change.

| Agent event | Qapsule state | Title |
| --- | --- | --- |
| Prompt submitted, or a tool starts | `running` | Working |
| Agent asks for permission or input | `needs_input` | Needs your input |
| Agent stops normally | `succeeded` | Done |
| Agent errors | `failed` | Stopped with an error |

**Rules**

- Limits: 64 KB per message, 50 live activities per source, 20 messages per second per connection. Extra messages are dropped with an error.
- Unknown fields are ignored, so v1 clients keep working as v1 grows. A breaking change means protocol v2 on a new pipe name.
- Bringing the right window forward: walk from the given process id up its parent chain to the first process that owns a visible window (Windows Terminal hosts the shell in a separate process).

## Widgets and providers

A widget is a React component plus a Rust provider that feeds it. Providers are event-driven wherever Windows allows, and poll only while their screen is on show, which keeps the always-visible peek cheap.

**Widget definition:** an id, allowed sizes (one slot or two), a settings schema, and the provider it reads. The layout lives in the config file.

**Visibility levels the core gives each provider**

- **Visible:** its screen is expanded. Refresh at the normal rate.
- **Background:** expanded view closed, but the provider feeds the peek or may pop. Event-driven or slow.
- **Hidden:** not placed anywhere and no pops enabled. Never runs.

| Provider | Source | While expanded | While in peek | Pops |
| --- | --- | --- | --- | --- |
| Time | System clock | Every 30 seconds | Once a minute, on the minute | No |
| Weather | Open-Meteo, city in settings | On expand, if older than 30 minutes | Nothing | No |
| Events | iCal link (Google Calendar: Settings → Integrate calendar → secret iCal address) | On expand, if older than 5 minutes | Every 5 minutes | "Starts in 10 minutes" |
| Activities and agent board | Activity store | Event-driven | Event-driven | Yes |
| Now playing | Windows media session events | Event-driven | Event-driven | Song change updates the cover; nothing pops |
| Timer | Built in, with a Pomodoro mode (focus sessions and breaks, "Session 2 of 4"); end time saved to disk | Every second | Ring updates every 5 seconds | "Time's up" |
| Volume | Windows Core Audio endpoint events | Event-driven | Nothing | No |
| Brightness | WMI (laptop screen); DDC/CI (external monitors) | Read on expand | Nothing | No |
| Windows notifications | Windows notification listener | Event-driven | Event-driven | Per app |
| Experimental toggles | Undocumented Windows settings | Read on expand | Nothing | No |
| GitHub CI and PRs | GitHub API with your token, plus ntfy webhooks | Every 60 seconds | Every 5 minutes while a run is in progress | Failed run, review requested |
| Dev servers | Windows TCP table plus HTTP health check on pinned ports | Every 5 seconds | Every 30 seconds, if crash pops are on | Server stopped |
| Downloads | File watcher on the Downloads folder | Event-driven | Event-driven | Optional |

**System screen shortcuts** open Windows Settings pages directly: Wi-Fi, Bluetooth, display, sound and power. Qapsule doesn't control Wi-Fi or Bluetooth itself.

**Known limits and risks**

- **Windows notifications** need Qapsule to have package identity (an MSIX package) plus your permission in Windows. Tauri's installers don't produce MSIX directly, so this needs a spike first (see Milestones). If it can't be done in time, launch without it.
- **Experimental toggles** (Do Not Disturb, night light, energy saver) have no public API. They rely on undocumented Windows internals that a Windows update can break. They live in one isolated module, are off by default, check that each change worked, and fall back to opening the Settings page.
- **External monitor brightness** works only on monitors that support DDC/CI, and each change takes a moment, so it runs off the UI thread.
- **Downloads** show "in progress" and "done", not a percentage: browsers don't tell other apps the final size.
- **Weather:** the free Open-Meteo API is aimed at non-commercial use. Check its terms before the paid Store build ships; keep the weather source swappable.
- **The iCal link** gives read access to your calendar, so it is stored like a password.

**Findings from the technical review (21 Oct)**

- **Audio bars:** a CSS loop that runs only while music is playing and stops on pause. Never capture the audio stream. Measure it in milestone 5; if it costs too much, show a still icon.
- **Agent progress:** coding agents report no percentage. Show an indeterminate bar and the elapsed time ("Working · 6 min").
- **Wi-Fi and Bluetooth:** drawn as shortcut buttons that open Settings. Windows has an official radio API, so real toggles are a spike for later.
- **Power button:** opens a small menu with Sleep, Restart and Shut down; shut down asks for confirmation.
- **Media position:** hide the song progress bar when the player doesn't report position and length.
- **Other apps' notifications:** a click opens the app, not the specific chat. Windows still shows its own toast unless the user turns off banners for that app; the setup guide must say so.
- **Quick reply is not possible:** Windows doesn't let one app use another app's reply action. Removed from the design.
- **Covered strip:** the pill hides a small strip of maximised windows (such as a browser tab bar). Auto visibility and Peek on demand are the answer; check the peek width during daily use.

## Window lifecycle and performance

The webview stays alive while the peek is on screen and is destroyed whenever Qapsule is fully out of sight. Every budget below is measured, published in the README, and checked before each release.

**When the webview lives**

- Auto and Always show modes: alive while the peek is visible.
- Hidden behind a fullscreen app (Auto), or tucked away (Peek on demand): destroyed after 10 seconds, recreated when needed.
- Recreating: render off-screen, wait for the UI's "ready" signal, then slide in. No white flash.

**One window, three sizes.** The window is created at the expanded size, and a window region (`SetWindowRgn`) matches the shape actually drawn. Clicks outside the visible pill pass through to the apps below, and growing from peek to pop to expanded is a CSS animation rather than a window resize, which avoids flicker. Prototype this in milestone 3; if regions misbehave, fall back to resizing the window.

**Window properties:** frameless, transparent, always on top, not in the taskbar or Alt+Tab, flush with the top edge of the primary monitor (above a top-docked taskbar), correct at 100%, 150% and 200% scaling.

**Budgets**

| Measure | Budget | How it's measured |
| --- | --- | --- |
| CPU, peek showing the time | Under 1% average | 10 minutes, Qapsule plus all its WebView2 processes |
| Memory, peek showing the time | Under 150 MB total (to confirm in milestone 5) | Qapsule plus all its WebView2 processes |
| CPU and memory, tucked away or hidden | Under 0.5% and 30 MB | Webview destroyed; Rust process alone |
| CPU, one activity updating | Under 3% average | Progress pushed 10 times a second for 2 minutes |
| Push to visible pop | Under 150 ms warm, 400 ms cold | From `qapsule push` to the first frame |
| Hover to expansion starting | Under 100 ms after the hover delay | Frame timing in the UI |
| UI updates | At most 4 per second | Coalesced in the core, whatever clients send |

**UI rules that protect the budget:** CSS transform and opacity transitions only, no JavaScript animation loops, no polling timers in the UI, honour Windows' reduce-motion setting.

**Fallback:** if peek memory or CPU misses its budget after tuning, draw the peek natively in Rust and use the webview only for pops and the expanded view.

## Security and privacy

Qapsule only talks to your own user account locally, stores secrets in Windows Credential Manager, and sends nothing about you anywhere.

- **Pipe access:** the named pipe's security descriptor allows only the current Windows user. Other users and remote machines can't connect.
- **No pushed command execution:** buttons from other apps can open http/https links, focus a window, or call back the pushing app. Only your own system-screen controls change system settings.
- **Secrets:** the GitHub token and the iCal link go into Windows Credential Manager, never into the config file or logs.
- **Windows notifications:** read only after you grant permission in Windows; never stored or logged; shown with or without details per app, as you choose.
- **Experimental toggles:** off by default, with a clear "may break after Windows updates" note; each change is verified and undone if Windows rejects it.
- **ntfy topic:** generated randomly (at least 128 bits) on first use, treated like a password, and never containing secrets in its messages.
- **Network calls:** only Open-Meteo, your iCal link, GitHub and ntfy, and only for widgets you have turned on. Listed in the README.
- **No telemetry.** Logs stay on disk; `qapsule doctor` builds a report the user can choose to send.
- **Untrusted text:** activity titles and notification text are rendered as plain text, never HTML, so nothing can inject script into the UI.

## Configuration

One TOML file at `%APPDATA%\Qapsule\config.toml`, editable by hand or through the settings and Edit screens UI. Qapsule reloads it when it changes.

```toml
[general]
hotkey = "Ctrl+Alt+Q"      # modifiers (at least one of Ctrl, Alt, Win) + key; "off" disables
visibility = "auto"        # auto | always | on_demand
expand_on = "click"        # click | hover
start_with_windows = false

[peek]
dual_status = true

[notifications]
do_not_disturb = false
agents       = { pop = true,  details = true }
music        = { pop = true,  details = true }   # kept for later; nothing pops for a song change
calendar     = { pop = true,  details = true, minutes_before = 10 }   # pops come from the reserved calendar: source
downloads    = { pop = false }
windows_apps = { pop = true,  details = false }  # needs Windows permission
"windows:WhatsApp"        = { pop = true, details = true }   # per app: spaces become dashes
"windows:Microsoft-Teams" = { pop = false }

[[screens]]   # left: your widgets
widgets = [
  { type = "agents", size = "wide" },
  { type = "timer",  size = "small" },
]

[[screens]]   # centre: main
widgets = [
  { type = "weather", size = "small", city = "Pretoria" },
  { type = "time",    size = "small" },
  { type = "events",  size = "small" },
]

[[screens]]   # right: system
widgets = [
  { type = "audio",     size = "wide" },   # volume, per-app volume, media
  { type = "shortcuts", size = "small" },  # brightness, dark mode, Settings links
]

[experimental]
toggles = false   # Do Not Disturb, night light, energy saver

[dev_servers]
pinned = [{ name = "web", port = 5173 }, { name = "api", port = 8080 }]
```

Secrets are never in this file. The iCal link is set with `qapsule calendar set-link`, which stores it in Windows Credential Manager (see docs/integrations/calendar.md). A wrong or unknown setting is logged and replaced by its default, so a typo can't stop Qapsule from starting.

## Project structure

A Cargo workspace where everything testable lives outside the Tauri app, so most code can be tested without opening a window.

```text
qapsule/
  crates/
    protocol/     # activity model + messages (public contract, MIT)
    core/         # activity store, priority, expiry, layout, event bus
    providers/    # weather, events, media, timer, github, devservers, downloads
    cli/          # the `qapsule` binary
  src-tauri/      # app shell: pipe server, window manager, tray, action router
  ui/             # React + TypeScript + Tailwind (Vite, Bun)
  integrations/   # hook configs for Claude Code, Codex, shell recipes
  docs/           # ARCHITECTURE.md, decision records, integration guides
  scripts/        # measure-idle.ps1 and other tooling
  AGENTS.md       # rules for AI coding agents working in this repo
```

**Key libraries:** `tokio` (async runtime and named pipes), `serde` / `serde_json`, `clap` (CLI), `windows` (Win32 and WinRT APIs: media, TCP table, windows and processes), `notify` (file watching), `reqwest` (HTTP), `keyring` (Credential Manager), `toml`. Confirm current versions when adding each.

**AGENTS.md holds the non-negotiables** for any AI assistant: Rust owns state; no JS animation loops or UI timers; every UI update goes through the core's coalescer; protocol changes need a decision record; never add a button that runs commands.

## Testing and measurement

Most logic is tested in plain Rust without a window; the rest is a short manual checklist run before each release.

| Layer | What's tested | How |
| --- | --- | --- |
| Protocol | Every message round-trips; unknown fields ignored; limits enforced | Unit tests, plus a folder of sample JSON files |
| Core | Priority order, expiry, update coalescing, visibility per provider | Unit tests with a fake clock |
| Providers | Parsing of Open-Meteo, iCal and GitHub responses | Unit tests against saved sample responses; no live network in CI |
| Pipe server and CLI | Push, update, end, callbacks, bad input, rate limits | Integration tests on the Windows CI runner |
| UI | Widgets render each state; snapshot then update | Component tests in Vitest |
| Performance | All budgets in the Budgets table | `scripts/measure-idle.ps1` locally before each release; results committed |
| Manual | Scaling at 100/150/200%, fullscreen hiding, hotkey, edit screens | Checklist in `docs/release-checklist.md` |

CI runs everything except performance and the manual checklist on every push. A pull request can't merge with failing tests.

## Milestones

Thirteen milestones from now to v1.0, each ending in something you can see working and show someone. Your top priority, agent status, arrives at milestone 4 on 6 December, early on purpose: once Qapsule is useful to you every day, motivation looks after itself.

| # | Milestone | Target date | What you'll see working | Share it |
| --- | --- | --- | --- | --- |
| 0 | It's public | Sun 25 Oct | The repo on GitHub with a README and licence | First build-in-public post |
| 1 | Skeleton | Sun 8 Nov | Workspace builds in CI; empty app opens | CI badge in the README |
|  | *Exams 9–17 Nov: no Qapsule work* |  |  |  |
| 2 | Hello, pipe | Sun 22 Nov | `qapsule push` in a terminal, then `qapsule list` shows it | Terminal screenshot |
| 3 | It hangs from the edge | Sun 29 Nov | The peek, flush with the top of the screen, showing the time; the three visibility modes; hidden in fullscreen | First GIF |
| 4 | Tap on the shoulder | Sun 6 Dec | Claude Code needs input, the pill pops with an Open terminal button; dual status with a `wrap` progress ring; per-source pop settings | GIF: "my agent now taps me on the shoulder" |
| 5 | Lean when idle | Thu 10 Dec | Peek CPU and memory measured and in the README; a one-day spike answers whether Windows notifications can work | The numbers |
| 6 | My day at a glance | Fri 18 Dec | Hover expands to the main screen: weather, time, events; now playing in the peek | Screenshot of your dashboard |
| 7 | v0.1 in public | Tue 22 Dec | A preview anyone can download from GitHub Releases | "What I built in my summer break" |
| 8 | Make it mine | Sun 10 Jan | Your widgets screen with edit mode; system screen with volume, per-app volume, brightness, dark mode and Settings shortcuts | Short video of the three screens |
| 9 | A stranger installs it | Sun 17 Jan | Install from winget, set up from the docs alone, demo video done; Windows notifications if the spike worked | Ask one friend to try it cold |
| 10 | Launch | Tue 19 Jan | Show HN and Reddit posts live, leading with the agent API | Launch posts |
| 11 | Power user | Sun 28 Feb | GitHub CI and PRs, dev servers, experimental toggles | Technical write-up on the idle-CPU work |
| 12 | v1.0 | Tue 30 Mar | Downloads; Windows notifications if not already in; protocol v1 frozen; paid Microsoft Store listing | v1.0 post |

**Rules for milestones**

- Each one ends with a short demo you record, even if it's rough. The recording is the reward.
- If a milestone slips, move the dates after it; never skip its demo.
- Tick it off in GitHub milestones, so the progress is visible on your profile.

December is the tightest stretch: the always-visible peek, pops and the main screen all land before v0.1. If milestone 6 slips, v0.1 ships with the peek and agent pops only, and the main screen follows on 10 January. Windows notifications are a stretch goal: launch without them if the spike says no.

## Decisions and open questions

Write each decision below as a short record in `docs/decisions/` yourself: context, options, choice, consequences. These are what interviewers will ask about.

**Decisions made (records to write)**

- [ ] Named pipe as the main transport, not local HTTP.
- [ ] Rust owns all state; the UI is a disposable view.
- [ ] Always-visible peek in a live webview, with a native peek as the fallback if budgets fail.
- [ ] One window with a click-through region, rather than resizing between states.
- [ ] Pushed buttons can't run commands; callbacks go to the pushing app instead.
- [ ] Wi-Fi and Bluetooth open Windows Settings rather than being controlled directly.
- [ ] Experimental toggles isolated, off by default, with a Settings fallback.
- [ ] Fully open source (MIT or Apache-2.0); money from Sponsors and the Store build.
- [ ] Calendar through an iCal link, not Google sign-in.

**Open questions**

- [ ] Peek design, to workshop: shape, size, colours, and exactly how the ¾ and ¼ split and swap.
- [ ] Should the peek show seconds, or a second time zone, when only the time is showing?
- [ ] Can Qapsule get package identity for Windows notifications while keeping the GitHub installer? Answered by the milestone 5 spike.
- [ ] Is a weather source with clear commercial terms needed before the paid Store build?
- [ ] Should third parties eventually define their own widgets, not just activities? Not before v1.0.
