# Calendar

Qapsule shows what is on now and what is next from any calendar that can give you a secret iCal link. Google Calendar is the example here.

## Connect a calendar

1. In Google Calendar, open Settings, pick your calendar, then Integrate calendar.
2. Copy the "Secret address in iCal format".
3. Run `qapsule calendar set-link`, paste the link and press Enter.

Only `https://` and `webcal://` links are accepted. The link is never printed back.

```sh
qapsule calendar status       # says whether a link is set
qapsule calendar clear-link   # removes it
```

Anyone with the link can read your calendar, so treat it like a password. If it leaks, reset it in Google Calendar.

## Where the link is kept

In Windows Credential Manager, under `Qapsule/calendar-ical`. It is never written to `config.toml`, command lines or logs, and Qapsule never logs event titles or locations either.

## When it refreshes

- When you open the main screen, if the data is older than 5 minutes.
- Every 5 minutes in the background, only if calendar pops are on. An event added at short notice pops as soon as Qapsule sees it, saying how many minutes are left.

A new link applies at the next refresh. If a refresh fails, the last events stay on screen marked as out of date.

## Pops

Pops follow the usual notification settings, so they are on unless you turn them off. To set them explicitly:

```toml
[notifications]
calendar = { pop = true, details = true, minutes_before = 10 }
```

- `pop`: show a pop "Starts in N min" before each event.
- `details`: show the event title and place. With `false`, the pop says "Upcoming event".
- `minutes_before`: from 1 to 120, default 10.

All-day events show on the main screen but never pop.

Changes to `[notifications] calendar` apply after restarting Qapsule.

Outlook feeds that use Windows time zone names show events in your local time.
