//! Occurrences to the expanded view's events card. Pure: the time and the zone
//! are arguments.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use qapsule_core::view::{EventDay, EventHead, EventRow, EventsReady, MAX_WEEK_EVENTS};
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

/// Heights in px of what the card draws; they match the rules in `index.css`
/// (`.widget-events`, `.event-head`, `.event-row`, `.event-day-name`, `.event-now-line`,
/// `.event-more`). The card is 128 px tall and never scrolls.
const CARD: i32 = 128;
const ROW: i32 = 20;
const DAY_CAPTION: i32 = 16;
const NOW_LINE: i32 = 9;
const MORE: i32 = 16;
const STALE: i32 = 14;
const HEAD: i32 = 47;
const HEAD_LOCATION: i32 = 18;

fn day_label(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        _ => date.format("%a %-d %b").to_string(),
    }
}

/// "In 18 min", "In 2 h" or "In 2 h 5 min", rounded up to the minute.
fn in_text(until: Duration) -> String {
    let minutes = (until.num_seconds() + 59) / 60;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("In {m} min"),
        (h, 0) => format!("In {h} h"),
        (h, m) => format!("In {h} h {m} min"),
    }
}

struct Entry {
    date: NaiveDate,
    timed: bool,
    order: NaiveDateTime,
    row: EventRow,
}

fn row(title: String, time: String, past: bool) -> EventRow {
    EventRow {
        title,
        time,
        color: None,
        past,
    }
}

/// The card for `now`: the timed event in progress that began last (or the next
/// one today), then the rest of the week grouped by day and cut to what fits the
/// card (see the constants above). All-day events are never "now".
pub fn build(occurrences: &[Occurrence], now: DateTime<Utc>, tz: Tz, stale: bool) -> EventsReady {
    let today = now.with_timezone(&tz).date_naive();
    let limit = today + Duration::days(WEEK_DAYS);

    let current = occurrences
        .iter()
        .enumerate()
        .filter_map(|(i, o)| match &o.span {
            Span::Timed { start, end } if *start <= now && now < *end => Some((i, *start, *end)),
            _ => None,
        })
        .max_by_key(|(_, start, _)| *start);

    let next_today = || {
        occurrences
            .iter()
            .enumerate()
            .filter_map(|(i, o)| match &o.span {
                Span::Timed { start, .. }
                    if *start > now && start.with_timezone(&tz).date_naive() == today =>
                {
                    Some((i, *start))
                }
                _ => None,
            })
            .min_by_key(|(i, start)| (*start, title_of(&occurrences[*i])))
    };

    let (head_index, caption) = match current {
        Some((i, _, end)) => (
            Some(i),
            format!("Now · until {}", end.with_timezone(&tz).format("%H:%M")),
        ),
        None => match next_today() {
            Some((i, start)) => (Some(i), in_text(start - now)),
            None => (None, String::new()),
        },
    };
    let head = head_index.map(|i| EventHead {
        caption,
        title: title_of(&occurrences[i]),
        location: location_of(&occurrences[i]),
    });

    let mut upcoming: Vec<Entry> = Vec::new();
    let mut past: Vec<Entry> = Vec::new();
    for (i, o) in occurrences.iter().enumerate() {
        if Some(i) == head_index {
            continue;
        }
        match &o.span {
            Span::Timed { start, end } => {
                let local = start.with_timezone(&tz);
                let entry = |past| Entry {
                    date: local.date_naive(),
                    timed: true,
                    order: local.naive_local(),
                    row: row(title_of(o), local.format("%H:%M").to_string(), past),
                };
                if *start > now {
                    if local.date_naive() < limit {
                        upcoming.push(entry(false));
                    }
                } else if *end <= now && local.date_naive() == today {
                    past.push(entry(true));
                }
            }
            Span::AllDay { start, end } if *end > today && *start < limit => {
                let date = (*start).max(today);
                upcoming.push(Entry {
                    date,
                    timed: false,
                    order: date.and_hms_opt(0, 0, 0).unwrap(),
                    row: row(title_of(o), "All day".into(), false),
                });
            }
            _ => {}
        }
    }
    let key = |e: &Entry| (e.date, e.timed, e.order, e.row.title.clone());
    upcoming.sort_by_key(key);
    upcoming.truncate(MAX_WEEK_EVENTS);
    past.sort_by_key(key);

    let mut groups: Vec<(NaiveDate, Vec<EventRow>)> = Vec::new();
    for e in upcoming {
        match groups.last_mut() {
            Some((date, rows)) if *date == e.date => rows.push(e.row),
            _ => groups.push((e.date, vec![e.row])),
        }
    }
    let total: usize = groups.iter().map(|g| g.1.len()).sum();

    // Today needs no caption when the head already says it is today.
    let caption_cost = |date: NaiveDate| {
        if date == today && head.is_some() {
            0
        } else {
            DAY_CAPTION
        }
    };
    let mut avail = CARD - if stale { STALE } else { 0 };
    if let Some(h) = &head {
        avail -= HEAD
            + if h.location.is_some() {
                HEAD_LOCATION
            } else {
                0
            };
    }
    let fit = |budget: i32| -> Vec<usize> {
        let mut left = budget;
        let mut shown = vec![0; groups.len()];
        'days: for (i, (date, rows)) in groups.iter().enumerate() {
            let cost = caption_cost(*date);
            if left < cost + ROW {
                break;
            }
            left -= cost;
            for _ in rows {
                if left < ROW {
                    break 'days;
                }
                left -= ROW;
                shown[i] += 1;
            }
        }
        shown
    };
    let mut shown = fit(avail);
    if shown.iter().sum::<usize>() < total {
        shown = fit(avail - MORE);
    }
    let hidden = total - shown.iter().sum::<usize>();
    let hidden_today = groups
        .first()
        .filter(|g| g.0 == today)
        .map_or(0, |g| g.1.len() - shown[0]);
    let more = if hidden_today > 0 {
        Some(format!("+{hidden_today} more today"))
    } else if hidden > 0 {
        Some(format!("+{hidden} more"))
    } else {
        None
    };

    // Finished events take whatever room is left, most recent first, but only when
    // no upcoming row was cut.
    let mut used: i32 = groups
        .iter()
        .zip(&shown)
        .filter(|(_, n)| **n > 0)
        .map(|((date, _), n)| caption_cost(*date) + *n as i32 * ROW)
        .sum();
    if hidden > 0 {
        used += MORE;
    }
    let has_today = groups.first().is_some_and(|g| g.0 == today && shown[0] > 0);
    let extra = NOW_LINE + if has_today { 0 } else { caption_cost(today) };
    let room = ((avail - used - extra).max(0) / ROW) as usize;
    let keep = if hidden == 0 { past.len().min(room) } else { 0 };
    let past_rows: Vec<EventRow> = past
        .split_off(past.len() - keep)
        .into_iter()
        .map(|e| e.row)
        .collect();

    let mut days: Vec<EventDay> = Vec::new();
    let mut past_rows = Some(past_rows).filter(|p| !p.is_empty());
    if !has_today && let Some(p) = past_rows.take() {
        days.push(EventDay {
            label: (caption_cost(today) > 0).then(|| day_label(today, today)),
            now_line: Some(p.len()),
            rows: p,
        });
    }
    for ((date, rows), n) in groups.into_iter().zip(shown) {
        if n == 0 {
            break;
        }
        let mut rows: Vec<EventRow> = rows.into_iter().take(n).collect();
        let mut now_line = None;
        if date == today
            && let Some(p) = past_rows.take()
        {
            now_line = Some(p.len());
            rows.splice(0..0, p);
        }
        days.push(EventDay {
            label: (caption_cost(date) > 0).then(|| day_label(date, today)),
            now_line,
            rows,
        });
    }

    EventsReady {
        head,
        days,
        more,
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

    /// (day label or "", time, title) for every row.
    fn rows(card: &EventsReady) -> Vec<(String, &str, &str)> {
        card.days
            .iter()
            .flat_map(|d| {
                d.rows.iter().map(move |r| {
                    (
                        d.label.clone().unwrap_or_default(),
                        r.time.as_str(),
                        r.title.as_str(),
                    )
                })
            })
            .collect()
    }

    fn titles(card: &EventsReady) -> Vec<&str> {
        card.days
            .iter()
            .flat_map(|d| d.rows.iter().map(|r| r.title.as_str()))
            .collect()
    }

    fn many(prefix: &str, date: &str, n: usize) -> Vec<Occurrence> {
        (0..n)
            .map(|i| {
                timed(
                    &format!("{prefix}{i}"),
                    &format!("{date} {:02}:00", 6 + i),
                    &format!("{date} {:02}:30", 6 + i),
                )
            })
            .collect()
    }

    #[test]
    fn the_google_export_makes_the_expected_card() {
        let text = include_str!("../../samples/calendar/google-export.ics");
        let occurrences = super::super::parse::parse_calendar(text, JOBURG, utc(NOW)).unwrap();
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let head = card.head.clone().unwrap();
        assert_eq!(head.title, "Daily standup with a folded title");
        assert_eq!(head.caption, "Now · until 11:00");
        assert_eq!(
            rows(&card)
                .into_iter()
                .filter(|r| r.0.is_empty())
                .map(|r| (r.1, r.2))
                .collect::<Vec<_>>(),
            vec![("11:30", "Design review"), ("15:00", "Weekly sync (moved)")]
        );
        assert!(!card.stale);
    }

    #[test]
    fn now_is_the_timed_event_that_started_most_recently() {
        let mut short = timed("Short", "2025-03-10 07:30", "2025-03-10 08:30");
        short.location = Some("Room 4".into());
        let occurrences = [
            timed("Long", "2025-03-10 06:00", "2025-03-10 12:00"),
            short,
            timed("Over", "2025-03-10 05:00", "2025-03-10 08:00"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let head = card.head.clone().unwrap();
        assert_eq!(head.caption, "Now · until 10:30");
        assert_eq!(head.title, "Short");
        assert_eq!(head.location.as_deref(), Some("Room 4"));
        // "Over" ended exactly now: finished, shown dimmed above the now line.
        assert_eq!(titles(&card), ["Over"]);
        assert!(card.days[0].rows[0].past);
    }

    #[test]
    fn the_next_event_today_heads_the_card_when_nothing_is_current() {
        let occurrences = [
            timed("Later", "2025-03-10 09:30", "2025-03-10 10:00"),
            timed("Soon", "2025-03-10 08:18", "2025-03-10 09:00"),
            timed("Tomorrow", "2025-03-11 06:00", "2025-03-11 07:00"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let head = card.head.clone().unwrap();
        assert_eq!(
            (head.caption.as_str(), head.title.as_str()),
            ("In 18 min", "Soon")
        );
        assert_eq!(
            rows(&card),
            vec![
                (String::new(), "11:30", "Later"),
                ("Tomorrow".to_string(), "08:00", "Tomorrow"),
            ]
        );
        // Nothing left today: no head, and the first day is captioned.
        let card = build(&occurrences[2..], utc(NOW), JOBURG, false);
        assert!(card.head.is_none());
        assert_eq!(rows(&card)[0].0, "Tomorrow");
        assert_eq!(in_text(Duration::minutes(125)), "In 2 h 5 min");
        assert_eq!(in_text(Duration::minutes(120)), "In 2 h");
        assert_eq!(in_text(Duration::seconds(61)), "In 2 min");
    }

    #[test]
    fn days_group_across_midnight_in_the_users_zone() {
        // 21:30 UTC is 23:30 in Johannesburg; 22:30 UTC is 00:30 the next day.
        let occurrences = [
            timed("After midnight", "2025-03-11 22:30", "2025-03-11 23:00"),
            timed("Late", "2025-03-11 21:30", "2025-03-11 21:45"),
            timed("Early", "2025-03-11 05:30", "2025-03-11 06:00"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert!(card.head.is_none());
        assert_eq!(
            rows(&card),
            vec![
                ("Tomorrow".to_string(), "07:30", "Early"),
                ("Tomorrow".to_string(), "23:30", "Late"),
                ("Wed 12 Mar".to_string(), "00:30", "After midnight"),
            ]
        );
        assert_eq!(card.days.len(), 2);
    }

    #[test]
    fn finished_events_sit_above_the_now_line() {
        let occurrences = [
            timed("Done", "2025-03-10 05:00", "2025-03-10 06:00"),
            timed("Head", "2025-03-10 08:10", "2025-03-10 08:50"),
            timed("Next", "2025-03-10 09:00", "2025-03-10 10:00"),
            timed("Yesterday", "2025-03-09 05:00", "2025-03-09 06:00"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(card.days.len(), 1);
        let today = &card.days[0];
        assert_eq!(today.label, None);
        assert_eq!(today.now_line, Some(1));
        assert_eq!(
            today
                .rows
                .iter()
                .map(|r| (r.title.as_str(), r.past))
                .collect::<Vec<_>>(),
            vec![("Done", true), ("Next", false)]
        );
        assert!(today.rows.iter().all(|r| r.color.is_none()));
        // With nothing upcoming today, the finished row still gets a day and a line.
        let card = build(&occurrences[..1], utc(NOW), JOBURG, false);
        assert!(card.head.is_none());
        assert_eq!(card.days[0].label.as_deref(), Some("Today"));
        assert_eq!(card.days[0].now_line, Some(1));
    }

    #[test]
    fn rows_that_do_not_fit_become_a_more_line() {
        // Head takes 47 px of 128; 81 left, 65 once the "+N more" line is reserved: 3 rows.
        let mut occurrences = many("E", "2025-03-10", 8);
        occurrences.iter_mut().for_each(|o| {
            if let Span::Timed { start, end } = &mut o.span {
                *start += Duration::hours(3);
                *end += Duration::hours(3);
            }
        });
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        assert_eq!(card.head.as_ref().unwrap().title, "E0");
        assert_eq!(titles(&card), ["E1", "E2", "E3"]);
        assert_eq!(card.more.as_deref(), Some("+4 more today"));
        // Four rows fit exactly when nothing is cut.
        let card = build(&occurrences[..5], utc(NOW), JOBURG, false);
        assert_eq!(titles(&card).len(), 4);
        assert_eq!(card.more, None);
        // Stale takes 14 px: one row fewer.
        let card = build(&occurrences[..5], utc(NOW), JOBURG, true);
        assert_eq!(titles(&card).len(), 2);
        assert_eq!(card.more.as_deref(), Some("+2 more today"));

        // Beyond today it is "+N more".
        let mut later = many("T", "2025-03-12", 6);
        later.push(timed("Now", "2025-03-10 07:00", "2025-03-10 09:00"));
        let card = build(&later, utc(NOW), JOBURG, false);
        assert_eq!(card.days[0].label.as_deref(), Some("Wed 12 Mar"));
        assert_eq!(titles(&card).len(), 2);
        assert_eq!(card.more.as_deref(), Some("+4 more"));
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
            all_day("Holiday", "2025-03-10"),
            multi,
            past,
            all_day("Long", "2025-03-10"),
        ];
        let card = build(&occurrences, utc(NOW), JOBURG, true);
        assert!(card.head.is_none() && card.stale);
        assert_eq!(
            rows(&card),
            vec![
                ("Today".to_string(), "All day", "Holiday"),
                ("Today".to_string(), "All day", "Long"),
                ("Today".to_string(), "All day", "Trip"),
            ]
        );
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
                ("Sun 16 Mar".to_string(), "All day", "In all day"),
                ("Sun 16 Mar".to_string(), "23:00", "In"),
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
        ];
        assert_eq!(
            titles(&build(&occurrences, utc(NOW), JOBURG, false)),
            ["Day", "Early", "A", "B"]
        );
    }

    #[test]
    fn the_input_is_capped() {
        let occurrences: Vec<_> = (0..MAX_WEEK_EVENTS + 20)
            .map(|i| timed(&format!("E{i}"), "2025-03-12 06:00", "2025-03-12 07:00"))
            .collect();
        let card = build(&occurrences, utc(NOW), JOBURG, false);
        let shown: usize = card.days.iter().map(|d| d.rows.len()).sum();
        let more: usize = card.more.unwrap()[1..]
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(shown + more, MAX_WEEK_EVENTS);
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
