//! Occurrences to the expanded view's events card. Pure: the time and the zone
//! are arguments.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use qapsule_core::view::{EventNext, EventNow, EventsReady};
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

fn day_prefix(days_away: i64, date: NaiveDate) -> String {
    match days_away {
        0 => String::new(),
        1 => "Tomorrow".into(),
        2..=6 => date.format("%a").to_string(),
        _ => date.format("%a %-d %b").to_string(),
    }
}

/// `09:00`, `Tomorrow 09:00` or `Thu 07:30`.
pub fn when_timed(start: DateTime<Utc>, now: DateTime<Utc>, tz: Tz) -> String {
    let local = start.with_timezone(&tz);
    let today = now.with_timezone(&tz).date_naive();
    let prefix = day_prefix((local.date_naive() - today).num_days(), local.date_naive());
    let clock = local.format("%H:%M");
    if prefix.is_empty() {
        clock.to_string()
    } else {
        format!("{prefix} {clock}")
    }
}

/// `All day`, `Tomorrow · All day` or `Thu · All day`.
pub fn when_all_day(date: NaiveDate, today: NaiveDate) -> String {
    let prefix = day_prefix((date - today).num_days(), date);
    if prefix.is_empty() {
        "All day".into()
    } else {
        format!("{prefix} · All day")
    }
}

/// The card for `now`: the timed event in progress that began last, and the
/// next two to begin. All-day events are never "now".
pub fn build(occurrences: &[Occurrence], now: DateTime<Utc>, tz: Tz, stale: bool) -> EventsReady {
    let today = now.with_timezone(&tz).date_naive();

    let current = occurrences
        .iter()
        .filter_map(|o| match &o.span {
            Span::Timed { start, end } if *start <= now && now < *end => Some((o, *start, *end)),
            _ => None,
        })
        .max_by_key(|(_, start, _)| *start);

    // (local start used for ordering, occurrence, text for `when`)
    let mut upcoming: Vec<_> = occurrences
        .iter()
        .filter_map(|o| match &o.span {
            Span::Timed { start, .. } if *start > now => Some((
                start.with_timezone(&tz).naive_local(),
                o,
                when_timed(*start, now, tz),
            )),
            Span::AllDay { start, .. } if *start > today => Some((
                start.and_hms_opt(0, 0, 0).unwrap(),
                o,
                when_all_day(*start, today),
            )),
            _ => None,
        })
        .collect();
    upcoming.sort_by_key(|a| a.0);

    EventsReady {
        now: current.map(|(o, _, end)| EventNow {
            title: title_of(o),
            until: end.with_timezone(&tz).format("%H:%M").to_string(),
            location: location_of(o),
        }),
        next: upcoming
            .into_iter()
            .take(2)
            .map(|(_, o, when)| EventNext {
                title: title_of(o),
                when,
            })
            .collect(),
        stale,
    }
}

/// The next local midnight after `now`.
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

    #[test]
    fn the_google_export_makes_the_expected_card() {
        let text = include_str!("../../samples/calendar/google-export.ics");
        let occurrences = super::super::parse::parse_calendar(text, JOBURG, utc(NOW)).unwrap();
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let now = card.now.unwrap();
        assert_eq!(now.title, "Daily standup with a folded title");
        assert_eq!(now.until, "11:00");
        assert_eq!(
            card.next
                .iter()
                .map(|n| (n.title.as_str(), n.when.as_str()))
                .collect::<Vec<_>>(),
            vec![("Design review", "11:30"), ("Weekly sync (moved)", "15:00")]
        );
        assert!(!card.stale);
    }

    #[test]
    fn now_is_the_timed_event_that_started_most_recently() {
        let occurrences = [
            timed("Long", "2025-03-10 06:00", "2025-03-10 12:00"),
            timed("Short", "2025-03-10 07:30", "2025-03-10 08:30"),
            timed("Over", "2025-03-10 05:00", "2025-03-10 08:00"),
            all_day("Holiday", "2025-03-10"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(card.now.unwrap().title, "Short");
        assert!(card.next.is_empty());
    }

    #[test]
    fn next_labels_today_tomorrow_and_weekday_for_timed_and_all_day() {
        let occurrences = [
            timed("Today", "2025-03-10 09:00", "2025-03-10 10:00"),
            all_day("Tomorrow all day", "2025-03-11"),
            timed("Tomorrow", "2025-03-11 05:30", "2025-03-11 06:00"),
            timed("Later", "2025-03-13 05:30", "2025-03-13 06:00"),
            all_day("Friday", "2025-03-14"),
            all_day("Far", "2025-03-17"),
        ];
        let whens = |from: &str| -> Vec<String> {
            let now = utc(from);
            let mut o: Vec<Occurrence> = occurrences.to_vec();
            o.retain(|x| match &x.span {
                Span::Timed { start, .. } => *start > now,
                Span::AllDay { .. } => true,
            });
            build(&o, now, JOBURG, false)
                .next
                .into_iter()
                .map(|n| n.when)
                .collect()
        };
        assert_eq!(whens(NOW), vec!["11:00", "Tomorrow · All day"]);
        assert_eq!(
            whens("2025-03-10 09:30"),
            vec!["Tomorrow · All day", "Tomorrow 07:30"]
        );
        assert_eq!(
            when_timed(utc("2025-03-13 05:30"), utc(NOW), JOBURG),
            "Thu 07:30"
        );
        assert_eq!(
            when_all_day(
                NaiveDate::from_ymd_opt(2025, 3, 14).unwrap(),
                NaiveDate::from_ymd_opt(2025, 3, 10).unwrap()
            ),
            "Fri · All day"
        );
        assert_eq!(
            when_timed(utc("2025-03-17 05:30"), utc(NOW), JOBURG),
            "Mon 17 Mar 07:30"
        );
    }

    #[test]
    fn an_all_day_event_that_started_today_is_not_next() {
        let card = build(&[all_day("Today", "2025-03-10")], utc(NOW), JOBURG, true);
        assert!(card.now.is_none() && card.next.is_empty() && card.stale);
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
