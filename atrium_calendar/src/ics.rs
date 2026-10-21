//! iCalendar files (.ics): a calendar out, and events in. Times leave in UTC; a time zone named in a file (`TZID`) is applied when it is read.

use aether_sdk::dates::{self, NaiveDate, NaiveDateTime, NaiveTime};
use aether_sdk::prelude::*;

use crate::access::{require, Role};
use crate::calendars::CalendarOnly;
use crate::common::*;
use crate::events::span;
use crate::schedule::parse;

const MAX_IMPORT: usize = 1000;
const MAX_BYTES: usize = 1_000_000;

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace("\r\n", "\\n").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Lines longer than 75 characters continue on the next line after a space.
fn fold(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= 75 {
        return line.to_string();
    }
    let mut out = String::new();
    for (i, chunk) in chars.chunks(74).enumerate() {
        if i > 0 {
            out.push_str("\r\n ");
        }
        out.extend(chunk);
    }
    out
}

fn ics_moment(moment: NaiveDateTime) -> String {
    moment.format("%Y%m%dT%H%M%SZ").to_string()
}

fn ics_date(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn freq(repeat: &str) -> Option<&'static str> {
    match repeat {
        "daily" => Some("DAILY"),
        "weekly" => Some("WEEKLY"),
        "monthly" => Some("MONTHLY"),
        "yearly" => Some("YEARLY"),
        _ => None,
    }
}

pub fn render(name: &str, events: &[Record], skipped: &[(String, NaiveDate)], stamp: NaiveDateTime) -> String {
    let mut lines = vec!["BEGIN:VCALENDAR".to_string(), "VERSION:2.0".into(), "PRODID:-//Atrium//Calendar//EN".into(), "CALSCALE:GREGORIAN".into(), format!("X-WR-CALNAME:{}", escape(name))];
    for event in events {
        let (Some(start), Some(end)) = (text(event, "starts_at").and_then(|t| parse(t).ok()), text(event, "ends_at").and_then(|t| parse(t).ok())) else { continue };
        let id = key(id_text(event));
        lines.push("BEGIN:VEVENT".into());
        lines.push(format!("UID:{id}@atrium"));
        lines.push(format!("DTSTAMP:{}", ics_moment(stamp)));
        if flag(event, "all_day") {
            lines.push(format!("DTSTART;VALUE=DATE:{}", ics_date(start.date())));
            // The end of an all-day event is the day after its last day.
            lines.push(format!("DTEND;VALUE=DATE:{}", ics_date(end.date() + dates::Duration::days(1))));
        } else {
            lines.push(format!("DTSTART:{}", ics_moment(start)));
            lines.push(format!("DTEND:{}", ics_moment(end)));
        }
        lines.push(format!("SUMMARY:{}", escape(text(event, "title").unwrap_or_default())));
        for (field, name) in [("description", "DESCRIPTION"), ("location", "LOCATION")] {
            if let Some(v) = text(event, field).filter(|v| !v.is_empty()) {
                lines.push(format!("{name}:{}", escape(v)));
            }
        }
        if let Some(f) = freq(text(event, "repeat").unwrap_or("none")) {
            let until = text(event, "until").and_then(|u| dates::parse_date(u).ok()).map(|u| format!(";UNTIL={}T235959Z", ics_date(u))).unwrap_or_default();
            lines.push(format!("RRULE:FREQ={f}{until}"));
            for (_, day) in skipped.iter().filter(|(e, _)| e == id) {
                lines.push(format!("EXDATE:{}", ics_moment(day.and_time(start.time()))));
            }
        }
        lines.push("END:VEVENT".into());
    }
    lines.push("END:VCALENDAR".into());
    lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("\r\n") + "\r\n"
}

/// An event read from a file.
#[derive(Debug, Default, PartialEq)]
pub struct Parsed {
    pub title: String,
    pub description: String,
    pub location: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub all_day: bool,
    pub repeat: String,
    pub until: Option<String>,
    /// A rule this plugin cannot keep (every other week, certain weekdays, a count).
    pub rule_ignored: bool,
}

fn unfold(source: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in source.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if let Some(rest) = raw.strip_prefix(' ').or_else(|| raw.strip_prefix('\t')) {
            if let Some(last) = lines.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        lines.push(raw.to_string());
    }
    lines
}

/// `20261020T090000Z`, `20261020T090000`, `20261020` as the text `span` reads.
///
/// A time with a `Z` keeps it (it is UTC), and so does one with a `TZID` the plugin knows (it is turned into UTC); a time with
/// neither is the importer's own and is read in their zone.
fn moment_text(value: &str) -> Option<(String, bool)> {
    moment_in(value, None)
}

fn moment_in(value: &str, zone: Option<&str>) -> Option<(String, bool)> {
    let value = value.trim();
    let utc = value.ends_with('Z');
    let v = value.trim_end_matches('Z');
    if let Ok(d) = NaiveDate::parse_from_str(v, "%Y%m%d") {
        return Some((d.to_string(), true));
    }
    let moment = NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
    if let Some(z) = zone.and_then(crate::zones::parse_zone).filter(|_| !utc) {
        return Some((crate::zones::local_to_utc(moment, z).format("%Y-%m-%dT%H:%M:%SZ").to_string(), false));
    }
    Some((format!("{}{}", moment.format("%Y-%m-%dT%H:%M:%S"), if utc { "Z" } else { "" }), false))
}

/// The `TZID=...` parameter of a property line's head (`DTSTART;TZID=Europe/Oslo`).
fn tzid(head: &str) -> Option<&str> {
    head.split(';').skip(1).find_map(|p| p.strip_prefix("TZID=").or_else(|| p.strip_prefix("tzid=")))
}

pub fn parse_events(source: &str) -> Vec<Parsed> {
    let mut out = Vec::new();
    let mut current: Option<Parsed> = None;
    for line in unfold(source) {
        let line = line.trim_end();
        match line {
            "BEGIN:VEVENT" => current = Some(Parsed { repeat: "none".into(), ..Default::default() }),
            "END:VEVENT" => {
                if let Some(e) = current.take() {
                    out.push(e);
                }
            }
            _ => {
                let Some(event) = current.as_mut() else { continue };
                let Some((head, value)) = line.split_once(':') else { continue };
                let name = head.split(';').next().unwrap_or_default().to_ascii_uppercase();
                match name.as_str() {
                    "SUMMARY" => event.title = unescape(value),
                    "DESCRIPTION" => event.description = unescape(value),
                    "LOCATION" => event.location = unescape(value),
                    "DTSTART" => {
                        if let Some((t, all_day)) = moment_in(value, tzid(head)) {
                            event.start = Some(t);
                            event.all_day = all_day;
                        }
                    }
                    "DTEND" => {
                        if let Some((t, all_day)) = moment_in(value, tzid(head)) {
                            // A date-only end is the day after the last day.
                            event.end = Some(if all_day { NaiveDate::parse_from_str(&t, "%Y-%m-%d").map(|d| (d - dates::Duration::days(1)).to_string()).unwrap_or(t) } else { t });
                        }
                    }
                    "RRULE" => {
                        let parts: Vec<(&str, &str)> = value.split(';').filter_map(|p| p.split_once('=')).collect();
                        let get = |k: &str| parts.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| *v);
                        event.repeat = match get("FREQ").map(str::to_ascii_uppercase).as_deref() {
                            Some("DAILY") => "daily",
                            Some("WEEKLY") => "weekly",
                            Some("MONTHLY") => "monthly",
                            Some("YEARLY") => "yearly",
                            _ => "none",
                        }
                        .to_string();
                        event.until = get("UNTIL").and_then(moment_text).map(|(t, _)| t[..10].to_string());
                        let plain = get("INTERVAL").is_none_or(|i| i == "1") && get("COUNT").is_none() && get("BYDAY").is_none() && get("BYMONTHDAY").is_none() && get("BYSETPOS").is_none() && get("BYMONTH").is_none();
                        if event.repeat == "none" || !plain {
                            event.rule_ignored = true;
                            if !plain {
                                event.repeat = "none".into();
                                event.until = None;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

pub fn export_ics(input: CalendarOnly) -> Result<Value> {
    let (calendar, _, _) = require(&input.calendar, Role::Viewer)?;
    let id = key(id_text(&calendar));
    let events: Vec<Record> = db::find("cal_event").filter("calendar", id).order_by("starts_at").limit(20000).all()?;
    let mut skipped = Vec::new();
    for e in events.iter().filter(|e| text(e, "repeat").unwrap_or("none") != "none") {
        for s in db::find::<Record>("cal_skip").filter("event", key(id_text(e))).limit(2000).all()? {
            if let Some(d) = text(&s, "date").and_then(|d| dates::parse_date(d).ok()) {
                skipped.push((key(id_text(e)).to_string(), d));
            }
        }
    }
    let now = parse(&now()?)?;
    let name = text(&calendar, "name").unwrap_or("Calendar");
    Ok(json!({ "id": id, "name": name, "events": events.len(), "ics": render(name, &events, &skipped, now) }))
}

#[derive(Deserialize)]
pub struct ImportIcs {
    calendar: String,
    ics: String,
}

pub fn import_ics(input: ImportIcs) -> Result<Value> {
    let (calendar, _, me) = require(&input.calendar, Role::Editor)?;
    if input.ics.len() > MAX_BYTES {
        return Err(Error::msg("a file to import is at most 1 MB"));
    }
    let found = parse_events(&input.ics);
    if found.is_empty() {
        return Err(Error::msg("there are no events in that file"));
    }
    if found.len() > MAX_IMPORT {
        return Err(Error::msg(format!("a file to import holds at most {MAX_IMPORT} events")));
    }
    let viewer = crate::zones::Viewer::of(&me)?;
    let (mut imported, mut skipped, mut rules) = (0, 0, 0);
    for p in &found {
        let (Some(start), true) = (p.start.as_deref(), !p.title.trim().is_empty()) else {
            skipped += 1;
            continue;
        };
        let Ok((s, e)) = span(&viewer, start, p.end.as_deref(), p.all_day) else {
            skipped += 1;
            continue;
        };
        db::create::<Record>(
            "cal_event",
            &json!({
                "calendar": key(id_text(&calendar)), "title": p.title.chars().take(200).collect::<String>(), "description": p.description, "location": p.location.chars().take(200).collect::<String>(),
                "starts_at": dates::format_datetime(s), "ends_at": dates::format_datetime(e), "all_day": p.all_day, "repeat": p.repeat, "until": p.until,
                "attendees": [], "attendee_keys": "", "attendee_names": "", "created_by": me, "tz": viewer.name(),
            }),
        )?;
        imported += 1;
        rules += usize::from(p.rule_ignored);
    }
    Ok(json!({ "imported": imported, "skipped": skipped, "rules_simplified": rules }))
}

#[allow(dead_code)]
fn midnight() -> NaiveTime {
    NaiveTime::MIN
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:1\r\nDTSTART:20261020T090000Z\r\nDTEND:20261020T100000Z\r\nSUMMARY:Stand-up\\, daily\r\nRRULE:FREQ=WEEKLY;UNTIL=20261231T000000Z\r\nLOCATION:Room 4\r\nDESCRIPTION:Line one\\nline\r\n  two\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART;VALUE=DATE:20261101\r\nDTEND;VALUE=DATE:20261103\r\nSUMMARY:Offsite\r\nRRULE:FREQ=WEEKLY;INTERVAL=2\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n";

    #[test]
    fn events_are_read_from_a_file() {
        let events = parse_events(SAMPLE);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "Stand-up, daily");
        assert_eq!((events[0].repeat.as_str(), events[0].until.as_deref()), ("weekly", Some("2026-12-31")));
        assert_eq!(events[0].description, "Line one\nline two");
        assert_eq!(events[0].start.as_deref(), Some("2026-10-20T09:00:00Z"));
        assert!(events[1].all_day && events[1].rule_ignored);
        // The end of a whole-day event is the day after its last day.
        assert_eq!(events[1].end.as_deref(), Some("2026-11-02"));
        assert_eq!(events[1].repeat, "none");
    }

    #[test]
    fn a_named_zone_in_a_file_is_applied() {
        let file = "BEGIN:VEVENT\r\nDTSTART;TZID=Europe/Oslo:20260701T090000\r\nDTEND;TZID=Europe/Oslo:20260701T100000\r\nSUMMARY:Local\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nDTSTART:20260701T090000\r\nSUMMARY:Floating\r\nEND:VEVENT\r\n";
        let events = parse_events(file);
        assert_eq!(events[0].start.as_deref(), Some("2026-07-01T07:00:00Z"));
        assert_eq!(events[0].end.as_deref(), Some("2026-07-01T08:00:00Z"));
        assert_eq!(events[1].start.as_deref(), Some("2026-07-01T09:00:00"));
    }

    #[test]
    fn a_calendar_goes_out_and_comes_back() {
        let event = json!({
            "id": "cal_event:abc", "title": "Review; part 2", "location": "HQ", "starts_at": "2026-10-20T09:00:00Z", "ends_at": "2026-10-20T10:00:00Z",
            "all_day": false, "repeat": "weekly", "until": "2026-12-31",
        });
        let text = render("Team", &[event], &[("abc".into(), NaiveDate::from_ymd_opt(2026, 10, 27).unwrap())], parse("2026-10-18T00:00:00Z").unwrap());
        assert!(text.starts_with("BEGIN:VCALENDAR\r\n") && text.contains("RRULE:FREQ=WEEKLY;UNTIL=20261231T235959Z") && text.contains("EXDATE:20261027T090000Z"));
        let back = parse_events(&text);
        assert_eq!(back.len(), 1);
        assert_eq!((back[0].title.as_str(), back[0].location.as_str(), back[0].repeat.as_str()), ("Review; part 2", "HQ", "weekly"));
    }

    #[test]
    fn long_lines_are_folded_and_text_is_escaped() {
        let long = format!("SUMMARY:{}", "x".repeat(200));
        assert!(fold(&long).split("\r\n").all(|l| l.chars().count() <= 75));
        assert_eq!(unescape(&escape("a,b;c\nd\\e")), "a,b;c\nd\\e");
    }

    #[test]
    fn a_file_without_events_gives_nothing() {
        assert!(parse_events("BEGIN:VCALENDAR\nEND:VCALENDAR").is_empty());
    }
}
