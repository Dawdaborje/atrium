//! What is in a person's calendar between two moments: every event of the calendars they are in, and the events
//! they are invited to, with repeating events spread out into their occurrences.

use std::collections::HashMap;

use aether_sdk::dates::{self, Duration, NaiveDate, NaiveDateTime};
use aether_sdk::{db::Filter, prelude::*};

use crate::access::my_calendar_keys;
use crate::common::*;
use crate::recur::{occurrences, Occurrence};
use crate::zones::{local_to_utc, parse_zone, utc_to_local};

/// One occurrence of an event.
#[derive(Debug, Clone)]
pub struct Item {
    pub event: Record,
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Everything on the calendars the person is in, and what they are invited to.
    Everything,
    /// Only what takes the person's own time: their personal calendar and what they accepted or have not answered.
    Busy,
}

pub fn parse(text: &str) -> Result<NaiveDateTime> {
    dates::parse_datetime(text)
}

pub fn stamp(moment: NaiveDateTime) -> String {
    dates::format_datetime(moment)
}

fn skips() -> Result<HashMap<String, Vec<NaiveDate>>> {
    let rows: Vec<Record> = db::find("cal_skip").limit(20000).all()?;
    let mut out: HashMap<String, Vec<NaiveDate>> = HashMap::new();
    for row in rows {
        if let (Some(event), Some(date)) = (text(&row, "event"), text(&row, "date").and_then(|d| dates::parse_date(d).ok())) {
            out.entry(event.to_string()).or_default().push(date);
        }
    }
    Ok(out)
}

/// The answers a person gave, by event key.
pub fn answers_of(user: &str) -> Result<HashMap<String, String>> {
    let rows: Vec<Record> = db::find("cal_response").filter("user", user).limit(5000).all()?;
    Ok(rows.iter().filter_map(|r| Some((key(text(r, "event")?).to_string(), text(r, "status")?.to_string()))).collect())
}

pub fn expand(event: &Record, skipped: &[NaiveDate], from: NaiveDateTime, to: NaiveDateTime) -> Vec<Item> {
    let Some(first) = text(event, "starts_at").and_then(|t| parse(t).ok()) else { return Vec::new() };
    let last = text(event, "ends_at").and_then(|t| parse(t).ok()).unwrap_or(first);
    let until = text(event, "until").and_then(|d| dates::parse_date(d).ok());
    let repeat = text(event, "repeat").unwrap_or("none");
    // A repeating event keeps its wall-clock time in the zone it was made in, so it stays at nine o'clock through
    // the clock changes: it is spread out in that zone and each occurrence is turned back into UTC.
    let zone = if repeat == "none" || flag(event, "all_day") { None } else { text(event, "tz").and_then(parse_zone).filter(|z| z.name() != "UTC") };
    match zone {
        None => occurrences(first, last, repeat, until, skipped, from, to).into_iter().map(|Occurrence { start, end }| Item { event: event.clone(), start, end }).collect(),
        Some(z) => occurrences(utc_to_local(first, z), utc_to_local(last, z), repeat, until, skipped, utc_to_local(from, z) - Duration::days(1), utc_to_local(to, z) + Duration::days(1))
            .into_iter()
            .map(|Occurrence { start, end }| Item { event: event.clone(), start: local_to_utc(start, z), end: local_to_utc(end, z) })
            .filter(|i| i.start <= to && i.end >= from)
            .collect(),
    }
}

pub fn is_attendee(event: &Record, user: &str) -> bool {
    strings(event.get("attendees")).iter().any(|a| key(a) == key(user))
}

/// The occurrences between `from` and `to` for one person, in time order. `only` limits it to one calendar.
pub fn events_for(user: &str, from: NaiveDateTime, to: NaiveDateTime, only: Option<&str>, scope: Scope) -> Result<Vec<Item>> {
    let mine = my_calendar_keys(user)?;
    let calendars: Vec<Record> = db::find("cal_calendar").limit(5000).all()?;
    let personal: Vec<String> = calendars.iter().filter(|c| flag(c, "personal") && text(c, "owner").is_some_and(|o| key(o) == key(user))).map(|c| key(id_text(c)).to_string()).collect();
    let mut events: Vec<Record> = Vec::new();
    let wanted: Vec<&String> = match only {
        Some(c) => mine.iter().filter(|m| key(m) == key(c)).collect(),
        None => mine.iter().collect(),
    };
    for calendar in wanted {
        events.extend(db::find::<Record>("cal_event").filter("calendar", calendar.as_str()).limit(10000).all()?);
    }
    if only.is_none() {
        let invited: Vec<Record> = db::find("cal_event").matching(Filter::like("attendee_keys", &format!(",{},", key(user)))).limit(10000).all()?;
        for e in invited {
            if !events.iter().any(|x| id_text(x) == id_text(&e)) {
                events.push(e);
            }
        }
    }
    let answers = answers_of(user)?;
    let skipped = skips()?;
    let mut out = Vec::new();
    for event in &events {
        let id = key(id_text(event));
        let answer = answers.get(id).map(String::as_str).unwrap_or("needs_action");
        let invited = is_attendee(event, user);
        if invited && answer == "declined" {
            continue;
        }
        if scope == Scope::Busy && !invited && !personal.iter().any(|p| p == text(event, "calendar").unwrap_or_default()) {
            continue;
        }
        out.extend(expand(event, skipped.get(id).map(Vec::as_slice).unwrap_or(&[]), from, to));
    }
    out.sort_by_key(|i| (i.start, i.end));
    Ok(out)
}

/// A person is busy when any of their occurrences overlaps the interval.
pub fn busy_during(user: &str, from: NaiveDateTime, to: NaiveDateTime, except_event: Option<&str>) -> Result<bool> {
    Ok(events_for(user, from, to, None, Scope::Busy)?
        .iter()
        .any(|i| i.start < to && i.end > from && !except_event.is_some_and(|e| key(id_text(&i.event)) == key(e)) && !flag(&i.event, "all_day")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_event_without_a_start_has_no_occurrences() {
        let at = parse("2026-10-18T00:00:00Z").unwrap();
        assert!(expand(&json!({ "title": "x" }), &[], at, at).is_empty());
    }

    #[test]
    fn a_repeating_event_keeps_its_local_time_through_the_clock_change() {
        // Weekly at 09:00 in Oslo, first on Oct 20 (UTC+2): after the clocks go back on Oct 25 it is 08:00 UTC.
        let event = json!({ "starts_at": "2026-10-20T07:00:00Z", "ends_at": "2026-10-20T08:00:00Z", "repeat": "weekly", "tz": "Europe/Oslo", "all_day": false });
        let found = expand(&event, &[], parse("2026-10-19T00:00:00Z").unwrap(), parse("2026-11-05T00:00:00Z").unwrap());
        let starts: Vec<String> = found.iter().map(|i| stamp(i.start)).collect();
        assert_eq!(starts, ["2026-10-20T07:00:00Z", "2026-10-27T08:00:00Z", "2026-11-03T08:00:00Z"]);
        let utc_event = json!({ "starts_at": "2026-10-20T07:00:00Z", "ends_at": "2026-10-20T08:00:00Z", "repeat": "weekly", "all_day": false });
        let same: Vec<String> = expand(&utc_event, &[], parse("2026-10-19T00:00:00Z").unwrap(), parse("2026-11-05T00:00:00Z").unwrap()).iter().map(|i| stamp(i.start)).collect();
        assert_eq!(same, ["2026-10-20T07:00:00Z", "2026-10-27T07:00:00Z", "2026-11-03T07:00:00Z"]);
    }

    #[test]
    fn invited_people_are_found_by_key() {
        assert!(is_attendee(&json!({ "attendees": ["user:a", "user:b"] }), "user:b"));
        assert!(!is_attendee(&json!({ "attendees": ["user:a"] }), "user:c"));
        assert!(!is_attendee(&json!({}), "user:a"));
    }
}
