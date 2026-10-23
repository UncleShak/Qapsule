# Release checklist

Run before every release, on a release build (`bun run tauri build --no-bundle`). The automated tests cover the logic; this covers what only a real screen can show.

## Measure

- [ ] `./scripts/measure-idle.ps1 -Scenario idle -Seconds 600` with nothing happening and the peek showing the time (Auto mode). Budget: CPU under 1%, memory under 150 MB.
- [ ] `./scripts/measure-idle.ps1 -Scenario activity -Seconds 120`. Budget: CPU under 3%.
- [ ] Switch to Peek on demand, wait 15 seconds, then `./scripts/measure-idle.ps1 -Scenario hidden -Seconds 600`. Budget: CPU under 0.5%, memory under 30 MB, and exactly 1 process.
- [ ] Put the numbers in the README's "Measured" table.

## Display scaling

Repeat at 100%, 150% and 200% (Settings → System → Display → Scale), changing the scale while Qapsule is running:

- [ ] The pill sits centred, flush with the top edge; no gap and nothing cut off.
- [ ] Shapes and text are sharp, not blurry.
- [ ] Hover stretches the shape; clicks on the pill work; clicks just beside it reach the app underneath.
- [ ] In Peek on demand, touching the top centre brings the pill out.

## Behaviour

- [ ] Auto: the pill hides behind a fullscreen video, and stays visible over a maximized window.
- [ ] Peek on demand: tucked away, comes out on edge hover and on Ctrl+Alt+Space, and goes back.
- [ ] After 15 seconds tucked away, Task Manager shows no WebView2 processes under Qapsule; a pop then still appears.
- [ ] A Claude Code permission prompt pops "Claude needs you", and **Open terminal** brings the right window forward.
- [ ] Dismiss, then Undo, brings the pop back.
- [ ] Quit from the tray menu closes Qapsule completely.
