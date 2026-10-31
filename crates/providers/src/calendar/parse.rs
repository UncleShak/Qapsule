//! iCal text to the occurrences inside a window around now. Pure: no network,
//! no clock. Only the properties Qapsule shows are read.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use rrule::{Frequency, RRule, RRuleSet, Tz, Unvalidated};

/// Occurrences starting this long before now are kept (for events still running).
const WINDOW_BEFORE_DAYS: i64 = 1;
/// Occurrences starting up to this long after now are kept.
const WINDOW_AFTER_DAYS: i64 = 8;
/// At most this many occurrences are kept.
pub const MAX_OCCURRENCES: usize = 2000;
/// The most occurrences expanded from one recurring event.
const MAX_PER_SERIES: usize = 2000;
/// The most occurrences expanded across the whole feed.
const MAX_EXPANDED: usize = 10_000;
/// The most events read from one feed.
const MAX_EVENTS: usize = 5000;
/// The most dates a rule may generate while searching for the window.
const MAX_ITERATIONS: usize = 100_000;
/// Logged when a limit cuts a feed short (fixed text only).
const TOO_LARGE: &str = "qapsule: calendar too large; showing part of it";

/// The text was not an iCal calendar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub &'static str);

/// When an occurrence happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// Whole days: `end` is the day after the last one.
    AllDay { start: NaiveDate, end: NaiveDate },
}

/// One event on one day (or several), recurring events already expanded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub uid: String,
    pub title: String,
    pub location: Option<String>,
    pub span: Span,
}

impl Occurrence {
    /// An instant to sort by; all-day events sort at midnight UTC of their day.
    fn sort_key(&self) -> DateTime<Utc> {
        match &self.span {
            Span::Timed { start, .. } => *start,
            Span::AllDay { start, .. } => start.and_hms_opt(0, 0, 0).unwrap().and_utc(),
        }
    }
}

// --- Lines ---------------------------------------------------------------------------------

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Joins continuation lines (starting with a space or tab) to the line before.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match lines.last_mut() {
            Some(last) if raw.starts_with([' ', '\t']) => last.push_str(&raw[1..]),
            _ => lines.push(raw.to_string()),
        }
    }
    lines
}

/// Splits on `sep` outside double quotes.
fn split_unquoted(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut quoted, mut from) = (false, 0);
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            parts.push(&text[from..i]);
            from = i + 1;
        }
    }
    parts.push(&text[from..]);
    parts
}

fn parse_line(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let colon = line.char_indices().find_map(|(i, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        (c == ':' && !quoted).then_some(i)
    })?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = split_unquoted(head, ';').into_iter();
    let name = parts.next()?.trim().to_ascii_uppercase();
    if name.is_empty() {
        return None;
    }
    let params = parts
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                k.trim().to_ascii_uppercase(),
                v.trim().trim_matches('"').into(),
            ))
        })
        .collect();
    Some(Prop {
        name,
        params,
        value: value.to_string(),
    })
}

/// Undoes iCal text escaping: `\n`, `\,`, `\;` and `\\`.
pub fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

// --- Times ---------------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Moment {
    Date(NaiveDate),
    Time(DateTime<Tz>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Date(NaiveDate),
    Instant(DateTime<Utc>),
}

impl Moment {
    fn key(&self) -> Key {
        match self {
            Moment::Date(d) => Key::Date(*d),
            Moment::Time(t) => Key::Instant(t.with_timezone(&Utc)),
        }
    }
}

fn local_time(zone: Tz, naive: NaiveDateTime) -> Option<DateTime<Tz>> {
    zone.from_local_datetime(&naive)
        .earliest()
        // In a spring-forward gap: the same wall time an hour later.
        .or_else(|| {
            zone.from_local_datetime(&(naive + Duration::hours(1)))
                .earliest()
        })
}

/// One date or date-time value. An unknown `TZID` is read as local time.
fn moment_of(value: &str, prop: &Prop, local: Tz) -> Option<Moment> {
    let value = value.trim();
    if prop.param("VALUE") == Some("DATE") || (value.len() == 8 && !value.contains('T')) {
        return NaiveDate::parse_from_str(value, "%Y%m%d")
            .ok()
            .map(Moment::Date);
    }
    let (digits, utc) = match value.strip_suffix(['Z', 'z']) {
        Some(rest) => (rest, true),
        None => (value, false),
    };
    let naive = NaiveDateTime::parse_from_str(digits, "%Y%m%dT%H%M%S").ok()?;
    let time = if utc {
        Tz::UTC.from_utc_datetime(&naive)
    } else {
        let zone = prop
            .param("TZID")
            .and_then(|id| id.parse::<chrono_tz::Tz>().ok())
            .map_or(local, Tz::Tz);
        local_time(zone, naive)?
    };
    Some(Moment::Time(time))
}

fn moments_of(prop: &Prop, local: Tz) -> Vec<Moment> {
    prop.value
        .split(',')
        .filter_map(|v| moment_of(v, prop, local))
        .collect()
}

/// `P1DT2H30M`, `PT15M`, `P1W`, optionally negative.
fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let rest = text.strip_prefix('P')?;
    let (mut total, mut digits, mut in_time) = (Duration::zero(), String::new(), false);
    for c in rest.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        if c == 'T' {
            in_time = true;
            continue;
        }
        let n: i64 = digits.parse().ok()?;
        digits.clear();
        total += match (c, in_time) {
            ('W', false) => Duration::try_weeks(n)?,
            ('D', false) => Duration::try_days(n)?,
            ('H', true) => Duration::try_hours(n)?,
            ('M', true) => Duration::try_minutes(n)?,
            ('S', true) => Duration::try_seconds(n)?,
            _ => return None,
        };
    }
    digits
        .is_empty()
        .then_some(if negative { -total } else { total })
}

// --- Events --------------------------------------------------------------------------------

#[derive(Default)]
struct RawEvent {
    uid: Option<String>,
    summary: Option<String>,
    location: Option<String>,
    dtstart: Option<Prop>,
    dtend: Option<Prop>,
    duration: Option<String>,
    cancelled: bool,
    rrule: Option<String>,
    exdates: Vec<Prop>,
    recurrence_id: Option<Prop>,
}

/// The events of the feed (at most [`MAX_EVENTS`]) and whether some were left out.
fn raw_events(text: &str) -> (Vec<RawEvent>, bool) {
    let mut events = Vec::new();
    let mut current: Option<RawEvent> = None;
    let mut nested = 0usize;
    for line in unfold(text) {
        let Some(prop) = parse_line(&line) else {
            continue;
        };
        match (prop.name.as_str(), &mut current) {
            ("BEGIN", None) if prop.value.eq_ignore_ascii_case("VEVENT") => {
                if events.len() >= MAX_EVENTS {
                    return (events, true);
                }
                current = Some(RawEvent::default());
            }
            ("BEGIN", Some(_)) => nested += 1,
            ("END", Some(_)) if nested > 0 => nested -= 1,
            ("END", Some(_)) if prop.value.eq_ignore_ascii_case("VEVENT") => {
                events.extend(current.take());
            }
            (_, Some(event)) if nested == 0 => match prop.name.as_str() {
                "UID" => event.uid = Some(prop.value.trim().to_string()),
                "SUMMARY" => event.summary = Some(unescape(&prop.value)),
                "LOCATION" => event.location = Some(unescape(&prop.value)),
                "DTSTART" => event.dtstart = Some(prop),
                "DTEND" => event.dtend = Some(prop),
                "DURATION" => event.duration = Some(prop.value),
                "STATUS" => event.cancelled = prop.value.trim().eq_ignore_ascii_case("CANCELLED"),
                "RRULE" => event.rrule = Some(prop.value),
                "EXDATE" => event.exdates.push(prop),
                "RECURRENCE-ID" => event.recurrence_id = Some(prop),
                _ => {}
            },
            _ => {}
        }
    }
    (events, false)
}

/// How long one occurrence of the event lasts.
fn length(event: &RawEvent, start: &Moment, local: Tz) -> Duration {
    let ended = event
        .dtend
        .as_ref()
        .and_then(|p| moments_of(p, local).into_iter().next());
    let given = event.duration.as_deref().and_then(parse_duration);
    match start {
        Moment::Time(s) => match ended {
            Some(Moment::Time(e)) => {
                (e.with_timezone(&Utc) - s.with_timezone(&Utc)).max(Duration::zero())
            }
            _ => given.unwrap_or_else(Duration::zero).max(Duration::zero()),
        },
        Moment::Date(s) => {
            let days = match (ended, given) {
                (Some(Moment::Date(e)), _) => (e - *s).num_days(),
                (_, Some(d)) => d.num_days(),
                _ => 1,
            };
            Duration::days(days.max(1))
        }
    }
}

/// The rule with an `UNTIL` that has no zone (a date, as Google exports, or a
/// floating time) given in UTC, read in `zone`; `rrule` refuses anything else
/// once the start has a zone. A date means through the end of that day.
fn utc_until(rule: &str, zone: Tz) -> String {
    rule.split(';')
        .map(|part| {
            let Some((key, value)) = part.split_once('=') else {
                return part.to_string();
            };
            if !key.trim().eq_ignore_ascii_case("UNTIL") || value.ends_with(['Z', 'z']) {
                return part.to_string();
            }
            let naive = NaiveDate::parse_from_str(value, "%Y%m%d")
                .ok()
                .and_then(|d| d.and_hms_opt(23, 59, 59))
                .or_else(|| NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S").ok());
            match naive.and_then(|n| local_time(zone, n)) {
                Some(t) => format!("UNTIL={}", t.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ")),
                None => part.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The starts of a recurring event around the window, at most `room` of them,
/// and whether that limit (or the search limit) cut the series short. `None` if
/// the rule is unusable.
fn expand(
    start: &Moment,
    rule: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    length: Duration,
    room: usize,
) -> Option<(Vec<Moment>, bool)> {
    let dt_start = match start {
        Moment::Time(t) => *t,
        // All-day series run in UTC so daylight saving can't shift the date.
        Moment::Date(d) => Tz::UTC.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?),
    };
    let rule: RRule<Unvalidated> = utc_until(rule.trim(), dt_start.timezone()).parse().ok()?;
    let rule = rule.validate(dt_start).ok()?;
    // Every second or minute would only fill the list; show the first one.
    if matches!(rule.get_freq(), Frequency::Secondly | Frequency::Minutely) {
        return Some((vec![start.clone()], false));
    }
    let first = (from - length - Duration::days(1)).with_timezone(&Tz::UTC);
    let last = (to + Duration::days(1)).with_timezone(&Tz::UTC);
    let limit = MAX_PER_SERIES.min(room);
    let set = RRuleSet::new(dt_start).rrule(rule);
    let mut starts = Vec::new();
    let mut cut = false;
    for date in set.into_iter().take(MAX_ITERATIONS) {
        if date > last {
            break;
        }
        if date < first {
            continue;
        }
        if starts.len() >= limit {
            cut = true;
            break;
        }
        starts.push(match start {
            Moment::Date(_) => Moment::Date(date.date_naive()),
            Moment::Time(_) => Moment::Time(date),
        });
    }
    Some((starts, cut))
}

/// The occurrences of every event in the window `[now - 1 day, now + 8 days]`,
/// sorted by start and capped at [`MAX_OCCURRENCES`]. `local` is the zone for
/// times with no zone of their own and for deciding which day an all-day event is on.
pub fn parse_calendar(
    text: &str,
    local: Tz,
    now: DateTime<Utc>,
) -> Result<Vec<Occurrence>, ParseError> {
    if !text
        .get(..text.len().min(4096))
        .unwrap_or(text)
        .to_ascii_uppercase()
        .contains("BEGIN:VCALENDAR")
    {
        return Err(ParseError("not a calendar"));
    }
    let from = now - Duration::days(WINDOW_BEFORE_DAYS);
    let to = now + Duration::days(WINDOW_AFTER_DAYS);
    let (from_day, to_day) = (
        from.with_timezone(&local).date_naive(),
        to.with_timezone(&local).date_naive(),
    );
    let (events, mut cut) = raw_events(text);
    let mut expanded = 0usize;

    // Starts replaced by an override or cancelled, by event.
    let mut replaced: HashMap<&str, HashSet<Key>> = HashMap::new();
    for event in &events {
        if let (Some(uid), Some(id)) = (&event.uid, &event.recurrence_id)
            && let Some(m) = moments_of(id, local).first()
        {
            replaced.entry(uid).or_default().insert(m.key());
        }
    }

    let mut out = Vec::new();
    for event in &events {
        if event.cancelled {
            continue;
        }
        let Some(start) = event
            .dtstart
            .as_ref()
            .and_then(|p| moments_of(p, local).into_iter().next())
        else {
            continue;
        };
        let uid = event.uid.clone().unwrap_or_default();
        let len = length(event, &start, local);
        let starts = match (&event.rrule, &event.recurrence_id) {
            (Some(rule), None) => {
                let skipped: HashSet<Key> = event
                    .exdates
                    .iter()
                    .flat_map(|p| moments_of(p, local))
                    .map(|m| m.key())
                    .chain(replaced.get(uid.as_str()).into_iter().flatten().cloned())
                    .collect();
                let room = MAX_EXPANDED.saturating_sub(expanded);
                if room == 0 {
                    cut = true;
                    continue;
                }
                let (starts, was_cut) = expand(&start, rule, from, to, len, room)
                    .unwrap_or_else(|| (vec![start.clone()], false));
                cut |= was_cut;
                expanded += starts.len();
                starts
                    .into_iter()
                    .filter(|m| !skipped.contains(&m.key()))
                    .collect()
            }
            _ => vec![start],
        };
        for s in starts {
            let span = match s {
                Moment::Time(t) => {
                    let start = t.with_timezone(&Utc);
                    let end = start + len;
                    if start >= to || end < from {
                        continue;
                    }
                    Span::Timed { start, end }
                }
                Moment::Date(d) => {
                    let end = d + len;
                    if d > to_day || end <= from_day {
                        continue;
                    }
                    Span::AllDay { start: d, end }
                }
            };
            out.push(Occurrence {
                uid: uid.clone(),
                title: event.summary.clone().unwrap_or_default(),
                location: event.location.clone().filter(|l| !l.trim().is_empty()),
                span,
            });
        }
    }
    out.sort_by_key(|o| o.sort_key());
    if out.len() > MAX_OCCURRENCES {
        cut = true;
        out.truncate(MAX_OCCURRENCES);
    }
    if cut {
        eprintln!("{TOO_LARGE}");
    }
    Ok(out)
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

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn calendar(events: &str) -> String {
        format!("BEGIN:VCALENDAR\r\nVERSION:2.0\r\n{events}END:VCALENDAR\r\n")
    }

    fn titles(occurrences: &[Occurrence]) -> Vec<&str> {
        occurrences.iter().map(|o| o.title.as_str()).collect()
    }

    fn start_of(o: &Occurrence) -> DateTime<Utc> {
        match &o.span {
            Span::Timed { start, .. } => *start,
            other => panic!("not timed: {other:?}"),
        }
    }

    #[test]
    fn lines_unfold_and_text_unescapes() {
        assert_eq!(
            unfold("A:one\r\n two\r\n\tthree\nB:x"),
            vec!["A:onetwothree", "B:x"]
        );
        assert_eq!(unescape(r"a\, b\; c\\d\ne"), "a, b; c\\d\ne");
        let p = parse_line(r#"DTSTART;TZID="Europe/London";X=a:b:20250101T000000"#).unwrap();
        assert_eq!(p.name, "DTSTART");
        assert_eq!(p.param("TZID"), Some("Europe/London"));
        assert_eq!(p.value, "b:20250101T000000");
    }

    #[test]
    fn durations_parse() {
        assert_eq!(parse_duration("PT1H30M"), Some(Duration::minutes(90)));
        assert_eq!(parse_duration("P1DT2H"), Some(Duration::hours(26)));
        assert_eq!(parse_duration("P1W"), Some(Duration::days(7)));
        assert_eq!(parse_duration("-PT15M"), Some(Duration::minutes(-15)));
        assert_eq!(parse_duration("PT"), Some(Duration::zero()));
        assert_eq!(parse_duration("1H"), None);
        assert_eq!(parse_duration("P1H"), None);
    }

    #[test]
    fn the_google_export_parses_to_the_expected_occurrences() {
        let text = include_str!("../../samples/calendar/google-export.ics");
        let now = utc("2025-03-10 08:00");
        let all = parse_calendar(text, JOBURG, now).unwrap();
        assert_eq!(
            titles(&all),
            vec![
                "Daily standup with a folded title",
                "Design review",
                "Weekly sync (moved)",
                "Team offsite",
                "Weekly sync",
            ]
        );
        // UTC.
        assert_eq!(start_of(&all[0]), utc("2025-03-10 07:30"));
        // TZID Europe/London (GMT in early March).
        assert_eq!(start_of(&all[1]), utc("2025-03-10 09:30"));
        assert_eq!(all[1].location.as_deref(), Some("Room 4, Floor 2"));
        // The RECURRENCE-ID override replaced the 14:00 start with 15:00.
        assert_eq!(start_of(&all[2]), utc("2025-03-10 13:00"));
        assert_eq!(
            all[3].span,
            Span::AllDay {
                start: day("2025-03-11"),
                end: day("2025-03-12")
            }
        );
        // Tuesday only: Monday the 17th is an EXDATE and the 18th is past the window.
        assert_eq!(start_of(&all[4]), utc("2025-03-11 12:00"));
    }

    #[test]
    fn cancelled_events_are_dropped_and_the_cancel_hides_one_occurrence() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T100000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:No\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20250310T100000Z\r\nRRULE:FREQ=DAILY;COUNT=3\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nRECURRENCE-ID:20250311T100000Z\r\nDTSTART:20250311T100000Z\r\nSTATUS:CANCELLED\r\nSUMMARY:Daily\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        let starts: Vec<_> = all.iter().map(start_of).collect();
        assert_eq!(
            starts,
            vec![utc("2025-03-10 10:00"), utc("2025-03-12 10:00")]
        );
    }

    #[test]
    fn floating_times_use_the_local_zone_and_duration_sets_the_end() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T140000\r\nDURATION:PT45M\r\nSUMMARY:Float\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(
            all[0].span,
            Span::Timed {
                start: utc("2025-03-10 12:00"),
                end: utc("2025-03-10 12:45")
            }
        );
    }

    #[test]
    fn all_day_events_span_their_days_and_default_to_one() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART;VALUE=DATE:20250311\r\nDTEND;VALUE=DATE:20250314\r\nSUMMARY:Trip\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nDTSTART;VALUE=DATE:20250312\r\nSUMMARY:Day\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(
            all[0].span,
            Span::AllDay {
                start: day("2025-03-11"),
                end: day("2025-03-14")
            }
        );
        assert_eq!(
            all[1].span,
            Span::AllDay {
                start: day("2025-03-12"),
                end: day("2025-03-13")
            }
        );
    }

    #[test]
    fn events_outside_the_window_are_dropped() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250301T100000Z\r\nSUMMARY:Old\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20250320T100000Z\r\nSUMMARY:Far\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:c\r\nDTSTART:20250309T060000Z\r\nDTEND:20250309T090000Z\r\nSUMMARY:Edge\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(titles(&all), vec!["Edge"]);
    }

    #[test]
    fn a_minutely_series_keeps_one_occurrence() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T100000Z\r\nRRULE:FREQ=MINUTELY\r\nSUMMARY:Spam\r\nEND:VEVENT\r\n\
             BEGIN:VEVENT\r\nUID:b\r\nDTSTART:20250310T100000Z\r\nRRULE:FREQ=SECONDLY\r\nSUMMARY:Worse\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(titles(&all), vec!["Spam", "Worse"]);
    }

    #[test]
    fn an_ancient_hourly_series_ends_quickly_inside_the_window() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:19000101T000000Z\r\nRRULE:FREQ=HOURLY\r\nSUMMARY:Old\r\nEND:VEVENT\r\n",
        );
        let began = std::time::Instant::now();
        let now = utc("2025-03-10 08:00");
        let all = parse_calendar(&text, JOBURG, now).unwrap();
        assert!(began.elapsed() < std::time::Duration::from_secs(10));
        assert!(all.len() <= MAX_OCCURRENCES);
        // 100,000 hours from 1900 ends long before the window, so nothing is out of it.
        for o in &all {
            let Span::Timed { start, .. } = o.span else {
                panic!("timed expected")
            };
            assert!(start >= now - Duration::days(2) && start <= now + Duration::days(10));
        }
    }

    #[test]
    fn a_feed_with_too_many_events_is_cut() {
        let one =
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T100000Z\r\nSUMMARY:E\r\nEND:VEVENT\r\n";
        let text = calendar(&one.repeat(MAX_EVENTS + 50));
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(all.len(), MAX_OCCURRENCES);
        let (events, cut) = raw_events(&text);
        assert_eq!(events.len(), MAX_EVENTS);
        assert!(cut);
    }

    #[test]
    fn expansion_stops_at_the_feed_total() {
        // Each daily series gives ~10 in-window dates; 1500 series would be 15,000.
        let one = |n: usize| {
            format!(
                "BEGIN:VEVENT\r\nUID:u{n}\r\nDTSTART:20250301T100000Z\r\nRRULE:FREQ=DAILY\r\nSUMMARY:D\r\nEND:VEVENT\r\n"
            )
        };
        let text = calendar(&(0..1500).map(one).collect::<String>());
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(all.len(), MAX_OCCURRENCES);
    }

    #[test]
    fn a_bad_rule_keeps_the_first_occurrence_and_junk_is_refused() {
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART:20250310T100000Z\r\nRRULE:FREQ=NONSENSE\r\nSUMMARY:One\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2025-03-10 08:00")).unwrap();
        assert_eq!(titles(&all), vec!["One"]);
        assert!(parse_calendar("<html>hi</html>", JOBURG, utc("2025-03-10 08:00")).is_err());
    }

    #[test]
    fn an_until_without_a_zone_is_read_in_the_start_zone() {
        assert_eq!(
            utc_until("FREQ=WEEKLY;UNTIL=20261107", JOBURG),
            "FREQ=WEEKLY;UNTIL=20261107T215959Z"
        );
        assert_eq!(
            utc_until("FREQ=DAILY;until=20261107T090000;COUNT=3", JOBURG),
            "FREQ=DAILY;UNTIL=20261107T070000Z;COUNT=3"
        );
        for kept in [
            "FREQ=WEEKLY;UNTIL=20261107T215959Z",
            "FREQ=WEEKLY;UNTIL=junk",
        ] {
            assert_eq!(utc_until(kept, JOBURG), kept);
        }
    }

    #[test]
    fn a_weekly_series_with_a_date_until_repeats_through_that_day() {
        // Google exports a zoned start with a date-only UNTIL.
        let text = calendar(
            "BEGIN:VEVENT\r\nUID:a\r\nDTSTART;TZID=Africa/Johannesburg:20260720T083000\r\nDTEND;TZID=Africa/Johannesburg:20260720T092000\r\nRRULE:FREQ=WEEKLY;UNTIL=20261102\r\nSUMMARY:Lecture\r\nEND:VEVENT\r\n",
        );
        let all = parse_calendar(&text, JOBURG, utc("2026-10-31 06:00")).unwrap();
        // The last Monday is the UNTIL date itself.
        assert_eq!(all.len(), 1);
        assert_eq!(start_of(&all[0]), utc("2026-11-02 06:30"));
    }
}
