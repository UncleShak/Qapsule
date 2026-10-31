//! Occurrences to the expanded view's events card. Pure: the time and the zone
//! are arguments.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use qapsule_core::view::{EventNext, EventNow, EventsReady, MAX_WEEK_EVENTS};
use rrule::Tz;

use super::parse::{Occurrence, Span};

pub const MAX_TITLE: usize = 80;
pub const MAX_LOCATION: usize = 120;

/// Control characters become spaces (line breaks) or vanish; the text is
/// trimmed and cut to `max` characters, ending with an ellipsis when cut.
pub fn clean(text: &str, max: usize) -> String {
    let spaced: String = text
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    let trimmed = spaced.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// The title to show; untitled events get a plain name.
pub fn title_of(occurrence: &Occurrence) -> String {
    let title = clean(&occurrence.title, MAX_TITLE);
    if title.is_empty() {
        "Untitled event".into()
    } else {
        title
    }
}

pub fn location_of(occurrence: &Occurrence) -> Option<String> {
    occurrence
        .location
        .as_deref()
        .map(|l| clean(l, MAX_LOCATION))
        .filter(|l| !l.is_empty())
}

/// How many days the card covers, today included.
const WEEK_DAYS: i64 = 7;

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        _ => date.format("%a %-d %b").to_string(),
    }
}

/// The card for `now`: the timed event in progress that began last, and every
/// event from now until the end of the sixth day after today, grouped by day.
/// All-day events are never "now".
pub fn build(occurrences: &[Occurrence], now: DateTime<Utc>, tz: Tz, stale: bool) -> EventsReady {
    let today = now.with_timezone(&tz).date_naive();
    let limit = today + Duration::days(WEEK_DAYS);

    let current = occurrences
        .iter()
        .filter_map(|o| match &o.span {
            Span::Timed { start, end } if *start <= now && now < *end => Some((o, *start, *end)),
            _ => None,
        })
        .max_by_key(|(_, start, _)| *start);

    // (label date, timed?, start for ordering, title, time text)
    let mut upcoming: Vec<_> = occurrences
        .iter()
        .filter_map(|o| match &o.span {
            Span::Timed { start, .. } if *start > now => {
                let local = start.with_timezone(&tz);
                (local.date_naive() < limit).then(|| {
                    (
                        local.date_naive(),
                        true,
                        local.naive_local(),
                        title_of(o),
                        local.format("%H:%M").to_string(),
                    )
                })
            }
            Span::AllDay { start, end } if *end > today && *start < limit => {
                let label = (*start).max(today);
                Some((
                    label,
                    false,
                    label.and_hms_opt(0, 0, 0).unwrap(),
                    title_of(o),
                    "All day".to_string(),
                ))
            }
            _ => None,
        })
        .collect();
    upcoming.sort_by(|a, b| (a.0, a.1, a.2, &a.3).cmp(&(b.0, b.1, b.2, &b.3)));

    EventsReady {
        now: current.map(|(o, _, end)| EventNow {
            title: title_of(o),
            until: end.with_timezone(&tz).format("%H:%M").to_string(),
            location: location_of(o),
        }),
        next: upcoming
            .into_iter()
            .take(MAX_WEEK_EVENTS)
            .map(|(date, _, _, title, time)| EventNext {
                title,
                day: day_label(date, today),
                time,
            })
            .collect(),
        stale,
    }
}

/// The next local midnight after `now`.
// The 1 h fallback covers days with no local midnight (DST at 00:00); it is accepted.
pub fn next_midnight(now: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    let tomorrow = now.with_timezone(&tz).date_naive() + Duration::days(1);
    tomorrow
        .and_hms_opt(0, 0, 0)
        .and_then(|midnight| tz.from_local_datetime(&midnight).earliest())
        .map_or(now + Duration::hours(1), |t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDateTime;

    const JOBURG: Tz = Tz::Africa__Johannesburg;

    fn utc(s: &str) -> DateTime<Utc> {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
            .unwrap()
            .and_utc()
    }

    fn timed(title: &str, start: &str, end: &str) -> Occurrence {
        Occurrence {
            uid: title.into(),
            title: title.into(),
            location: None,
            span: Span::Timed {
                start: utc(start),
                end: utc(end),
            },
        }
    }

    fn all_day(title: &str, date: &str) -> Occurrence {
        let start = NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap();
        Occurrence {
            uid: title.into(),
            title: title.into(),
            location: None,
            span: Span::AllDay {
                start,
                end: start + Duration::days(1),
            },
        }
    }

    // Monday 2025-03-10, 10:00 in Johannesburg.
    const NOW: &str = "2025-03-10 08:00";

    fn rows(card: &EventsReady) -> Vec<(&str, &str, &str)> {
        card.next
            .iter()
            .map(|n| (n.day.as_str(), n.time.as_str(), n.title.as_str()))
            .collect()
    }

    #[test]
    fn the_google_export_makes_the_expected_card() {
        let text = include_str!("../../samples/calendar/google-export.ics");
        let occurrences = super::super::parse::parse_calendar(text, JOBURG, utc(NOW)).unwrap();
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let now = card.now.clone().unwrap();
        assert_eq!(now.title, "Daily standup with a folded title");
        assert_eq!(now.until, "11:00");
        let today: Vec<_> = card.next.iter().filter(|n| n.day == "Today").collect();
        assert_eq!(
            today
                .iter()
                .map(|n| (n.time.as_str(), n.title.as_str()))
                .take(2)
                .collect::<Vec<_>>(),
            vec![("11:30", "Design review"), ("15:00", "Weekly sync (moved)")]
        );
        assert!(!card.stale);
    }

    #[test]
    fn now_is_the_timed_event_that_started_most_recently() {
        let occurrences = [
            timed("Long", "2025-03-10 06:00", "2025-03-10 12:00"),
            timed("Short", "2025-03-10 07:30", "2025-03-10 08:30"),
            timed("Over", "2025-03-10 05:00", "2025-03-10 08:00"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(card.now.unwrap().title, "Short");
        assert!(card.next.is_empty());
    }

    #[test]
    fn days_are_labelled_today_tomorrow_and_by_date() {
        let occurrences = [
            timed("Later", "2025-03-13 05:30", "2025-03-13 06:00"),
            timed("Tomorrow", "2025-03-11 05:30", "2025-03-11 06:00"),
            timed("Today", "2025-03-10 09:00", "2025-03-10 10:00"),
        ];
        assert_eq!(
            rows(&build(&occurrences, utc(NOW), JOBURG, false)),
            vec![
                ("Today", "11:00", "Today"),
                ("Tomorrow", "07:30", "Tomorrow"),
                ("Thu 13 Mar", "07:30", "Later"),
            ]
        );
        let card = build(
            &[timed("M", "2026-11-02 05:00", "2026-11-02 06:00")],
            utc("2026-10-30 08:00"),
            JOBURG,
            false,
        );
        assert_eq!(card.next[0].day, "Mon 2 Nov");
    }

    #[test]
    fn the_week_ends_at_local_midnight_seven_days_on() {
        // Today is Mon 10 Mar; the window ends at 00:00 on Mon 17 Mar local (22:00 UTC on the 16th).
        let occurrences = [
            timed("In", "2025-03-16 21:00", "2025-03-16 21:30"),
            timed("Out", "2025-03-16 22:00", "2025-03-16 22:30"),
            all_day("Out all day", "2025-03-17"),
            all_day("In all day", "2025-03-16"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(
            rows(&card),
            vec![
                ("Sun 16 Mar", "All day", "In all day"),
                ("Sun 16 Mar", "23:00", "In"),
            ]
        );
    }

    #[test]
    fn all_day_events_running_today_sit_under_today_and_are_never_now() {
        let mut multi = all_day("Trip", "2025-03-09");
        multi.span = Span::AllDay {
            start: NaiveDate::from_ymd_opt(2025, 3, 9).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 3, 12).unwrap(),
        };
        let past = all_day("Past", "2025-03-09");
        let occurrences = [
            timed("Call", "2025-03-10 09:00", "2025-03-10 10:00"),
            all_day("Holiday", "2025-03-10"),
            multi,
            past,
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, true);
        assert!(card.now.is_none() && card.stale);
        assert_eq!(
            rows(&card),
            vec![
                ("Today", "All day", "Holiday"),
                ("Today", "All day", "Trip"),
                ("Today", "11:00", "Call"),
            ]
        );
    }

    #[test]
    fn events_are_ordered_by_day_then_all_day_then_time_then_title() {
        let occurrences = [
            timed("B", "2025-03-11 06:00", "2025-03-11 07:00"),
            timed("A", "2025-03-11 06:00", "2025-03-11 07:00"),
            timed("Early", "2025-03-11 05:00", "2025-03-11 07:00"),
            all_day("Day", "2025-03-11"),
            timed("Today", "2025-03-10 09:00", "2025-03-10 10:00"),
        ];
        let titles: Vec<_> = build(&occurrences, utc(NOW), JOBURG, false)
            .next
            .into_iter()
            .map(|n| n.title)
            .collect();
        assert_eq!(titles, ["Today", "Day", "Early", "A", "B"]);
    }

    #[test]
    fn the_list_is_capped() {
        let occurrences: Vec<_> = (0..MAX_WEEK_EVENTS + 20)
            .map(|i| timed(&format!("E{i}"), "2025-03-12 06:00", "2025-03-12 07:00"))
            .collect();
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(card.next.len(), MAX_WEEK_EVENTS);
    }

    #[test]
    fn text_is_cleaned_and_cut() {
        assert_eq!(clean("a\nb\u{7}c\t d ", 80), "a bc  d");
        let long = "é".repeat(90);
        let cut = clean(&long, MAX_TITLE);
        assert_eq!(cut.chars().count(), MAX_TITLE);
        assert!(cut.ends_with('…'));
        let mut o = timed("", "2025-03-10 09:00", "2025-03-10 10:00");
        assert_eq!(title_of(&o), "Untitled event");
        o.location = Some("x".repeat(200));
        assert_eq!(location_of(&o).unwrap().chars().count(), MAX_LOCATION);
    }

    #[test]
    fn midnight_is_local() {
        assert_eq!(next_midnight(utc(NOW), JOBURG), utc("2025-03-10 22:00"));
        assert_eq!(
            next_midnight(utc("2025-03-10 22:00"), JOBURG),
            utc("2025-03-11 22:00")
        );
    }
}
