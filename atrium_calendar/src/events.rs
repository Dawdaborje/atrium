//! Events: making, changing, inviting people, answering an invitation, deleting.

use aether_sdk::dates::{self, NaiveDate, NaiveDateTime, NaiveTime};
use aether_sdk::prelude::*;

use crate::access::*;
use crate::calendars::{check_users, notify};
use crate::common::*;
use crate::recur::REPEATS;
use crate::schedule::{busy_during, is_attendee, parse, stamp};
use crate::zones::Viewer;

const MAX_ATTENDEES: usize = 100;
pub const ANSWERS: [&str; 4] = ["needs_action", "accepted", "tentative", "declined"];

/// A moment as typed on a page or read from a file: RFC 3339, `2026-10-20T09:00`, `2026-10-20 09:00`, or a date alone
/// (midnight). A time with a `Z` or an offset is as it says; one without is in the viewer's zone (UTC when they have none).
pub fn norm_datetime(viewer: &Viewer, label: &str, value: &str) -> Result<NaiveDateTime> {
    let v = value.trim();
    if let Ok(moment) = parse(v) {
        return Ok(moment);
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(moment) = NaiveDateTime::parse_from_str(v, format) {
            return Ok(viewer.to_utc(moment));
        }
    }
    dates::parse_date(v).map(|d| viewer.to_utc(d.and_time(NaiveTime::MIN))).map_err(|_| Error::msg(format!("{label} is a date and time like 2026-10-20T09:00")))
}

/// The start and end to store. An all-day event runs from midnight on its first day to the last second of its last day;
/// without an end, an event lasts an hour.
pub fn span(viewer: &Viewer, starts: &str, ends: Option<&str>, all_day: bool) -> Result<(NaiveDateTime, NaiveDateTime)> {
    // Whole days are dates, not moments: no zone moves them.
    let viewer = if all_day { Viewer::utc() } else { *viewer };
    let start = norm_datetime(&viewer, "the start", starts)?;
    let end = ends.map(str::trim).filter(|e| !e.is_empty()).map(|e| norm_datetime(&viewer, "the end", e)).transpose()?;
    if all_day {
        let first = start.date();
        let last = end.map(|e| e.date()).unwrap_or(first);
        if last < first {
            return Err(Error::msg("an event cannot end before it starts"));
        }
        return Ok((first.and_time(NaiveTime::MIN), last.and_hms_opt(23, 59, 59).unwrap_or(last.and_time(NaiveTime::MIN))));
    }
    let end = end.unwrap_or(start + dates::Duration::hours(1));
    if end < start {
        return Err(Error::msg("an event cannot end before it starts"));
    }
    Ok((start, end))
}

fn check_repeat(repeat: &str, until: Option<&str>, start: NaiveDateTime) -> Result<Option<String>> {
    if !REPEATS.contains(&repeat) {
        return Err(Error::msg(format!("a repeat is one of: {}", REPEATS.join(", "))));
    }
    let until = until.map(str::trim).filter(|u| !u.is_empty());
    let Some(until) = until else { return Ok(None) };
    let date: NaiveDate = dates::parse_date(until).map_err(|_| Error::msg("the end of a repeat is a date like 2026-12-31"))?;
    if repeat == "none" {
        return Ok(None);
    }
    if date < start.date() {
        return Err(Error::msg("a repeat cannot end before the event starts"));
    }
    Ok(Some(until.to_string()))
}

/// The event, the caller's role in its calendar (none for someone only invited) and the caller. Changing needs a role
/// of at least `min`; looking is also allowed to the invited.
pub fn event_for(id: &str, min: Role) -> Result<(Record, Option<Role>, String)> {
    let me = me()?;
    let event = db::get::<Record>("cal_event", key(id))?.ok_or_else(|| Error::msg("there is no such event"))?;
    let role = role_of(text(&event, "calendar").unwrap_or_default(), &me)?;
    match role {
        Some(r) if r >= min => Ok((event, role, me)),
        Some(_) => Err(Error::msg("you can only look at this calendar")),
        None if min == Role::Viewer && is_attendee(&event, &me) => Ok((event, None, me)),
        None => Err(Error::msg("there is no such event")),
    }
}

struct Invitees {
    ids: Vec<String>,
    names: String,
    keys: String,
}

fn resolve_attendees(users: &[String]) -> Result<Invitees> {
    if users.len() > MAX_ATTENDEES {
        return Err(Error::msg(format!("an event has at most {MAX_ATTENDEES} attendees")));
    }
    let ids = check_users(users)?;
    let people = org_people()?;
    Ok(Invitees { names: ids.iter().map(|u| name_of(&people, u)).collect::<Vec<_>>().join(", "), keys: keys_text(&ids.iter().map(|u| key(u).to_string()).collect::<Vec<_>>()), ids })
}

fn when_label(start: NaiveDateTime, all_day: bool) -> String {
    if all_day { start.format("%a %b %-d").to_string() } else { start.format("%a %b %-d, %H:%M").to_string() }
}

fn invite(event: &Record, who: &[String], me: &str) -> Result<()> {
    let people = org_people()?;
    let start = text(event, "starts_at").and_then(|t| parse(t).ok());
    for user in who.iter().filter(|u| key(u) != key(me)) {
        // Each person is told the time in their own zone.
        let viewer = Viewer::of(user)?;
        let when = start.map(|s| format!(" · {}", when_label(if flag(event, "all_day") { s } else { viewer.clock(s) }, flag(event, "all_day")))).unwrap_or_default();
        notify(user, &format!("{} invited you to “{}”{when}", name_of(&people, me), text(event, "title").unwrap_or("an event")), &format!("/calendar/event/{}", key(id_text(event))))?;
    }
    Ok(())
}

/// Names of the invited who are busy at the time of the event (an event never conflicts with itself).
pub fn conflicts(event: &Record) -> Result<Vec<String>> {
    let (Some(start), Some(end)) = (text(event, "starts_at").and_then(|t| parse(t).ok()), text(event, "ends_at").and_then(|t| parse(t).ok())) else { return Ok(Vec::new()) };
    if flag(event, "all_day") {
        return Ok(Vec::new());
    }
    let people = org_people()?;
    let mut out = Vec::new();
    for user in strings(event.get("attendees")) {
        if busy_during(&user, start, end, Some(id_text(event)))? {
            out.push(name_of(&people, &user));
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewEvent {
    #[serde(default)]
    calendar: Option<String>,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    starts_at: String,
    #[serde(default)]
    ends_at: Option<String>,
    #[serde(default)]
    all_day: bool,
    #[serde(default)]
    repeat: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    attendees: Vec<String>,
}

pub fn create_event(input: NewEvent) -> Result<Value> {
    let me = me()?;
    ensure_personal(&me)?;
    let calendar = match input.calendar.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => require(c, Role::Editor)?.0,
        None => personal_calendar(&me)?,
    };
    let viewer = Viewer::of(&me)?;
    let (start, end) = span(&viewer, &input.starts_at, input.ends_at.as_deref(), input.all_day)?;
    let repeat = input.repeat.as_deref().unwrap_or("none");
    let until = check_repeat(repeat, input.until.as_deref(), start)?;
    let invitees = resolve_attendees(&input.attendees)?;
    let event: Record = db::create(
        "cal_event",
        &json!({
            "calendar": key(id_text(&calendar)), "title": clean("a title", &input.title, 200)?, "description": input.description.as_deref().map(str::trim),
            "location": input.location.as_deref().map(str::trim), "starts_at": stamp(start), "ends_at": stamp(end), "all_day": input.all_day,
            "repeat": repeat, "until": until, "attendees": invitees.ids, "attendee_keys": invitees.keys, "attendee_names": invitees.names, "created_by": me,
            "tz": viewer.name(),
        }),
    )?;
    invite(&event, &invitees.ids, &me)?;
    events::emit("event_created", &json!({ "event": key(id_text(&event)), "calendar": key(id_text(&calendar)), "attendees": invitees.ids }))?;
    Ok(json!({ "id": key(id_text(&event)), "conflicts": conflicts(&event)? }))
}

pub fn personal_calendar(me: &str) -> Result<Record> {
    for calendar in my_calendar_keys(me)? {
        if let Some(c) = db::get::<Record>("cal_calendar", &calendar)? {
            if flag(&c, "personal") && text(&c, "owner").is_some_and(|o| key(o) == key(me)) {
                return Ok(c);
            }
        }
    }
    Err(Error::msg("you have no calendar of your own yet"))
}

#[derive(Deserialize)]
pub struct EventOnly {
    pub id: String,
}

#[derive(Deserialize)]
pub struct UpdateEvent {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
    #[serde(default)]
    all_day: Option<bool>,
    #[serde(default)]
    repeat: Option<String>,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    attendees: Option<Vec<String>>,
    #[serde(default)]
    calendar: Option<String>,
    /// The caller's own answer to an invitation.
    #[serde(default)]
    my_response: Option<String>,
    /// One day of a repeating event to leave out.
    #[serde(default)]
    skip: Option<String>,
}

pub fn update_event(input: UpdateEvent) -> Result<Value> {
    let (event, role, me) = event_for(&input.id, Role::Viewer)?;
    let id = key(id_text(&event)).to_string();
    if let Some(answer) = input.my_response.as_deref().filter(|a| !a.is_empty() && *a != "needs_action") {
        answer_event(&event, &me, answer)?;
    }
    let changes_event = input.title.is_some() || input.description.is_some() || input.location.is_some() || input.starts_at.is_some() || input.ends_at.is_some() || input.all_day.is_some() || input.repeat.is_some() || input.until.is_some() || input.attendees.is_some() || input.calendar.is_some() || input.skip.as_deref().is_some_and(|s| !s.trim().is_empty());
    if !changes_event {
        return Ok(json!({ "ok": true }));
    }
    if !role.is_some_and(|r| r >= Role::Editor) {
        if input.my_response.is_some() && input.title.is_none() {
            return Ok(json!({ "ok": true }));
        }
        return Err(Error::msg("only people who can change the calendar can change its events"));
    }
    let mut data = json!({});
    if let Some(t) = &input.title {
        data["title"] = json!(clean("a title", t, 200)?);
    }
    if let Some(d) = &input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(l) = &input.location {
        data["location"] = json!(l.trim());
    }
    let viewer = Viewer::of(&me)?;
    let all_day = input.all_day.unwrap_or_else(|| flag(&event, "all_day"));
    let timing_changed = input.starts_at.is_some() || input.ends_at.is_some() || input.all_day.is_some();
    let old_start = text(&event, "starts_at").unwrap_or_default().to_string();
    let mut start = parse(&old_start)?;
    if timing_changed {
        let starts = input.starts_at.clone().filter(|s| !s.trim().is_empty()).unwrap_or(old_start);
        let ends = match &input.ends_at {
            Some(e) => Some(e.clone()),
            None => text(&event, "ends_at").map(str::to_string),
        };
        let (s, e) = span(&viewer, &starts, ends.as_deref(), all_day)?;
        start = s;
        data["starts_at"] = json!(stamp(s));
        data["ends_at"] = json!(stamp(e));
        data["all_day"] = json!(all_day);
    }
    if input.repeat.is_some() || input.until.is_some() || timing_changed {
        let repeat = input.repeat.clone().unwrap_or_else(|| text(&event, "repeat").unwrap_or("none").to_string());
        let until = match &input.until {
            Some(u) => Some(u.clone()),
            None => text(&event, "until").map(str::to_string),
        };
        data["repeat"] = json!(repeat);
        data["until"] = json!(check_repeat(&repeat, until.as_deref(), start)?);
    }
    if let Some(target) = input.calendar.as_deref().filter(|c| !c.trim().is_empty() && key(c) != text(&event, "calendar").unwrap_or_default()) {
        let (moved_to, _, _) = require(target, Role::Editor)?;
        data["calendar"] = json!(key(id_text(&moved_to)));
    }
    let mut newly = Vec::new();
    if let Some(users) = &input.attendees {
        let invitees = resolve_attendees(users)?;
        let before = strings(event.get("attendees"));
        newly = invitees.ids.iter().filter(|u| !before.iter().any(|b| key(b) == key(u))).cloned().collect();
        for gone in before.iter().filter(|b| !invitees.ids.iter().any(|u| key(u) == key(b))) {
            for r in db::find::<Record>("cal_response").filter("event", id.as_str()).filter("user", gone.as_str()).limit(5).all()? {
                db::delete::<Record>("cal_response", key(id_text(&r)))?;
            }
        }
        data["attendees"] = json!(invitees.ids);
        data["attendee_keys"] = json!(invitees.keys);
        data["attendee_names"] = json!(invitees.names);
    }
    if let Some(day) = input.skip.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        skip_day(&event, day)?;
    }
    let updated: Record = db::update("cal_event", &id, &data)?.ok_or_else(|| Error::msg("there is no such event"))?;
    invite(&updated, &newly, &me)?;
    if timing_changed {
        // A new time means the answers no longer stand.
        for r in db::find::<Record>("cal_response").filter("event", id.as_str()).limit(500).all()? {
            db::update::<Record>("cal_response", key(id_text(&r)), &json!({ "status": "needs_action" }))?;
        }
        for user in strings(updated.get("attendees")).iter().filter(|u| key(u) != key(&me) && !newly.contains(u)) {
            let there = Viewer::of(user)?;
            notify(user, &format!("“{}” was moved to {}", text(&updated, "title").unwrap_or("An event"), when_label(if all_day { start } else { there.clock(start) }, all_day)), &format!("/calendar/event/{id}"))?;
        }
    }
    Ok(json!({ "ok": true, "conflicts": conflicts(&updated)? }))
}

fn skip_day(event: &Record, day: &str) -> Result<()> {
    if text(event, "repeat").unwrap_or("none") == "none" {
        return Err(Error::msg("only a repeating event has days to leave out"));
    }
    let date = dates::parse_date(day).map_err(|_| Error::msg("the day to skip is a date like 2026-10-26"))?;
    let id = key(id_text(event)).to_string();
    let formatted = dates::format_date(date);
    let existing: Vec<Record> = db::find("cal_skip").filter("event", id.as_str()).limit(2000).all()?;
    if !existing.iter().any(|s| text(s, "date") == Some(formatted.as_str())) {
        db::create::<Record>("cal_skip", &json!({ "event": id, "date": formatted }))?;
    }
    Ok(())
}

pub fn answer_event(event: &Record, user: &str, answer: &str) -> Result<()> {
    if !ANSWERS.contains(&answer) {
        return Err(Error::msg(format!("an answer is one of: {}", ANSWERS.join(", "))));
    }
    if !is_attendee(event, user) {
        return Err(Error::msg("you are not invited to this event"));
    }
    let id = key(id_text(event)).to_string();
    let existing: Vec<Record> = db::find("cal_response").filter("event", id.as_str()).filter("user", user).limit(5).all()?;
    match existing.first() {
        Some(row) => {
            db::update::<Record>("cal_response", key(id_text(row)), &json!({ "status": answer }))?;
        }
        None => {
            db::create::<Record>("cal_response", &json!({ "event": id, "user": user, "status": answer }))?;
        }
    }
    if let Some(owner) = text(event, "created_by").filter(|o| key(o) != key(user)) {
        let people = org_people()?;
        let verb = match answer {
            "accepted" => "accepted",
            "declined" => "declined",
            "tentative" => "might come to",
            _ => "has not answered",
        };
        notify(owner, &format!("{} {verb} “{}”", name_of(&people, user), text(event, "title").unwrap_or("your event")), &format!("/calendar/event/{id}"))?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct Answer {
    id: String,
    status: String,
}

pub fn respond(input: Answer) -> Result<Value> {
    let (event, _, me) = event_for(&input.id, Role::Viewer)?;
    answer_event(&event, &me, &input.status)?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct SkipDay {
    id: String,
    date: String,
}

pub fn delete_occurrence(input: SkipDay) -> Result<Value> {
    let (event, _, _) = event_for(&input.id, Role::Editor)?;
    skip_day(&event, input.date.trim())?;
    Ok(json!({ "ok": true }))
}

pub fn delete_event(input: EventOnly) -> Result<Value> {
    let (event, _, me) = event_for(&input.id, Role::Editor)?;
    let people = org_people()?;
    for user in strings(event.get("attendees")).iter().filter(|u| key(u) != key(&me)) {
        notify(user, &format!("{} cancelled “{}”", name_of(&people, &me), text(&event, "title").unwrap_or("an event")), "/calendar")?;
    }
    purge_event(&event)?;
    Ok(json!({ "deleted": true }))
}

pub fn purge_event(event: &Record) -> Result<()> {
    let id = key(id_text(event)).to_string();
    for model in ["cal_response", "cal_skip"] {
        for row in db::find::<Record>(model).filter("event", id.as_str()).limit(2000).all()? {
            db::delete::<Record>(model, key(id_text(&row)))?;
        }
    }
    db::delete::<Record>("cal_event", &id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_are_read_in_the_ways_pages_and_files_write_them() {
        let want = parse("2026-10-20T09:00:00Z").unwrap();
        for text in ["2026-10-20T09:00:00Z", "2026-10-20T09:00", "2026-10-20 09:00", "2026-10-20T09:00:00"] {
            assert_eq!(norm_datetime(&Viewer::utc(), "x", text).unwrap(), want, "{text}");
        }
        assert_eq!(norm_datetime(&Viewer::utc(), "x", "2026-10-20").unwrap(), parse("2026-10-20T00:00:00Z").unwrap());
        assert!(norm_datetime(&Viewer::utc(), "x", "tomorrow").is_err());
    }

    #[test]
    fn typed_times_are_in_the_viewers_zone_and_whole_days_stay_put() {
        let oslo = Viewer::in_zone(crate::zones::parse_zone("Europe/Oslo").unwrap());
        assert_eq!(norm_datetime(&oslo, "x", "2026-07-01T09:00").unwrap(), parse("2026-07-01T07:00:00Z").unwrap());
        assert_eq!(norm_datetime(&oslo, "x", "2026-07-01T09:00:00Z").unwrap(), parse("2026-07-01T09:00:00Z").unwrap());
        let (s, e) = span(&oslo, "2026-07-01T00:30", Some("2026-07-02"), true).unwrap();
        assert_eq!((stamp(s), stamp(e)), ("2026-07-01T00:00:00Z".to_string(), "2026-07-02T23:59:59Z".to_string()));
    }

    #[test]
    fn an_event_lasts_an_hour_unless_told_otherwise() {
        let (s, e) = span(&Viewer::utc(), "2026-10-20T09:00", None, false).unwrap();
        assert_eq!(e - s, dates::Duration::hours(1));
        assert!(span(&Viewer::utc(), "2026-10-20T09:00", Some("2026-10-20T08:00"), false).is_err());
    }

    #[test]
    fn an_all_day_event_covers_whole_days() {
        let (s, e) = span(&Viewer::utc(), "2026-10-20T15:00", Some("2026-10-22"), true).unwrap();
        assert_eq!((stamp(s), stamp(e)), ("2026-10-20T00:00:00Z".to_string(), "2026-10-22T23:59:59Z".to_string()));
        assert!(span(&Viewer::utc(), "2026-10-22", Some("2026-10-20"), true).is_err());
    }

    #[test]
    fn a_repeat_is_checked() {
        let start = parse("2026-10-20T09:00:00Z").unwrap();
        assert_eq!(check_repeat("weekly", Some("2026-12-31"), start).unwrap().as_deref(), Some("2026-12-31"));
        assert!(check_repeat("weekly", Some("2026-01-01"), start).is_err());
        assert!(check_repeat("hourly", None, start).is_err());
        assert_eq!(check_repeat("none", Some("2026-12-31"), start).unwrap(), None);
    }
}
