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
- [ ] The expanded view fits, centred, with nothing cut off.

## Behaviour

- [ ] Auto: the pill hides behind a fullscreen video, and stays visible over a maximized window.
- [ ] Peek on demand: tucked away, comes out on edge hover and on Ctrl+Alt+Q, and goes back.
- [ ] After 15 seconds tucked away, Task Manager shows no WebView2 processes under Qapsule; a pop then still appears.
- [ ] A Claude Code permission prompt pops "Claude needs you", and **Open terminal** brings the right window forward.
- [ ] Dismiss, then Undo, brings the pop back.
- [ ] Quit from the tray menu closes Qapsule completely.
- [ ] Hovering the peek slowly opens the expanded view; sweeping past quickly doesn't; leaving closes it after a moment.
- [ ] With `expand_on = "click"`, hover doesn't open it and a click on the peek does.
- [ ] Ctrl+Alt+Q opens it and takes keyboard focus; Esc closes it and focus returns to the previous window. Opening by hotkey, clicking another app, then the hotkey again leaves that app focused.
- [ ] The tray shows "Hotkey: Ctrl+Alt+Q". Setting `hotkey = "Ctrl+Alt+K"` in config.toml moves it without a restart; a combo another app holds shows "Hotkey ... is taken" in the tray; `hotkey = "off"` shows "Hotkey off" and the old combo stops working.
- [ ] A pop arriving while the view is open shows after it closes, for its full time.
- [ ] Weather shows your city's forecast; with no city it asks for one.
- [ ] Events show now and next from `qapsule calendar set-link`; a test event pops 10 minutes before it starts.
