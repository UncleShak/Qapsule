# Qapsule Roadmap

Oct 18, 2026 · @Nadiya

## Overview

Qapsule is open source and public on GitHub from this week, ships a v0.1 developer preview on Tuesday 22 December 2026, and launches publicly on Tuesday 19 January 2027.

Qapsule is a Windows 11 status hub that other apps, scripts and AI coding agents push live updates into. It stays invisible when nothing is running, shows a small pill when something is, and opens into a three-screen widget dashboard when you ask for it. The pitch in one line: **see what your builds and agents are doing without switching windows, at under 1% idle CPU.**

Features, architecture and the 13 milestones are in the Qapsule Technical Spec. Each phase below names the milestones it delivers.

**Built around your university year.** Studies come first; the heavy building happens in the summer break.

| Period | Dates | What's happening | Qapsule hours a week |
| --- | --- | --- | --- |
| Term | 19 Oct – 8 Nov 2026 | Lectures, two tests on Sat 24 Oct, COS 214 project | About 5 |
| Exams | 9 – 17 Nov 2026 | COS 284 (11 Nov), COS 214 (12 Nov), COS 226 (14 Nov), WTW 285 (16 Nov), STC 122 (17 Nov) | 0 |
| Summer break | 18 Nov – 22 Dec 2026 | Free | About 30 |
| Holidays | 23 Dec 2026 – 3 Jan 2027 | Rest | 0 |
| Summer break | 4 Jan – end of Jan 2027 | Free | About 30 |
| Next semester | From early Feb 2027 | Classes | About 5 |

**Decisions already made**

- Fresh codebase on Tauri v2, Rust and React, not the Bloom fork.
- Fully open source under MIT or Apache-2.0: free code signing, free CI and hosting, and a public portfolio piece.
- Money without closing the source: GitHub Sponsors from launch, then a paid Microsoft Store build ($3–5) alongside v1.0. GitHub downloads stay free.
- No servers. Webhooks go through ntfy.sh, which is free.

**Open question:** when is the COS 214 project due? This plan assumes before exams. If it's later, Phase 1 moves back by the same amount.

## Timeline at a glance

&#91;embedded content: Qapsule roadmap · 6 phases around the university year\]

Each phase ends with a gate you must pass before the next starts; the dashed line marks launch day.

## Principles for a solo developer

1. **University comes first.** No Qapsule work in exam weeks. A slow week during term is normal, not failure.
2. **Ship the smallest thing that proves the pitch.** The pitch is "my agent's status, live, at zero idle cost." Every feature that doesn't serve it waits until after launch.
3. **Rust owns all state; the webview is disposable.** This one rule is what makes the 1% budget reachable.
4. **Use it yourself every day from Phase 2 on.** You are a developer running long tasks and AI agents. You are the first user.
5. **Keep a cut list.** When a phase runs late, cut scope, never the gate. Ideas go on the After 1.0 list, not into the current phase.
6. **Work in public.** The repo is public from the first commit. Clear commit messages, issues and milestones are part of the portfolio.
7. **Spend nothing.** Everything in this plan runs on free tiers: GitHub, SignPath, ntfy.sh and GitHub Pages.

## Phase 0: Foundations (19 Oct – 8 Nov 2026, milestones 0–1)

Three light weeks, about 5 hours each, around your tests and the COS 214 project. Milestone 0, "It's public", by Sun 25 Oct; milestone 1, "Skeleton", by Sun 8 Nov. Small tasks only, each doable in one sitting, and nothing at all from 9 November until your last exam on 17 November.

**Week of 19 Oct** (tests on Sat 24 Oct, so keep it to about 2 hours)

- [ ] Create the public GitHub repo with an MIT or Apache-2.0 licence.
- [ ] Write a one-page README with the pitch and a "work in progress" note.
- [ ] Name check: search for conflicts on "Qynetik" and "Qapsule"; check that the CLI name (`qapsule`) is free on crates.io, winget and Scoop.

**Weeks of 26 Oct and 2 Nov**

- [ ] Create a fresh Tauri v2 project with React, TypeScript, Tailwind, Vite and Bun.
- [ ] Turn it into a Cargo workspace: `crates/protocol`, `crates/cli`, `src-tauri`.
- [ ] Re-apply your earlier work: app name, transparent background, Mica or Acrylic test.
- [ ] GitHub Actions on a Windows runner that builds and runs tests on every push.
- [ ] Turn the phases of this roadmap into GitHub milestones and issues.
- [ ] Apply for the GitHub Student Developer Pack, if you haven't already.

**Gate:** the public repo builds in CI with all three crates compiling. If the COS 214 project needs the time, this slips into the week of 18 November; that's fine.

## Phase 1: Core engine and first pill (18 Nov – 6 Dec 2026, milestones 2–4)

Starts the day after your last exam. Two and a half full-time weeks that end with your top-priority feature working: an agent tapping you on the shoulder.

**Milestone 2, "Hello, pipe" (Sun 22 Nov)**

- [x] Protocol crate: activity fields, messages and action kinds as in the spec, with unit tests and sample JSON files.
- [x] Core crate: activity store with priority, expiry and update coalescing, tested with a fake clock.
- [x] Named-pipe server, locked to the current Windows user.
- [x] CLI: `push`, `done`, `fail`, `list`, `clear`, `wrap`.

**Milestone 3, "It hangs from the edge" (Sun 29 Nov)**

- [x] Window manager: one window flush with the top edge, with a click-through region matching the visible pill (fall back to resizing if regions misbehave).
- [x] Peek showing the time, updated once a minute.
- [x] Visibility modes: Auto, Always show, Peek on demand; hidden in fullscreen apps under Auto.

**Milestone 4, "Tap on the shoulder" (Sun 6 Dec)**

- [x] Pop notifications: the pill grows with up to two buttons, then shrinks back.
- [x] Dual status: ¾ main item, ¼ progress ring, click to swap. `qapsule wrap` shows as the ring.
- [x] `focus_window` action, including the parent-process walk that finds the terminal window.
- [x] Per-source pop settings: pop or not, details or not.
- [x] Claude Code hook config in `integrations/`, mapping its events to running, needs input and done.
- [x] Write the first three decision records: named pipe, Rust owns state, one window with a click-through region.

**Gate:** Claude Code asks for input, the pill pops to say so, and one click brings the right terminal to the front.

## Phase 2: Lean peek, main screen and v0.1 (7 – 22 Dec 2026, milestones 5–7)

Two weeks to prove the 1% promise, build your main dashboard screen, and put a first release on GitHub before the holidays.

**Milestone 5, "Lean when idle" (Thu 10 Dec)**

- [x] Webview lifecycle: alive while the peek shows; destroyed 10 seconds after being hidden or tucked away; off-screen render and slide-in on recreate.
- [ ] Correct at 100%, 150% and 200% scaling.
- [x] `scripts/measure-idle.ps1`; peek CPU and memory, and every other budget in the spec, measured and put in the README.
- [x] One-day spike: can Qapsule get package identity and read Windows notifications? Decide: launch feature or later. Answer: not unless SignPath signing is ready by milestone 9; otherwise v1.0 (docs/spikes/windows-notifications.md).

**Milestone 6, "My day at a glance" (Fri 18 Dec)**

- [ ] Hover (300 ms) expands to the main screen; moving away collapses it; hotkey pins it open.
- [ ] Main screen: weather (Open-Meteo, city in settings), time, current and next events (iCal link in Credential Manager), with the "starts in 10 minutes" pop.
- [ ] Now playing in the peek, with optional song-change pops.
- [ ] Tray icon: Do Not Disturb, start with Windows, quit. Config file with reload.

**Milestone 7, "v0.1 in public" (Tue 22 Dec)**

- [ ] Unsigned installer on GitHub Releases, clearly labelled "preview".
- [ ] A short post with a GIF: "what I built in my summer break."

**Fallback:** if milestone 6 slips, v0.1 ships with the peek and agent pops only, and the main screen moves to 10 January.

**Gate:** idle CPU under 1% over 10 minutes, and v0.1 is downloadable from GitHub.

## Phase 3: Your screens and launch prep (4 – 18 Jan 2027, milestones 8–9)

Two weeks after the holidays: make the dashboard yours, then make Qapsule installable by strangers. Anyone can try the v0.1 preview meanwhile and report problems as GitHub issues.

**Milestone 8, "Make it mine" (Sun 10 Jan)**

- [ ] Your widgets screen (left) and system screen (right), switched by mouse wheel, arrow keys, dots or touchpad swipe.
- [ ] Edit mode: drag to swap, × to remove, + to add; one to three widgets per screen.
- [ ] Your widgets: agent board and timer.
- [ ] System screen: volume and per-app volume, brightness (laptop, plus DDC/CI monitors where supported), dark mode, and shortcuts to the Wi-Fi, Bluetooth, display, sound and power Settings pages.
- [ ] Codex CLI integration, plus shell recipes for PowerShell, cmd and Git Bash.

**Milestone 9, "A stranger installs it" (Sun 17 Jan)**

- [ ] Apply to SignPath Foundation for free code signing early in the phase; ship unsigned with a clear note if approval hasn't come through.
- [ ] Auto-update through the Tauri updater and GitHub Releases; publish to winget and Scoop.
- [ ] `qapsule doctor` for diagnostic reports. No telemetry.
- [ ] Landing page on GitHub Pages, a 30–60 second demo video, and launch posts written in advance.
- [ ] Ask one friend to install it using only the docs, and fix whatever trips them up.
- [ ] If the milestone 5 spike succeeded: Windows notifications in the peek, with per-app pop and detail settings. Otherwise, move it to v1.0 and launch without it.

**Gate:** a fresh machine goes from install to a live Claude Code status in under five minutes, following only the docs.

## Phase 4: Public launch (19 Jan – end of Jan 2027, milestone 10)

Launch day is Tuesday 19 January 2027, leaving about two weeks of break to handle the response before classes start.

**Launch day**

- [ ] Show HN post in the morning US Eastern time (afternoon in South Africa). Stay online to answer every comment.
- [ ] Posts on r/Windows11, r/ClaudeAI and your build-in-public channels.
- [ ] Tag the release v0.2 with a changelog.

**The week after**

- [ ] Fixes only, no new features. Reply to every issue within a day.
- [ ] Publish the technical write-up: how the webview gets to near-zero idle CPU. Post it to r/rust and link it from the README. This is the piece to put in job applications.
- [ ] Turn on GitHub Sponsors (South Africa is a supported region) and add a Sponsor button to the README.

**Gate:** 100 GitHub stars and 25 people using it in the second week. Below that, talk to a few users before building more.

## Phase 5: v1.0 at semester pace (Feb – 30 Mar 2027, milestones 11–12)

v1.0 ships on Tuesday 30 March 2027. Back at university, so this phase is maintenance plus a few well-chosen improvements from user feedback.

About 5 hours a week.

**Milestone 11, "Power user" (Sun 28 Feb)**

- [ ] GitHub CI and pull request widget, with a token stored in Credential Manager and a re-run button.
- [ ] Webhooks through ntfy.sh on a long, random topic, so failed runs reach the pill instantly without a server.
- [ ] Dev servers widget: pinned ports, health checks, crash alerts.
- [ ] Publish the technical write-up on the idle-CPU work, if not done at launch.
- [ ] Experimental toggles for Do Not Disturb, night light and energy saver: one isolated module, off by default, each change verified, falling back to the Settings page.

**Milestone 12, "v1.0" (Tue 30 Mar)**

- [ ] Downloads widget.
- [ ] Fix the top issues from launch and beta users.
- [ ] Freeze protocol v1: no breaking changes from here, so integrators can rely on it.
- [ ] v1.0 release post with what changed since launch.
- [ ] Windows notifications, if they didn't make the launch.

**Portfolio**

- [ ] `ARCHITECTURE.md`: how the parts fit and why.
- [ ] Short decision records for the big choices: named pipes vs HTTP, destroying vs suspending the webview, open source vs paid.
- [ ] Pin the repo on your GitHub profile; add Qapsule to your CV with the measured CPU number and user count.
- [ ] Prepare a two-minute verbal walkthrough for interviews: the problem, your design, one hard bug, the result.

**Money**

- [ ] Register a Microsoft Store individual developer account. It's free and needs only a Microsoft account plus ID and selfie verification; start early in case verification is slow.
- [ ] Package Qapsule as an MSIX, so the Store hosts, signs and auto-updates it for free.
- [ ] Publish a paid Store listing at $3–5 on v1.0 day, described as the one-click, auto-updating build.
- [ ] Keep the GitHub Releases download free, and say so in the README and the Store listing.
- [ ] Only if users hit the free ntfy.sh limits: consider a paid hosted relay. The code stays open; the subscription pays for the server.
- [ ] Skip closed "Pro" features. They would break SignPath's free signing, which requires a fully open-source project.

**Gate:** v1.0 released, and you can explain every major design decision in an interview.

## After 1.0: the idea list

This is where new ideas go while you're building. Order them by what users ask for most after launch.

| Idea | Why it matters | Effort |
| --- | --- | --- |
| Node and Python SDKs | Lets apps push without shelling out to the CLI | Small |
| VS Code extension | Puts build and task status in Qapsule automatically | Medium |
| Raycast and Command Palette extensions | A distribution channel the research ranked 6.5 | Small |
| Third-party widgets | Lets other apps add widgets, not just activities | Medium |
| Natively drawn pill | Only if Phase 2 measurements demand it | Large |
| Self-hosted ntfy relay | Only if the free ntfy.sh limits become a problem; could run on student credits | Medium |
| Drag-and-drop shelf with phone handoff | Research opportunity #3, shares the same top-of-screen surface | Large |

## Risks and responses

The biggest risk for a first solo project is not technical: it's losing momentum when the work stops being new.

| Risk | Early warning sign | Response |
| --- | --- | --- |
| Losing momentum | Two weeks with no commits | Shrink the next task until it fits one evening; post about it anyway |
| Scope creep | A phase runs more than two weeks late | Move features to the idea list; keep the gate |
| Webview misses the CPU budget | Phase 2 numbers above 1% with one activity | Lower the update rate first; then build a native pill |
| Hook formats change in Claude Code or Codex | An integration stops working after a tool update | Keep integrations as docs and config, so a fix is a docs edit |
| Island apps add an API | A competitor announces push support | Lean on the developer focus: CLI, `wrap`, agent integrations, measured CPU |
| Microsoft ships a similar system surface | An Insider build or blog post | Integrate with it if possible; the CLI and integrations still have value |
| Weak demand | Under 25 users two weeks after launch | Talk to 10 target users before writing more code |
| Coursework or exams squeeze the plan | A phase misses its gate during term | Move the dates, never the study time; the summer break carries most of the work |

## Staying committed

A fixed weekly rhythm matters more than motivation. Scale it to the period: a single short session in term, real build days in the break.

1. **Monday: pick tasks** from the current phase. One during term, three in the break.
2. **Fixed build hours** in your calendar, treated like lectures. None in exam weeks.
3. **Weekend: ship and post.** A commit, a screenshot or a short note on what you learned, even if the week was slow.

**Numbers to track**

| Metric | From | Target |
| --- | --- | --- |
| Weeks with a commit, outside exams | Phase 0 | Every week |
| Idle CPU | Phase 2 | Under 1% |
| Preview downloads | v0.1, 22 Dec | 25 |
| GitHub stars | Launch, 19 Jan | 100 |
| Weekly active users | Phase 4 | 25 or more |
| Outside contributors or issue reporters | Phase 5 | 5 |
| GitHub sponsors | Launch, 19 Jan | First 5 |
| Microsoft Store sales | v1.0, 30 Mar | First 20 |

The targets are starting points for a first launch. Adjust them after the v0.1 preview, when you have real numbers.
