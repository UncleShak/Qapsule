# 4. Protocol v1

Date: 2026-10-23 · Status: accepted

## Context

AGENTS.md requires a decision record for protocol changes. This record covers the first version, implemented in `crates/protocol` following the spec, and the details the spec left open.

## Options

For each open detail, the choice below was picked over leaving it to each client or the app to guess.

## Choice

- Newline-delimited JSON on `\.\pipe\qapsule` (the spec's `qynetik-capsule` name predates the rename). Every message carries `"v": 1`; a missing or different `v` gets `unsupported_version`.
- Client messages: `hello`, `upsert`, `end`, `clear`, `list`. Server messages: `ok`, `error` (with a `code`), `list_result`, `action`.
- An activity is identified by `source` plus `id`, so `end` and `clear` name both.
- `end` takes `succeeded` (default) or `failed`, and may replace the title and subtitle. `clear` removes one activity, a source, or everything; the CLI asks for `--all` before clearing everything.
- Error codes: `invalid_json`, `invalid_message`, `unsupported_version`, `too_large`, `rate_limited`, `too_many_activities`, `not_found`.
- Limits: 64 KB per message, 20 messages a second per connection, 50 live activities per source, title 80 and subtitle 120 characters, ids and sources 1 to 64 characters with no spaces, up to 2 actions with at most one primary, `expires_in` from 1 second to 1 day.
- `open_url` accepts only `http` and `https`. There is no action that runs a command (AGENTS.md rule 5).
- Unknown fields are ignored; unknown message types, states and action kinds are refused.
- Samples of valid and invalid messages live in `crates/protocol/samples/` and are checked by the tests.

## Consequences

- Clients can rely on precise, tested error codes.
- Adding fields is backwards compatible; adding a new action kind or state is not, for older Qapsule versions, so it must wait for a deliberate version bump or be ignored by older apps.
