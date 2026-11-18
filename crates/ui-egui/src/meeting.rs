//! Home > Meeting Details (issue #283): the details of a meeting from a calendar file, put on
//! the page as ordinary words.
//!
//! The calendar is a file the person chose (`.ics`, as every calendar exports and every
//! invitation carries), read with `notecraft_formats::ics`. NoteCraft connects to no calendar
//! service and sends nothing; the file is only read. What goes on the page is text the person
//! can change like any other: nothing on the page stays tied to the calendar.

use notecraft_formats::Block;
use notecraft_formats::ics::{Event, Occurrence, When};
use notecraft_formats::markdown::Span;

/// The largest calendar file read, in bytes.
pub const MAX_FILE: u64 = 8 * 1024 * 1024;

/// A time the way a note shows it: on the person's clock for a moment, as written with its zone
/// named for a time on a zone's clock, and as the day alone for a whole day.
pub fn when_text(w: &When, offset: i32) -> String {
    match w {
        When::Utc(secs) => notecraft_model::time::long_from_iso_at(&notecraft_model::time::iso_from_secs(*secs), offset).unwrap_or_default(),
        When::Local { secs, zone } => {
            let iso = notecraft_model::time::iso_from_secs(*secs);
            let shown = notecraft_model::time::long_from_iso_at(iso.trim_end_matches('Z'), 0).unwrap_or_default();
            match zone {
                Some(zone) => format!("{shown} ({zone})"),
                None => shown,
            }
        }
        When::Day(day) => notecraft_model::time::date_from_iso(&notecraft_model::time::iso_from_secs(day * 86_400)).unwrap_or_default(),
    }
}

/// When an occurrence is, start to end. An end on the same day is only its clock.
pub fn span_text(o: &Occurrence, offset: i32) -> String {
    let start = when_text(&o.start, offset);
    let Some(end) = o.end.as_ref() else { return start };
    match (&o.start, end) {
        // A whole day ends the day after its last; one day is just that day.
        (When::Day(a), When::Day(b)) if *b <= a + 1 => start,
        (When::Day(_), When::Day(b)) => {
            format!("{start} {} {}", crate::tl_id!("ui.meeting_to"), when_text(&When::Day(b - 1), offset))
        }
        _ => {
            let same_day = o.start.local_secs(offset).div_euclid(86_400) == end.local_secs(offset).div_euclid(86_400);
            let end_text = when_text(end, offset);
            let end_text = if same_day {
                // "Monday, 23 November 2026, 10:00 (Zone)" -> "10:00 (Zone)": the day is already said.
                end_text.split(", ").last().unwrap_or(&end_text).to_string()
            } else {
                end_text
            };
            // The zone is said once, after the end.
            let start = match &o.start {
                When::Local { zone: Some(zone), .. } if same_day => start.trim_end_matches(&format!(" ({zone})")).to_string(),
                _ => start,
            };
            format!("{start} {} {end_text}", crate::tl_id!("ui.meeting_to"))
        }
    }
}

fn plain(text: &str) -> Span {
    Span { text: text.to_string(), ..Span::default() }
}

fn bold(text: &str) -> Span {
    Span { text: text.to_string(), bold: true, ..Span::default() }
}

/// A line's words without their markup, as the dialog shows what will be put on the page.
pub fn plain_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) => spans.iter().map(|s| s.text.as_str()).collect(),
        other => crate::pageedit::block_text(other),
    }
}

/// One line: a name in bold and what it says.
fn line(name: &str, value: &str) -> Block {
    Block::Paragraph(vec![bold(&format!("{name}: ")), plain(value)])
}

/// The details of one meeting as lines of a page: its title, when, where, its link, who
/// organised it, who was asked, how it repeats, and its agenda. A field the file does not have
/// is left out rather than written empty.
pub fn details(event: &Event, o: &Occurrence, offset: i32) -> Vec<Block> {
    let title = if event.summary.is_empty() { crate::tl_id!("ui.meeting_with_no_title").to_string() } else { event.summary.clone() };
    let mut out = vec![Block::Heading { level: 3, spans: vec![plain(&title)] }];
    out.push(line(crate::tl_id!("ui.meeting_when"), &span_text(o, offset)));
    if !event.location.is_empty() {
        out.push(line(crate::tl_id!("ui.meeting_where"), &event.location));
    }
    if event.url.starts_with("https://") || event.url.starts_with("http://") {
        out.push(Block::Paragraph(vec![
            bold(&format!("{}: ", crate::tl_id!("ui.meeting_link"))),
            Span { text: event.url.clone(), link: Some(event.url.clone()), ..Span::default() },
        ]));
    }
    if let Some(organizer) = event.organizer.as_ref().map(notecraft_formats::ics::Person::shown).filter(|o| !o.is_empty()) {
        out.push(line(crate::tl_id!("ui.meeting_organizer"), &organizer));
    }
    let people: Vec<String> = event.attendees.iter().map(notecraft_formats::ics::Person::shown).filter(|p| !p.is_empty()).collect();
    if !people.is_empty() {
        out.push(line(crate::tl_id!("ui.meeting_participants"), &people.join(", ")));
    }
    if event.rule.is_some() {
        let how = if notecraft_formats::ics::repeats_followed(event) {
            crate::tl_id!("ui.meeting_repeats")
        } else {
            crate::tl_id!("ui.meeting_repeats_not_followed")
        };
        out.push(line(crate::tl_id!("ui.meeting_recurrence"), how));
    }
    let agenda: Vec<&str> = event.description.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    if !agenda.is_empty() {
        out.push(Block::Paragraph(vec![bold(crate::tl_id!("ui.meeting_agenda"))]));
        out.extend(agenda.iter().map(|l| Block::Paragraph(vec![plain(l)])));
    }
    out
}

/// What the Meeting Details dialog keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct Choice {
    /// The calendar file, as chosen.
    pub file: Option<std::path::PathBuf>,
    pub events: Vec<Event>,
    /// The day shown, as days since 1970 on the person's clock.
    pub day: i64,
    /// The occurrence picked, as its place in the day's list.
    pub picked: Option<usize>,
    /// Why the file could not be read.
    pub error: Option<String>,
}

impl Choice {
    /// Reads `file` as a calendar. A file that is too large, unreadable or holds no events
    /// says so rather than showing an empty day as if there were no meetings.
    pub fn read(&mut self, file: &std::path::Path) {
        self.file = Some(file.to_path_buf());
        self.picked = None;
        self.events.clear();
        self.error = None;
        let size = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
        if size > MAX_FILE {
            self.error = Some(crate::tl_id!("ui.meeting_file_too_large").to_string());
            return;
        }
        match std::fs::read(file) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                self.events = notecraft_formats::ics::parse(&text);
                if self.events.is_empty() {
                    self.error = Some(crate::tl_id!("ui.meeting_file_has_no_events").to_string());
                }
            }
            Err(e) => self.error = Some(crate::i18n::format_message_id("ui.meeting_file_unreadable_error", &[("error", &e.to_string())])),
        }
    }

    /// The meetings on the day shown, earliest first.
    pub fn today(&self, offset: i32) -> Vec<Occurrence> {
        notecraft_formats::ics::between(&self.events, self.day * 86_400, (self.day + 1) * 86_400, offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_name_only_what_the_file_has_and_keep_the_zone_it_said() {
        let file = "BEGIN:VEVENT\nSUMMARY:Planning\nDTSTART;TZID=Europe/Berlin:20261123T091500\nDTEND;TZID=Europe/Berlin:20261123T100000\nATTENDEE;CN=Ada:mailto:ada@example.org\nDESCRIPTION:Budget\\n\\nHiring\nEND:VEVENT\n";
        let events = notecraft_formats::ics::parse(file);
        let day = notecraft_formats::ics::between(&events, 0, i64::MAX / 2, 0);
        let blocks = details(&events[0], &day[0], 3_600);
        let text: Vec<String> = blocks.iter().map(plain_text).collect();
        assert_eq!(text[0], "Planning");
        assert_eq!(text[1], "When: Monday, 23 November 2026, 09:15 to 10:00 (Europe/Berlin)", "a zone's clock is not moved to the person's");
        assert_eq!(text[2], "Participants: Ada");
        assert_eq!(&text[3..], ["Agenda", "Budget", "Hiring"], "no Where, Link or Organizer the file does not have");
    }

    #[test]
    fn a_utc_meeting_is_shown_on_the_persons_clock() {
        let file = "BEGIN:VEVENT\nSUMMARY:Call\nDTSTART:20261123T230000Z\nDTEND:20261124T000000Z\nURL:javascript:alert(1)\nEND:VEVENT\n";
        let events = notecraft_formats::ics::parse(file);
        let all = notecraft_formats::ics::between(&events, 0, i64::MAX / 2, 3_600);
        let text: Vec<String> = details(&events[0], &all[0], 3_600).iter().map(plain_text).collect();
        assert_eq!(text[1], "When: Tuesday, 24 November 2026, 00:00 to 01:00");
        assert_eq!(text.len(), 2, "a link that is not a web address is not put on the page: {text:?}");
    }
}
