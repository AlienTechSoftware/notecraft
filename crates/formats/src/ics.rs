//! iCalendar (RFC 5545) files, read for Home > Meeting Details (issue #283).
//!
//! A meeting comes from a calendar file the person chose: one exported from their calendar, or
//! saved from an invitation. NoteCraft reads it and nothing else; it signs in to no calendar
//! service and sends nothing anywhere. This reader takes what a meeting note needs from each
//! `VEVENT`: its title, when it is, where, its link, who organised it, who was asked, what it is
//! about, and whether it repeats. Anything else in the file is passed over.
//!
//! Times are kept the way the file says them. A UTC time (`...Z`) is a moment, shown on the
//! person's clock. A time with a `TZID` is a time on that zone's clock: NoteCraft has no zone
//! database, so it is shown as written with the zone's name beside it, never moved to a clock
//! it is not on. A time with neither is the same on every clock. A whole-day event has a date.

use std::collections::HashSet;

/// How many events one file gives, and how many steps one repeat rule is followed for: a
/// daily meeting for a hundred years.
const MAX_EVENTS: usize = 5_000;
const MAX_STEPS: i64 = 40_000;

/// When something happens, as the file said it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum When {
    /// A moment, in seconds since 1970 UTC.
    Utc(i64),
    /// A time on a clock: seconds since 1970 as if that clock were UTC, and the zone the file
    /// names for it (`None` for a time that is the same on every clock).
    Local { secs: i64, zone: Option<String> },
    /// A whole day, as days since 1970.
    Day(i64),
}

impl When {
    /// Seconds since 1970 on the person's clock, `offset` seconds ahead of UTC, for putting
    /// events in order and in a day. A time on a named zone's clock is taken as it is written,
    /// which is right when that zone is the person's own.
    pub fn local_secs(&self, offset: i32) -> i64 {
        match self {
            When::Utc(s) => s + i64::from(offset),
            When::Local { secs, .. } => *secs,
            When::Day(d) => d * 86_400,
        }
    }

    /// Moved on by `secs` seconds, or for a whole day by whole days.
    fn plus(&self, secs: i64) -> When {
        match self {
            When::Utc(s) => When::Utc(s + secs),
            When::Local { secs: s, zone } => When::Local { secs: s + secs, zone: zone.clone() },
            When::Day(d) => When::Day(d + secs.div_euclid(86_400)),
        }
    }
}

/// A person on an event: their name when the file gives one, and their address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Person {
    pub name: Option<String>,
    pub address: Option<String>,
}

impl Person {
    /// The name, or the address when there is no name.
    pub fn shown(&self) -> String {
        self.name.clone().or_else(|| self.address.clone()).unwrap_or_default()
    }
}

/// One event in the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Event {
    pub uid: String,
    pub summary: String,
    pub start: Option<When>,
    pub end: Option<When>,
    pub location: String,
    pub url: String,
    pub description: String,
    pub organizer: Option<Person>,
    pub attendees: Vec<Person>,
    /// The repeat rule as written, when the event repeats.
    pub rule: Option<String>,
    /// Days or times taken out of the repeats (`EXDATE`), as the file's own values.
    excluded: Vec<When>,
    pub cancelled: bool,
    /// Original start of an exception to a recurrence (RECURRENCE-ID).
    recurrence_id: Option<When>,
}

/// Undoes RFC 5545 text escapes: `\n`, `\,`, `\;` and `\\`.
fn unescape(text: &str) -> String {
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

/// The file's lines with folded lines joined: a line that starts with a space or a tab goes
/// on the end of the one before it.
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        match (line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }
    lines
}

/// One content line split into its name, its parameters and its value. Quoted parameter
/// values may hold `:` and `;`.
fn split_line(line: &str) -> Option<(String, Vec<(String, String)>, String)> {
    let mut quoted = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ':' if !quoted => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in head.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(&head[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&head[start..]);
    let name = parts.first()?.trim().to_ascii_uppercase();
    let params = parts[1..]
        .iter()
        .filter_map(|p| p.split_once('='))
        .map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string()))
        .collect();
    Some((name, params, value.to_string()))
}

fn param<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Days since 1970 for a date.
fn days(y: i64, m: u32, d: u32) -> Option<i64> {
    if !(1..=12).contains(&m) || d == 0 || d > month_len(y, m) {
        return None;
    }
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

/// (year, month, day) for days since 1970.
pub fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn month_len(y: i64, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// A `DATE` (`20261123`) or `DATE-TIME` (`20261123T090000`, `...Z`) value with its parameters.
fn when(value: &str, params: &[(String, String)]) -> Option<When> {
    let v = value.trim();
    let date = v.get(..8)?;
    let (y, m, d): (i64, u32, u32) = (date.get(..4)?.parse().ok()?, date.get(4..6)?.parse().ok()?, date.get(6..8)?.parse().ok()?);
    let day = days(y, m, d)?;
    if param(params, "VALUE").is_some_and(|t| t.eq_ignore_ascii_case("DATE")) || v.len() == 8 {
        return Some(When::Day(day));
    }
    let time = v.get(9..15)?;
    if v.as_bytes().get(8) != Some(&b'T') {
        return None;
    }
    let (h, mi, s): (i64, i64, i64) = (time.get(..2)?.parse().ok()?, time.get(2..4)?.parse().ok()?, time.get(4..6)?.parse().ok()?);
    if h > 23 || mi > 59 || s > 60 {
        return None;
    }
    let secs = day * 86_400 + h * 3_600 + mi * 60 + s;
    if v.ends_with('Z') {
        return Some(When::Utc(secs));
    }
    Some(When::Local { secs, zone: param(params, "TZID").map(str::to_string) })
}

/// The person a `CN` parameter and a `mailto:` value name.
fn person(value: &str, params: &[(String, String)]) -> Person {
    let address = value.trim();
    let address = address.strip_prefix("mailto:").or_else(|| address.strip_prefix("MAILTO:")).unwrap_or(address);
    Person {
        name: param(params, "CN").map(|n| unescape(n).trim().to_string()).filter(|n| !n.is_empty()),
        address: (!address.is_empty()).then(|| address.to_string()),
    }
}

/// Every event in an iCalendar file, in the order the file has them.
pub fn parse(text: &str) -> Vec<Event> {
    let mut out = Vec::new();
    let mut current: Option<Event> = None;
    // Inside a part of an event that is not the event itself (an alarm), whose lines are not
    // the event's.
    let mut nested = 0usize;
    for line in unfold(text) {
        let Some((name, params, value)) = split_line(&line) else { continue };
        match (name.as_str(), value.trim().to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") if current.is_none() => current = Some(Event::default()),
            ("BEGIN", _) if current.is_some() => nested += 1,
            ("END", "VEVENT") if nested == 0 => {
                if let Some(event) = current.take()
                    && (event.start.is_some() || (event.cancelled && event.recurrence_id.is_some()))
                    && out.len() < MAX_EVENTS
                {
                    out.push(event);
                }
            }
            ("END", _) if current.is_some() => nested = nested.saturating_sub(1),
            _ => {
                let Some(event) = current.as_mut().filter(|_| nested == 0) else { continue };
                match name.as_str() {
                    "UID" => event.uid = value.trim().to_string(),
                    "SUMMARY" => event.summary = unescape(&value).trim().to_string(),
                    "LOCATION" => event.location = unescape(&value).trim().to_string(),
                    "URL" => event.url = value.trim().to_string(),
                    "DESCRIPTION" => event.description = unescape(&value).trim().to_string(),
                    "DTSTART" => event.start = when(&value, &params),
                    "RECURRENCE-ID" => event.recurrence_id = when(&value, &params),
                    "DTEND" => event.end = when(&value, &params),
                    "ORGANIZER" => event.organizer = Some(person(&value, &params)),
                    "ATTENDEE" if event.attendees.len() < 500 => event.attendees.push(person(&value, &params)),
                    "RRULE" => event.rule = Some(value.trim().to_string()),
                    "EXDATE" => event.excluded.extend(value.split(',').filter_map(|v| when(v, &params))),
                    "STATUS" => event.cancelled = value.trim().eq_ignore_ascii_case("CANCELLED"),
                    _ => {}
                }
            }
        }
    }
    out
}

/// One time an event happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Which event, as its place in what [`parse`] gave.
    pub event: usize,
    pub start: When,
    pub end: Option<When>,
}

/// What a repeat rule says, as far as this reader follows it. `None` for a rule it does not
/// follow, whose event is then shown on its first day only.
struct Rule {
    freq: char,
    interval: i64,
    count: Option<usize>,
    until: Option<When>,
    /// Start of the week, Monday = 0.
    week_start: i64,
    /// Sorted day offsets from the start of the week, for a weekly rule.
    weekdays: Vec<i64>,
}

fn parse_weekday(text: &str) -> Option<i64> {
    ["MO", "TU", "WE", "TH", "FR", "SA", "SU"].iter().position(|day| day.eq_ignore_ascii_case(text.trim())).map(|at| at as i64)
}

fn rule(text: &str) -> Option<Rule> {
    let mut freq = None;
    let mut r = Rule { freq: 'D', interval: 1, count: None, until: None, week_start: 0, weekdays: Vec::new() };
    for part in text.split(';') {
        let (k, v) = part.split_once('=')?;
        match k.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = Some(match v.trim().to_ascii_uppercase().as_str() {
                    "DAILY" => 'D',
                    "WEEKLY" => 'W',
                    "MONTHLY" => 'M',
                    "YEARLY" => 'Y',
                    _ => return None,
                })
            }
            "INTERVAL" => r.interval = v.trim().parse::<i64>().ok().filter(|n| (1..=1000).contains(n))?,
            "COUNT" => r.count = Some(v.trim().parse::<usize>().ok()?),
            "UNTIL" => r.until = Some(when(v, &[])?),
            "BYDAY" => {
                for day in v.split(',') {
                    // "1MO" (the first Monday) is not followed.
                    r.weekdays.push(parse_weekday(day)?);
                }
            }
            "WKST" => r.week_start = parse_weekday(v)?,
            _ => return None,
        }
    }
    r.freq = freq?;
    if !r.weekdays.is_empty() && r.freq != 'W' {
        return None;
    }
    for day in &mut r.weekdays {
        *day = (*day - r.week_start).rem_euclid(7);
    }
    r.weekdays.sort_unstable();
    r.weekdays.dedup();
    Some(r)
}

/// True when this reader lays out every time the event repeats; false when it repeats in a way
/// only its first time is shown for.
pub fn repeats_followed(event: &Event) -> bool {
    event.rule.as_deref().is_none_or(|r| rule(r).is_some())
}

/// Monday = 0 for days since 1970 (a Thursday).
fn weekday(day: i64) -> i64 {
    (day + 3).rem_euclid(7)
}

/// The same time `months` months on, or `None` when that month has no such day.
fn plus_months(start: &When, months: i64) -> Option<When> {
    let (day, secs_in_day) = match start {
        When::Utc(s) | When::Local { secs: s, .. } => (s.div_euclid(86_400), s.rem_euclid(86_400)),
        When::Day(d) => (*d, 0),
    };
    let (y, m, d) = civil(day);
    let total = i64::from(m) - 1 + months;
    let (y, m) = (y + total.div_euclid(12), u32::try_from(total.rem_euclid(12) + 1).ok()?);
    let moved = days(y, m, d)?;
    Some(match start {
        When::Utc(_) => When::Utc(moved * 86_400 + secs_in_day),
        When::Local { zone, .. } => When::Local { secs: moved * 86_400 + secs_in_day, zone: zone.clone() },
        When::Day(_) => When::Day(moved),
    })
}

/// Every time the events happen that starts in `from..to` (seconds on the person's clock,
/// `offset` seconds ahead of UTC), earliest first. Cancelled events are left out.
pub fn between(events: &[Event], from: i64, to: i64, offset: i32) -> Vec<Occurrence> {
    // Exceptions can precede or follow the series in the file. Match the UID and the
    // original occurrence time, not a cancelled component's own DTSTART.
    let cancelled: HashSet<(&str, &When)> = events
        .iter()
        .filter(|event| event.cancelled && !event.uid.is_empty())
        .filter_map(|event| event.recurrence_id.as_ref().map(|at| (event.uid.as_str(), at)))
        .collect();
    let mut out = Vec::new();
    for (i, event) in events.iter().enumerate() {
        let Some(start) = event.start.clone() else { continue };
        if event.cancelled {
            continue;
        }
        let length = event.end.as_ref().map(|e| e.local_secs(offset) - start.local_secs(offset));
        let mut push = |at: When| {
            let s = at.local_secs(offset);
            // A whole-day event is on every day it covers.
            let reach = length.unwrap_or(0).max(0);
            let in_range = if matches!(at, When::Day(_)) { s < to && s + reach.max(86_400) > from } else { s >= from && s < to };
            let skipped = event.excluded.iter().any(|x| x.local_secs(offset) == s || matches!(x, When::Day(d) if *d == s.div_euclid(86_400)));
            if in_range && !skipped && !cancelled.contains(&(event.uid.as_str(), &at)) {
                out.push(Occurrence { event: i, end: length.map(|l| at.plus(l)), start: at });
            }
        };
        let Some(r) = event.rule.as_deref().and_then(rule) else {
            push(start);
            continue;
        };
        let until = r.until.as_ref().map(|u| u.local_secs(offset));
        let mut made = 0usize;
        let mut step = 0i64;
        'repeats: while step < MAX_STEPS {
            // The times this step gives: one, or for a weekly rule with weekdays, one a day.
            let times: Vec<When> = match r.freq {
                'D' => vec![start.plus(step * r.interval * 86_400)],
                'W' if r.weekdays.is_empty() => vec![start.plus(step * r.interval * 7 * 86_400)],
                'W' => {
                    // Generate on the event's own clock; the viewer's offset is only
                    // for filtering and display and must not change the weekdays.
                    let first = start.local_secs(0).div_euclid(86_400);
                    let week = first - (weekday(first) - r.week_start).rem_euclid(7) + step * r.interval * 7;
                    r.weekdays
                        .iter()
                        .map(|wd| start.plus((week + wd - first) * 86_400))
                        .filter(|t| t.local_secs(offset) >= start.local_secs(offset))
                        .collect()
                }
                'M' => plus_months(&start, step * r.interval).into_iter().collect(),
                _ => plus_months(&start, step * r.interval * 12).into_iter().collect(),
            };
            step += 1;
            for at in times {
                let s = at.local_secs(offset);
                if until.is_some_and(|u| s > u) || s >= to || r.count.is_some_and(|c| made >= c) {
                    break 'repeats;
                }
                made += 1;
                push(at);
            }
        }
    }
    out.sort_by_key(|o| o.start.local_secs(offset));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:one@example\r\nSUMMARY:Project review\\, phase 2\r\nDTSTART:20261123T150000Z\r\nDTEND:20261123T160000Z\r\nLOCATION:Room 4\r\nURL:https://meet.example/abc\r\nDESCRIPTION:Agenda:\\n1. Status\\n2. Risks\r\nORGANIZER;CN=\"Lee, Sam\":mailto:sam@example.org\r\nATTENDEE;CN=Ada;ROLE=REQ-PARTICIPANT:mailto:ada@example.org\r\nATTENDEE:mailto:grace@exa\r\n mple.org\r\nBEGIN:VALARM\r\nDESCRIPTION:Not the event's\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Stand-up\r\nDTSTART;TZID=Europe/Berlin:20261123T091500\r\nDTEND;TZID=Europe/Berlin:20261123T093000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=5\r\nEXDATE;TZID=Europe/Berlin:20261125T091500\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Holiday\r\nDTSTART;VALUE=DATE:20261124\r\nDTEND;VALUE=DATE:20261126\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Gone\r\nSTATUS:CANCELLED\r\nDTSTART:20261123T100000Z\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    fn day(y: i64, m: u32, d: u32) -> i64 {
        days(y, m, d).expect("a day") * 86_400
    }

    #[test]
    fn an_event_gives_its_title_time_place_link_people_and_agenda() {
        let events = parse(FILE);
        assert_eq!(events.len(), 4);
        let e = &events[0];
        assert_eq!(e.summary, "Project review, phase 2");
        assert_eq!(e.start, Some(When::Utc(day(2026, 11, 23) + 15 * 3600)));
        assert_eq!(e.location, "Room 4");
        assert_eq!(e.url, "https://meet.example/abc");
        assert_eq!(e.description, "Agenda:\n1. Status\n2. Risks", "the alarm's description is not the event's");
        assert_eq!(e.organizer.as_ref().map(Person::shown).as_deref(), Some("Lee, Sam"), "a quoted name may hold a comma");
        assert_eq!(e.attendees.iter().map(Person::shown).collect::<Vec<_>>(), ["Ada", "grace@example.org"], "a folded line is joined");
        assert_eq!(events[1].start, Some(When::Local { secs: day(2026, 11, 23) + 9 * 3600 + 900, zone: Some("Europe/Berlin".into()) }));
        assert_eq!(events[2].start, Some(When::Day(day(2026, 11, 24) / 86_400)));
        assert!(events[3].cancelled);
    }

    #[test]
    fn a_day_shows_the_events_on_it_with_repeats_laid_out_and_cancelled_ones_left_out() {
        let events = parse(FILE);
        let monday = between(&events, day(2026, 11, 23), day(2026, 11, 24), 0);
        assert_eq!(monday.iter().map(|o| events[o.event].summary.as_str()).collect::<Vec<_>>(), ["Stand-up", "Project review, phase 2"]);
        let tuesday = between(&events, day(2026, 11, 24), day(2026, 11, 25), 0);
        assert_eq!(tuesday.iter().map(|o| events[o.event].summary.as_str()).collect::<Vec<_>>(), ["Holiday"]);
        let wednesday = between(&events, day(2026, 11, 25), day(2026, 11, 26), 0);
        assert_eq!(wednesday.iter().map(|o| events[o.event].summary.as_str()).collect::<Vec<_>>(), ["Holiday"], "taken out of the repeats");
        let fortnight = between(&events, day(2026, 11, 23), day(2026, 12, 7), 0);
        let standups: Vec<i64> = fortnight.iter().filter(|o| o.event == 1).map(|o| o.start.local_secs(0) / 86_400).collect();
        // Mon 23, (Wed 25 excluded), Fri 27, Mon 30, Wed 2: five counted, one taken out.
        assert_eq!(standups, [day(2026, 11, 23), day(2026, 11, 27), day(2026, 11, 30), day(2026, 12, 2)].map(|d| d / 86_400));
        // On a clock two hours ahead, the UTC review is at 17:00 the same day.
        let ahead = between(&events, day(2026, 11, 23), day(2026, 11, 24), 7_200);
        assert!(ahead.iter().any(|o| o.event == 0 && o.start.local_secs(7_200) == day(2026, 11, 23) + 17 * 3600));
    }

    #[test]
    fn monthly_repeats_skip_months_without_the_day_and_odd_rules_show_once() {
        let text = "BEGIN:VEVENT\nSUMMARY:Pay\nDTSTART:20260131T090000\nRRULE:FREQ=MONTHLY;UNTIL=20260630T000000\nEND:VEVENT\nBEGIN:VEVENT\nSUMMARY:Odd\nDTSTART:20260105T090000\nRRULE:FREQ=MONTHLY;BYDAY=1MO\nEND:VEVENT\n";
        let events = parse(text);
        let year = between(&events, day(2026, 1, 1), day(2027, 1, 1), 0);
        let pay: Vec<(i64, u32, u32)> = year.iter().filter(|o| o.event == 0).map(|o| civil(o.start.local_secs(0) / 86_400)).collect();
        assert_eq!(pay, [(2026, 1, 31), (2026, 3, 31), (2026, 5, 31)]);
        assert!(!repeats_followed(&events[1]));
        assert_eq!(year.iter().filter(|o| o.event == 1).count(), 1, "shown on its first day only");
    }

    #[test]
    fn junk_gives_no_events_and_no_panic() {
        assert!(parse("").is_empty());
        assert!(parse("BEGIN:VEVENT\nSUMMARY:No time\nEND:VEVENT\n").is_empty(), "an event with no start is not one");
        assert!(parse("BEGIN:VEVENT\nDTSTART:2026\nDTSTART:20261340T000000\nEND:VEVENT").is_empty());
        assert!(parse(":\n;;;\nBEGIN\nEND:VEVENT\n\u{0}").is_empty());
    }
}

#[cfg(test)]
mod weekly_and_cancelled_tests {
    use super::*;

    fn june(day: u32) -> i64 {
        days(2026, 6, day).expect("June date") * 86_400
    }

    fn june_days(text: &str, from: u32, to: u32, offset: i32) -> Vec<u32> {
        between(&parse(text), june(from), june(to), offset).iter().map(|o| civil(o.start.local_secs(offset).div_euclid(86_400)).2).collect()
    }

    #[test]
    fn weekly_weekdays_are_a_set_and_count_and_window_follow_chronological_order() {
        for days in ["MO,FR", "FR,MO", "FR,MO,FR"] {
            let text = format!("BEGIN:VEVENT\nDTSTART:20260601T090000Z\nRRULE:FREQ=WEEKLY;BYDAY={days};COUNT=4\nEND:VEVENT\n");
            assert_eq!(june_days(&text, 1, 3, 0), [1], "{days}");
            assert_eq!(june_days(&text, 1, 29, 0), [1, 5, 8, 12], "{days}");
        }
    }

    #[test]
    fn a_biweekly_rule_starts_each_week_on_wkst_and_defaults_to_monday() {
        for (week_start, expected) in [("", [2, 7, 16, 21]), (";WKST=SU", [2, 14, 16, 28])] {
            let text = format!("BEGIN:VEVENT\nDTSTART:20260602T090000\nRRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=TU,SU;COUNT=4{week_start}\nEND:VEVENT\n");
            assert_eq!(june_days(&text, 1, 30, 0), expected, "{week_start}");
        }
    }

    #[test]
    fn a_utc_weekly_rule_uses_utc_weekdays_even_if_the_viewers_clock_crosses_midnight() {
        let text = "BEGIN:VEVENT\nDTSTART:20260608T013000Z\nRRULE:FREQ=WEEKLY;BYDAY=MO,FR;COUNT=4\nEND:VEVENT\n";
        assert_eq!(june_days(text, 1, 30, 0), [8, 12, 15, 19]);
        assert_eq!(june_days(text, 1, 30, -4 * 3600), [7, 11, 14, 18]);
    }

    #[test]
    fn a_cancelled_exception_without_dtstart_removes_only_its_uid_and_recurrence_id() {
        let text = "BEGIN:VEVENT\nUID:standup\nRECURRENCE-ID;TZID=America/Toronto:20260602T090000\nSTATUS:CANCELLED\nEND:VEVENT\n\
                    BEGIN:VEVENT\nUID:standup\nDTSTART;TZID=America/Toronto:20260601T090000\nRRULE:FREQ=DAILY;COUNT=3\nEND:VEVENT\n\
                    BEGIN:VEVENT\nUID:unrelated\nDTSTART;TZID=America/Toronto:20260602T090000\nEND:VEVENT\n";
        let events = parse(text);
        let meetings = between(&events, june(1), june(7), 0);
        assert_eq!(meetings.iter().map(|o| events[o.event].uid.as_str()).collect::<Vec<_>>(), ["standup", "unrelated", "standup"]);
        assert_eq!(meetings.iter().map(|o| civil(o.start.local_secs(0).div_euclid(86_400)).2).collect::<Vec<_>>(), [1, 2, 3]);
    }
}
