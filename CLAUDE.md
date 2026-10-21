@AGENTS.md

## Claude-specific

- This is a Windows project; shell commands run in PowerShell or Git Bash, so use forward slashes in Bash and avoid Unix-only tools in scripts meant for PowerShell.
- Before calling a task done, run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`, and `bun run --cwd ui build` if `ui/` changed.
