# Windows notifications

Qapsule pops other apps' toasts (WhatsApp, Teams, Outlook…) in the pill. For now this works only on a development machine, with a development identity. Shipping to everyone waits for SignPath or the Microsoft Store.

## Set up (once)

1. Turn on Settings → System → For developers → Developer Mode.
2. Build the app:

```powershell
cargo build -p qapsule-app
```

3. Register the development identity:

```powershell
./scripts/identity/register-identity.ps1
```

4. Check the identity:

```powershell
./target/debug/qapsule-app.exe --identity-check
```

Expect a package identity line, "subscribe ok", and real (not virtualised) writes.

5. Run the app:

```powershell
bun run tauri dev
```

The first time, Windows asks whether Qapsule may read your notifications. Say yes.

## Check it's working

The tray menu shows "Windows notifications: on".

- "allow in Settings…" means access was refused. Click it, turn Qapsule on in the Settings page that opens, then restart Qapsule.
- "unavailable" means there is no identity. Rerun `register-identity.ps1` and the identity check.

## Settings

```toml
[notifications]
windows_apps = { pop = true, details = false }
"windows:WhatsApp" = { pop = true, details = true }
"windows:Microsoft-Teams" = { pop = false }
```

- `windows_apps` sets the default for every app.
- Per-app keys are `"windows:<App-Name>"`, with spaces turned into dashes. Microsoft Teams becomes `"windows:Microsoft-Teams"`.
- `details = false` shows "New notification" instead of the text.

Config lives at `%APPDATA%\Qapsule\config.toml`. Edits take effect within a second, without a restart.

## What to expect

- Notifications pop and then go. They never stay in the peek.
- Windows still shows its own banner too. To stop that, turn off banners for that app in Settings → System → Notifications. Keep notifications themselves on, or Qapsule won't see them.
- Open launches the app, not the specific chat or email.
- Toasts already in the notification centre when Qapsule starts don't pop.
- Do Not Disturb holds pops like any other.

## Privacy

Notification text and app names stay in memory only. They are never written to disk or logs.

## Fallback: signed identity

If Developer Mode isn't available, use a self-signed certificate instead. Run the trust step from an elevated shell (Run as administrator).

```powershell
./scripts/identity/new-dev-cert.ps1
./scripts/identity/trust-dev-cert.ps1
./scripts/identity/build-identity.ps1
./scripts/identity/register-identity.ps1 -Signed
```

To undo the machine-wide trust, run `untrust-dev-cert.ps1` from an elevated shell.

## Measuring with the listener on

Release builds carry the identity only on request, so measure one built with it:

```powershell
$env:QAPSULE_DEV_IDENTITY = '1'; bun run tauri build --no-bundle
./scripts/identity/register-identity.ps1 -Release
./scripts/measure-idle.ps1 -Scenario idle -Seconds 600
```

Afterwards, run `./scripts/identity/register-identity.ps1` again to point the identity back at the debug build. Never ship a build made with `QAPSULE_DEV_IDENTITY`.

## Remove it

```powershell
./scripts/identity/unregister-identity.ps1
```
