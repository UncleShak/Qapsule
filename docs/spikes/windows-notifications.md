# Spike: can Qapsule show other apps' Windows notifications?

Date: 2026-10-23 · Milestone 5 · Time spent: about two hours

## Question

Can Qapsule read toast notifications from other apps (WhatsApp, Teams, Outlook…) and show them in the pill, while keeping the free GitHub installer? Should it be a launch feature or wait until later?

## What Windows offers

The only supported API is `UserNotificationListener` (WinRT). After the user allows it, an app can read other apps' toasts and subscribe to `NotificationChanged` to hear about new ones. Microsoft documents it for apps with **package identity**: either a full MSIX package, or a normal installer plus a small signed **identity package** registered "with external location" (also called a sparse package).

## What I tried

A throwaway Rust program, run unpackaged on the development machine (Windows 11), the way Qapsule runs today:

| Call | Result |
| --- | --- |
| `Package::Current()` | No identity, as expected |
| `RequestAccessAsync()` | `Allowed` |
| `GetNotificationsAsync(Toast)` | Returned the 7 toasts in the notification centre, with app names and full text |
| `NotificationChanged` (subscribe) | Fails: `0x80070490 Element not found` |
| One `GetNotificationsAsync` call | About 21 ms |

## Findings

1. **Without identity, Qapsule can read notifications but can't be told about new ones.** The only way to notice a new toast would be to check every few seconds. Every 3 seconds that means a 21 ms call around 20 times a minute, forever, in an app whose whole pitch is near-zero idle cost. It would also show toasts up to 3 seconds late.
2. **The unpackaged behaviour isn't dependable.** Others report the same calls returning `Allowed` and then an *empty* list on their machines ([blog.iany.me, Sep 2026](https://blog.iany.me/2026/09/forwarding-desktop-notifications-to-pushover/)). The event failure is an open Windows App SDK issue ([#6172](https://github.com/microsoft/WindowsAppSDK/issues/6172)), where a contributor confirms apps *with* identity get working events. Microsoft doesn't document unpackaged use, so it could change in any Windows update.
3. **With identity, it should work properly, and it fits the current installer.** The steps are in [Microsoft's guide](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/grant-identity-to-nonpackaged-apps):
   - an identity package manifest declaring `userNotificationListener`, with `AllowExternalContent`
   - an `msix` element in `qapsule-app.exe`'s application manifest that matches it
   - the installer registers it with `Add-AppxPackage -ExternalLocation <install folder>`
4. **Signing is the blocker.** The identity package must be signed by a certificate the user's machine trusts.
   - A self-signed certificate works only on your own machine, after importing it into `LocalMachine\TrustedPeople`, which needs one admin prompt.
   - For strangers' machines it needs a real code-signing certificate. SignPath Foundation provides one for open-source projects, once they're released; the Microsoft Store signs Store builds itself.

## Decision

**A launch feature, built properly with package identity.** Live notifications are part of the project's vision, so they get two routes to users instead of waiting for one (decided 23 Oct, see the roadmap):

- Don't build the timer-based version. It breaks the idle budget for a feature that can still silently show nothing on some machines.
- Build the event-driven version first on the development machine, with a self-signed identity package (18–30 Nov).
- For everyone else, either:
  - **SignPath:** apply right after v0.1 is released on 22 Dec, because SignPath requires a released project; the installer then registers a SignPath-signed identity package.
  - **Microsoft Store:** publish a Store build, which the Store signs (register the developer account early, as verification can be slow).
- If neither is ready by launch, launch without it and switch it on when one lands.

## Consequences

- The spec's "Notifications from other Windows apps" row says: launch, through a signed identity package or the Store build.
- When it's built, the setup guide must say:
  - Windows also keeps showing its own toast unless banners are turned off for that app.
  - Clicking opens the app, not the specific chat (known limits in the spec).
