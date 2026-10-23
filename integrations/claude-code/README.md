# Claude Code integration

Shows each Claude Code session in Qapsule: working, waiting for you, done, or stopped with an error. When Claude needs your permission, the pill pops with **Open terminal**, which brings the right window to the front.

## Set up

1. Put `qapsule.exe` on your `PATH` (for now: `cargo build --release -p qapsule-cli`, then copy `target\release\qapsule.exe` to a folder on your `PATH`).
2. Copy the `hooks` block from [`settings.json`](settings.json) into `~/.claude/settings.json` (all projects) or `.claude/settings.json` in one project. If the file already has a `hooks` block, merge the events into it.
3. Start Qapsule, then start Claude Code.

Each hook runs `qapsule hook claude-code`, which reads the event Claude Code sends on stdin. It prints nothing and always succeeds, so it can never block or slow Claude down, even when Qapsule isn't running.

Keep the `args` form shown, which starts `qapsule` directly from Claude Code. **Open terminal** finds your terminal by following the chain of processes from the hook up to the window; Git Bash's `bash -c` loses its place in that chain, so a shell-form hook (`"command": "qapsule hook claude-code"` without `args`) may push fine but fail to bring the terminal forward.

With Windows Terminal or VS Code, **Open terminal** brings that window to the front; it can't pick a particular tab.

## What each event does

| Claude Code event | In Qapsule |
| --- | --- |
| `UserPromptSubmit` | Running: "Claude is working", with the folder name and elapsed minutes |
| `PostToolUse` | Running again (after you approve a tool, the waiting pop goes away) |
| `Notification` (`permission_prompt`, `elicitation_dialog`, `agent_needs_input`) | Needs input: pops "Claude needs you" with Claude's message, until you answer |
| `Stop` | Done: pops briefly, then leaves |
| `StopFailure` | Stopped with an error: pops briefly, then leaves |
| `SessionEnd` | Removed |

Other notification types, such as `idle_prompt`, are ignored. Claude Code sends `permission_prompt` only after a prompt has waited about six seconds, so quick answers don't pop.

Pop settings live in `%APPDATA%\Qynetik\Qapsule\config.toml`:

```toml
[notifications]
agents = { pop = true, details = true }   # details = false shows only "Needs your input"
```

## When it breaks

Hook names change between Claude Code versions. The mapping is in `crates/cli/src/hook.rs`; check it against the [hooks reference](https://code.claude.com/docs/en/hooks) after a Claude Code update.
