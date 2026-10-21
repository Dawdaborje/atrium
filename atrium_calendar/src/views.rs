//! What the pages show: the calendars, a month, the agenda, who is free, and an event for the form.

use aether_sdk::dates::{Duration, NaiveDateTime};
use aether_sdk::prelude::*;

use crate::access::*;
use crate::calendars::CalendarOnly;
use crate::common::*;
use crate::events::{conflicts, event_for, EventOnly};
use crate::schedule::{answers_of, events_for, is_attendee, parse, stamp, Item, Scope};
use crate::zones::{day_bounds, Viewer};

const DEFAULT_COLOR: &str = "#2563eb";

fn matches(needle: &str, name: &str) -> bool {
    needle.is_empty() || name.to_lowercase().contains(needle)
}

/// "Tue Oct 20, 09:00 – 10:00", "Tue Oct 20 – Thu Oct 22 · all day".
pub fn when_text(start: NaiveDateTime, end: NaiveDateTime, all_day: bool) -> String {
    if all_day {
        return if start.date() == end.date() { format!("{} · all day", start.format("%a %b %-d")) } else { format!("{} – {} · all day", start.format("%a %b %-d"), end.format("%a %b %-d")) };
    }
    if start.date() == end.date() {
        format!("{}, {} – {}", start.format("%a %b %-d"), start.format("%H:%M"), end.format("%H:%M"))
    } else {
        format!("{} – {}", start.format("%a %b %-d, %H:%M"), end.format("%a %b %-d, %H:%M"))
    }
}

/// A moment of an event as text for a page: whole days are floating dates (no `Z` for a person with a zone), moments are
/// shown in the person's zone.
pub fn show(viewer: &Viewer, moment: NaiveDateTime, all_day: bool) -> String {
    match (all_day, viewer.zone()) {
        (true, Some(_)) => stamp(moment).trim_end_matches('Z').to_string(),
        (true, None) => stamp(moment),
        (false, _) => viewer.show(moment),
    }
}

/// The wall-clock time a person reads for a moment of an event.
fn clock(viewer: &Viewer, moment: NaiveDateTime, all_day: bool) -> NaiveDateTime {
    if all_day { moment } else { viewer.clock(moment) }
}

fn item_row(item: &Item, calendars: &[Record], viewer: &Viewer) -> Value {
    let e = &item.event;
    let calendar = calendars.iter().find(|c| key(id_text(c)) == text(e, "calendar").unwrap_or_default());
    let all_day = flag(e, "all_day");
    let (start, end) = (clock(viewer, item.start, all_day), clock(viewer, item.end, all_day));
    json!({
        "id": key(id_text(e)), "title": text(e, "title"), "starts_at": show(viewer, item.start, all_day), "ends_at": show(viewer, item.end, all_day), "all_day": all_day,
        "day": start.date().to_string(), "when_text": when_text(start, end, all_day), "location": text(e, "location"),
        "calendar": text(e, "calendar"), "calendar_name": calendar.and_then(|c| text(c, "name")),
        "color": calendar.and_then(|c| text(c, "color")).filter(|c| !c.is_empty()).unwrap_or(DEFAULT_COLOR), "repeat": text(e, "repeat"),
        "attendee_names": text(e, "attendee_names"), "response": "",
    })
}

fn rows(user: &str, items: &[Item]) -> Result<Vec<Value>> {
    let calendars: Vec<Record> = db::find("cal_calendar").limit(5000).all()?;
    let answers = answers_of(user)?;
    let viewer = Viewer::of(user)?;
    Ok(items
        .iter()
        .map(|i| {
            let mut row = item_row(i, &calendars, &viewer);
            if is_attendee(&i.event, user) {
                row["response"] = json!(answers.get(key(id_text(&i.event))).map(String::as_str).unwrap_or("needs_action"));
            }
            row
        })
        .collect())
}

#[derive(Deserialize, Default)]
pub struct Window {
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    end: Option<String>,
    #[serde(default)]
    calendar: Option<String>,
}

/// The month grid's events: the occurrences in the window it asks for.
pub fn events_in(input: Window) -> Result<Vec<Value>> {
    let me = me()?;
    ensure_personal(&me)?;
    let now = parse(&crate::common::now()?)?;
    let mut from = input.start.as_deref().map(parse).transpose()?.unwrap_or(now);
    let mut to = input.end.as_deref().map(parse).transpose()?.unwrap_or(from + Duration::days(35));
    // The grid's edges are the browser's midnights; a person in another zone also sees what is a few hours beyond them.
    if Viewer::of(&me)?.zone().is_some() {
        from -= Duration::days(1);
        to += Duration::days(1);
    }
    if to < from || to - from > Duration::days(400) {
        return Err(Error::msg("the window is at most 400 days"));
    }
    let only = input.calendar.as_deref().filter(|c| !c.is_empty());
    if let Some(c) = only {
        require(c, Role::Viewer)?;
    }
    let items = events_for(&me, from, to, only, Scope::Everything)?;
    rows(&me, &items.into_iter().take(2000).collect::<Vec<_>>())
}

/// The next two weeks, a day at a time.
pub fn agenda(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    ensure_personal(&me)?;
    let now = parse(&crate::common::now()?)?;
    // Today starts at the person's own midnight.
    let (from, _) = day_bounds(&Viewer::of(&me)?, now);
    let items = events_for(&me, from, from + Duration::days(14), None, Scope::Everything)?;
    rows(&me, &items.into_iter().take(500).collect::<Vec<_>>())
}

pub fn my_calendars(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    ensure_personal(&me)?;
    let mut out = Vec::new();
    for id in my_calendar_keys(&me)? {
        let Some(c) = db::get::<Record>("cal_calendar", &id)? else { continue };
        out.push(json!({
            "id": id, "name": text(&c, "name"), "description": text(&c, "description"), "color": text(&c, "color"), "personal": flag(&c, "personal"),
            "role": role_of(&id, &me)?.map(role_name), "people": members_of(&id)?.len(),
            "events": db::count("cal_event", aether_sdk::db::Filter::eq("calendar", id.as_str()))?,
        }));
    }
    out.sort_by_key(|r| (!r["personal"].as_bool().unwrap_or(false), r["name"].as_str().unwrap_or_default().to_lowercase()));
    Ok(out)
}

pub fn calendar_header(input: CalendarOnly) -> Result<Value> {
    let (calendar, role, _) = require(&input.calendar, Role::Viewer)?;
    Ok(json!({ "title": text(&calendar, "name"), "subtitle": format!("{} people · you are {}", members_of(id_text(&calendar))?.len(), role_name(role)) }))
}

pub fn calendar_info(input: CalendarOnly) -> Result<Value> {
    let (calendar, role, _) = require(&input.calendar, Role::Viewer)?;
    Ok(json!({ "id": key(id_text(&calendar)), "name": text(&calendar, "name"), "description": text(&calendar, "description"), "color": text(&calendar, "color"), "personal": flag(&calendar, "personal"), "role": role_name(role) }))
}

/// A stored moment as the form reads it.
fn shown(viewer: &Viewer, stored: Option<&str>, all_day: bool) -> Value {
    match stored.and_then(|t| parse(t).ok()) {
        Some(moment) => json!(show(viewer, moment, all_day)),
        None => json!(stored),
    }
}

/// An event as the form edits it.
pub fn get_event(input: EventOnly) -> Result<Value> {
    let (event, role, me) = event_for(&input.id, Role::Viewer)?;
    let answers = answers_of(&me)?;
    let calendar = db::get::<Record>("cal_calendar", text(&event, "calendar").unwrap_or_default())?;
    let responses: Vec<Record> = db::find("cal_response").filter("event", key(id_text(&event))).limit(500).all()?;
    let people = org_people()?;
    let summary: Vec<String> = strings(event.get("attendees"))
        .iter()
        .map(|u| {
            let status = responses.iter().find(|r| text(r, "user").is_some_and(|x| key(x) == key(u))).and_then(|r| text(r, "status")).unwrap_or("needs_action");
            format!("{}: {}", name_of(&people, u), status.replace('_', " "))
        })
        .collect();
    let busy = conflicts(&event)?;
    let viewer = Viewer::of(&me)?;
    Ok(json!({
        "id": key(id_text(&event)), "calendar": text(&event, "calendar"), "calendar_name": calendar.as_ref().and_then(|c| text(c, "name")), "title": text(&event, "title"),
        "description": text(&event, "description"), "location": text(&event, "location"),
        "starts_at": shown(&viewer, text(&event, "starts_at"), flag(&event, "all_day")), "ends_at": shown(&viewer, text(&event, "ends_at"), flag(&event, "all_day")),
        "time_zone": viewer.name(),
        "all_day": flag(&event, "all_day"), "repeat": text(&event, "repeat"), "until": text(&event, "until"), "attendees": event.get("attendees"),
        "my_response": if is_attendee(&event, &me) { answers.get(key(id_text(&event))).cloned().unwrap_or_else(|| "needs_action".to_string()) } else { String::new() },
        "responses": summary.join(" · "), "busy": if busy.is_empty() { String::new() } else { format!("Busy at that time: {}", busy.join(", ")) },
        "can_edit": role.is_some_and(|r| r >= Role::Editor), "created_by": text(&event, "created_by"),
    }))
}

#[derive(Deserialize, Default)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
}

fn needle(input: &Pick) -> String {
    input.q.clone().unwrap_or_default().trim().to_lowercase()
}

/// The calendars where the caller may add events.
pub fn pick_calendars(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    ensure_personal(&me)?;
    let needle = needle(&input);
    let mut out = Vec::new();
    for id in my_calendar_keys(&me)? {
        if role_of(&id, &me)?.is_some_and(|r| r >= Role::Editor) {
            if let Some(c) = db::get::<Record>("cal_calendar", &id)? {
                if matches(&needle, text(&c, "name").unwrap_or_default()) {
                    out.push(json!({ "id": id, "name": text(&c, "name") }));
                }
            }
        }
    }
    Ok(out)
}

/// Everyone in the organization, the caller included (a person may invite themselves to a meeting).
pub fn pick_org_people(input: Pick) -> Result<Vec<Value>> {
    let needle = needle(&input);
    Ok(org_people()?.into_iter().filter(|p| matches(&needle, &p.name)).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}

/// Everyone in the organization except the caller, to share a calendar with.
pub fn pick_other_people(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = needle(&input);
    Ok(org_people()?.into_iter().filter(|p| key(&p.id) != key(&me) && matches(&needle, &p.name)).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}

/// Who is busy now and when each person is next busy: a view of availability, never of what the events are.
pub fn availability(_input: Empty) -> Result<Vec<Value>> {
    let now = parse(&crate::common::now()?)?;
    let viewer = Viewer::me()?;
    let mut rows = Vec::new();
    for person in org_people()?.into_iter().take(200) {
        let items: Vec<Item> = events_for(&person.id, now, now + Duration::days(7), None, Scope::Busy)?.into_iter().filter(|i| !flag(&i.event, "all_day")).collect();
        let current = items.iter().find(|i| i.start <= now && i.end > now);
        let next = items.iter().find(|i| i.start > now);
        rows.push(json!({
            "id": person.id, "name": person.name,
            "status": if current.is_some() { "busy" } else { "free" },
            "until": current.map(|i| format!("until {}", viewer.clock(i.end).format("%a %H:%M"))).unwrap_or_default(),
            "next_busy": next.map(|i| viewer.clock(i.start).format("%a %b %-d, %H:%M").to_string()).unwrap_or_else(|| "nothing this week".to_string()),
        }));
    }
    rows.sort_by_key(|r| (r["status"] == "busy", r["name"].as_str().unwrap_or_default().to_lowercase()));
    Ok(rows)
}

/// For record rules: the keys of the calendars the caller is in, so members read the conversations of their events.
pub fn rule_my_calendars() -> Result<Vec<String>> {
    match context::current()?.actor.id {
        Some(me) => my_calendar_keys(&me),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        parse(text).unwrap()
    }

    #[test]
    fn times_read_naturally() {
        assert_eq!(when_text(at("2026-10-20T09:00:00Z"), at("2026-10-20T10:30:00Z"), false), "Tue Oct 20, 09:00 – 10:30");
        assert_eq!(when_text(at("2026-10-20T00:00:00Z"), at("2026-10-20T23:59:59Z"), true), "Tue Oct 20 · all day");
        assert_eq!(when_text(at("2026-10-20T00:00:00Z"), at("2026-10-22T23:59:59Z"), true), "Tue Oct 20 – Thu Oct 22 · all day");
        assert!(when_text(at("2026-10-20T22:00:00Z"), at("2026-10-21T02:00:00Z"), false).contains("Oct 21, 02:00"));
    }

    #[test]
    fn pickers_filter_by_a_lowercase_needle() {
        assert!(matches("", "Anything") && matches("ann", "Ann Lee") && !matches("bob", "Ann Lee"));
    }
}
