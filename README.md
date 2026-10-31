# Qapsule

[![CI](https://github.com/UncleShak/Qapsule/actions/workflows/ci.yml/badge.svg)](https://github.com/UncleShak/Qapsule/actions/workflows/ci.yml)

**See what your builds and agents are doing without switching windows, at under 1% idle CPU.**

>**Work in progress.** Qapsule is being built in public. Nothing is installable yet; the first developer preview (v0.1) is planned for **22 December 2026**. Star or watch the repo to follow along.

Qapsule is a small hub for Windows 11 that hangs from the top centre of your screen. At a glance it shows the time, your music, your next meeting and the weather, with quick controls for volume, brightness and more. And unlike other Windows islands, apps, scripts and AI coding agents can push live updates into it, so you can glance up instead of alt-tabbing to check whether your build finished or your agent is waiting for you.

## Why

If you run long builds or AI agents like Claude Code and Codex, you know the loop: start a task, switch to something else, forget about it, come back ten minutes later to find it's been waiting for your input since minute two.

Qapsule fixes that. When your agent needs you, the pill pops to say so, and one click brings the right terminal to the front.

## What it will do

### The pill

- **Peek view:** a small pill that shows the time, or whatever is happening right now: the song that's playing, a download, an agent at work.
- **Two things at once:** when more than one thing is going on, the most important takes most of the pill and the next shows as a small ring or icon. Click it to swap.
- **Pop notifications:** the pill briefly grows to show something you've allowed, with up to two buttons, then shrinks back.
- **Stays out of your way:** hides when you're watching a video or playing a game fullscreen. Bring it back by moving your mouse to the top of the screen or pressing **Ctrl+Alt+Q**.

### Three screens

Hover the pill and it opens into a dashboard. Scroll, swipe or use the arrow keys to switch screens.

| Your widgets (left) | Your day (centre) | System (right) |
| --- | --- | --- |
| Timer | Weather for your city | Volume, mute and per-app volume |
| Agent board | Time and date | Music controls |
| Downloads | Current event and the coming week | Brightness, including supported external monitors |
| GitHub and dev servers (for developers) | | Dark mode toggle |
| | | Quick links to Wi-Fi, Bluetooth, display, sound and power settings |

Make it yours: right-click to drag widgets around, remove the ones you don't use, or add new ones.

### Everyday notifications

- **Music:** see what's playing, skip or pause without opening your player, and optionally get a pop when the song changes.
- **Calendar:** a heads-up 10 minutes before your next event. Works with Google Calendar and anything else that offers an iCal link. Set it up with `qapsule calendar set-link` (docs/integrations/calendar.md).
- **Timer:** a pop when time's up.
- **Downloads:** know when a file has finished.
- **Other apps:** Windows notifications in the pill (planned, depending on what Windows allows).
- **Do Not Disturb:** hold every pop until you're ready, from the tray icon.

You decide what may interrupt you: every source has its own switch for whether it pops and whether it shows details or just its name.

### For developers

- **AI agents:** Claude Code and Codex tell you when they're working, done, or need your input. One click brings the right terminal to the front.
- **Long commands:** wrap any command to see it as a progress ring and get a pop when it finishes.
- **Push from anything:** a local CLI and API that any app or script can send live updates to, with action buttons.

## Pushing updates (developers)

Anything can push an activity through the `qapsule` CLI or a local named pipe:

```powershell
# Show a long command as a progress ring, and pop when it finishes
qapsule wrap -- cargo build --release

# Tell Qapsule your agent needs you, with a button that focuses its terminal
qapsule push --id cc-4821 --source claude-code --title "Claude needs your input" --state needs-input --focus-pid 4821

# Clear it once you're done
qapsule done --id cc-4821
```

Ready-made hook configs for Claude Code and Codex mean you won't have to write these yourself.

## Built to stay out of the way

- **Lean when idle:** under 1% CPU with the peek showing. Memory and CPU are measured before every release and the numbers are published here.
- **Local only:** the pipe accepts connections from your own Windows user only. No accounts, no cloud, no telemetry.
- **Safe buttons:** apps can push buttons that open a link, focus a window or call back to the app. They can never run commands.

### Measured

Every number below comes from [`scripts/measure-idle.ps1`](scripts/measure-idle.ps1) on a release build, counting Qapsule and all its WebView2 processes. Measured 23 Oct 2026 on a Ryzen 7 7730U laptop (16 threads, 16 GB) running Windows 11 build 26200. CPU is a share of the whole machine, as Task Manager shows it.

| Situation | Budget | Measured |
| --- | --- | --- |
| Peek showing the time, 10 min | CPU under 1%, memory under 150 MB | **0.02% CPU, 121 MB** |
| Progress pushed 10 times a second, 2 min | CPU under 3% | **0.13% CPU** |
| Tucked away or behind a fullscreen app, 10 min | CPU under 0.5%, memory under 30 MB | **0.04% CPU, 9.6 MB** on average (2.7 MB, 1 process, between visits) |
| Pop, from `qapsule push` to the first frame | 150 ms warm, 400 ms cold | **about 140 ms** warm, **590–690 ms** cold (over budget) |

"Cold" means the capsule had been out of sight for over 10 seconds, so its webview had been closed to save memory and has to start again. See [decision 5](docs/decisions/0005-destroy-the-webview-when-hidden.md).

## Tech

Rust and [Tauri v2](https://v2.tauri.app/) for the core, React and TypeScript for the UI. A single Rust process owns all state; the webview is a disposable view that can be destroyed when Qapsule is out of sight.

## Roadmap

| Milestone | Target | What you'll see |
| --- | --- | --- |
| It's public | Oct 2026 | This repo ✅ |
| Hello, pipe | Nov 2026 | `qapsule push` and `qapsule list` working |
| Tap on the shoulder | Dec 2026 | Agents pop the pill when they need your input |
| **v0.1 preview** | **22 Dec 2026** | First download on GitHub Releases |
| **Public launch** | **19 Jan 2027** | Three screens, system controls, winget and Scoop |
| **v1.0** | **30 Mar 2027** | GitHub CI, dev servers, downloads; protocol frozen |

Progress is tracked in [GitHub milestones](../../milestones).

## Follow along

I'm building Qapsule as a solo student project between lectures, and posting progress as I go. Ideas and feedback are welcome in [issues](../../issues).

## Code signing policy

**Status: applying.** Qapsule will apply to the [SignPath Foundation](https://signpath.org) for free code signing as soon as the first release is out (planned for 22 December 2026). Until then, downloads are unsigned and labelled "preview".

Once approved: *Free code signing provided by [SignPath.io](https://signpath.io), certificate by [SignPath Foundation](https://signpath.org).*

- **What is signed:** the Windows installer and app published on [GitHub Releases](../../releases), and the small identity package that lets Qapsule show other apps' notifications.
- **How:** release files are built by GitHub Actions from this public repository and sent to SignPath for signing. Nobody on the project holds the private key. Each release is signed only after manual approval.
- **Team roles** (a single-maintainer project for now):
  - Committers and reviewers: [@UncleShak](https://github.com/UncleShak). Every pull request from anyone else is reviewed before it's merged.
  - Approvers: [@UncleShak](https://github.com/UncleShak), who approves each signing request.
- **Privacy:** this program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it. Qapsule only connects to the services behind widgets you turn on (Open-Meteo for weather, your calendar's iCal link, GitHub, and ntfy.sh for webhooks), and has no telemetry.

## License

Qapsule is open source under the [MIT License](LICENSE).