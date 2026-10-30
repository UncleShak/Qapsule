# 6. Providers and reserved sources

Date: 2026-10-30 · Status: accepted

## Context

Live Windows notifications (`docs/plans/live-notifications.md`) are the first provider: activities that come from inside Qapsule, not from a pipe client. Three things the protocol and core don't cover yet:

- Provider activities have no pipe connection, so they can't own callbacks, and their buttons (open the app that sent the toast) must not become an action kind that pushed clients can use (AGENTS.md rule 5).
- A pipe client could push an activity named like a provider's ("Outlook: your password expires") and phish the user through the pill.
- Notification text is private (spec, Security and privacy): never stored or logged, and hidden when the user turns details off.

## Options

1. **New `ActionKind::OpenApp { aumid }`** in the protocol. Simple, but any pushed activity could then launch any installed app, which is close to running a command. Rejected.
2. **A separate provider pipeline with its own pop rules.** Duplicates priority, Do Not Disturb, expiry and the coalescer. Rejected.
3. **Provider activities are ordinary activities in a reserved source namespace, owned by no connection, whose buttons are routed back to the provider.** Chosen.

For the core's input interface, the larger `core.apply(Input)` refactor (architecture review, item 4) was considered and deferred: one narrow provider path now, revisit when the second provider lands.

## Choice

- **Reserved prefix.** Sources starting with `windows:` belong to Qapsule. A pipe `upsert` or `end` naming such a source is refused with `invalid_message` ("the windows: source prefix is reserved for Qapsule"). `clear` and `list` still accept it, so the user can clear notifications from the CLI. No new error code, so no version bump.
- **Source names.** One source per app: `windows:` followed by the app's display name with each run of whitespace turned into `-` and control characters dropped, cut so the whole key fits the 64-character limit (for example `windows:WhatsApp`, `windows:Microsoft-Teams`). Provider activities pass the same `Activity::validate` as pushed ones, so every protocol limit holds. The source label shown in the pill is the part after `windows:` with `-` turned back into spaces.
- **Settings.** `settings::category` maps every `windows:` source to `windows_apps`, so per-app entries override the category through the existing lookup:
  ```toml
  [notifications]
  windows_apps = { pop = true, details = false }
  "windows:WhatsApp" = { pop = true, details = true }
  ```
- **Provider input.** The core gets `provider_upsert(activity, now)` and `provider_remove(source, id, now)`. Provider activities have no owner and follow the same pop rules as pushed ones. Only sources with a reserved prefix go through this path.
- **Provider actions.** Pressing a button on a provider activity yields a new internal `Effect::Provider { source, id, action }`, which the engine hands to the provider (for notifications, `open` launches `shell:AppsFolder\<AUMID>`). This is not part of the protocol; `ActionKind` is unchanged and the Callback branch is unaffected.
- **Privacy.**
  - Notification text lives only in memory and is never written to disk or passed to `tracing`, `println` or error messages.
  - When a source's `details` setting is off, `list` replies give its `windows:` activities the source label as their title and no subtitle, matching what the pill shows. (Superseded: see Amendments, 2026-10-30.)
  - Text is plain text, as for every activity (spec, Untrusted text).

## Consequences

- Pipe clients see one new refusal: pushing to a `windows:` source. Existing clients are unaffected unless they used that prefix.
- Later providers (media, calendar, downloads) pick their own reserved prefix and reuse the same input path and `Effect::Provider`. Each new prefix is added to the reserved list in the same place and noted here.
- Display names can change with the system language, which would reset an app's per-app setting. Acceptable for now; revisit if it bites.
- Apps with more than 50 live notifications hit the per-source limit; the provider drops the oldest rather than failing.

## Amendments

**2026-10-30, after examination of the Phase 2 implementation**

- **Details off hides text everywhere.** For a `windows:` source with `details` off, pops, peek items and `list` replies all show the generic title ("New notification") and no subtitle. Peek items previously showed the full title. Pushed sources keep the old rule: details apply to pops only.
- **Reserved prefixes match without regard to case.** `Windows:WhatsApp` is refused like `windows:WhatsApp`.
- **The `app` icon is reserved.** A pipe `upsert` with `icon: "app"` is refused with `invalid_message`. Combined with the prefix rule, a pushed activity can't copy a provider notification's look exactly. It may still share a label (a pushed source named `WhatsApp`); that's accepted, because any process running as the user can already raise real Windows toasts, so the pipe is not a stronger phishing channel than Windows itself.
- **Provider buttons and ids carry no notification text.** Provider action labels are fixed strings (`Open`), action ids are fixed (`open`), and activity ids are the opaque Windows notification id. So with details off, nothing from the toast reaches the UI or `list` through labels or ids.
- **Undo can't revive a removed notification.** If Windows reports a notification removed while its dismissal can still be undone, the pending undo for it is dropped too.
- **Public surface.** `crates/protocol` exports `RESERVED_SOURCE_PREFIXES` and `is_reserved_source`; the matching helpers stay private to the crate.

**2026-10-30, after examination of the Phase 3 provider**

- **App names are private in logs.** Which apps send you notifications is personal, so app display names (and therefore `windows:` source keys) are never logged, like notification text. Logs carry error codes, HRESULTs and access statuses only.
- **Opening the sending app trusts the toast's AUMID within limits.** The Open button launches `shell:AppsFolder\<AUMID>`, where the AUMID comes from Windows' record of the toast. Packaged apps can't choose theirs; unpackaged apps can, but they already run as the user and could launch anything themselves, so this adds no new power. As defence in depth, AUMIDs containing `..` or `://`, or starting with `\`, are refused. Paths with single `\` stay allowed because browsers' AUMIDs are paths.

**2026-10-30, after the Archon's first test with real messages**

- **Notifications pop only.** Activities in reserved sources show as pops and never take a place in the peek. Reading a message in its app doesn't clear the Windows toast, so a notification would otherwise sit in the peek, above the clock, for its whole lifetime. They stay in the store until Windows removes them or they expire, for `list` and a future main-screen list.
