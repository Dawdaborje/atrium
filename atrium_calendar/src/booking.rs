//! What the booking plugin needs from the calendar: when a person is busy (never why), and a way to put a
//! booked meeting into the host's calendar and take it out again.

use aether_sdk::prelude::*;

use crate::access::*;
use crate::calendars::{check_users, notify};
use crate::common::*;
use crate::events::{conflicts, norm_datetime, purge_event};
use crate::schedule::{busy_during, events_for, stamp, Scope};
use crate::zones::Viewer;

#[derive(Deserialize)]
pub struct BusyIn {
    user: String,
    start: String,
    end: String,
}

/// The times a person is busy between two moments, merged by the caller as it likes. Titles never leave the calendar.
pub fn busy_of(input: BusyIn) -> Result<Vec<Value>> {
    me()?;
    let users = check_users(&[input.user])?;
    let from = norm_datetime(&Viewer::utc(), "the start", &input.start)?;
    let to = norm_datetime(&Viewer::utc(), "the end", &input.end)?;
    if to <= from || (to - from).num_days() > 62 {
        return Err(Error::msg("ask for at most two months, ending after the start"));
    }
    Ok(events_for(&users[0], from, to, None, Scope::Busy)?
        .into_iter()
        .filter(|i| !flag(&i.event, "all_day"))
        .map(|i| json!({ "start": stamp(i.start), "end": stamp(i.end) }))
        .collect())
}

#[derive(Deserialize)]
pub struct Book {
    host: String,
    title: String,
    starts_at: String,
    ends_at: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
}

/// A meeting in the host's own calendar with the caller invited (and answered "accepted"). Refused when the host is
/// busy then. A person cannot book themselves.
pub fn book_slot(input: Book) -> Result<Value> {
    let me = me()?;
    let host = check_users(&[input.host])?.remove(0);
    if key(&host) == key(&me) {
        return Err(Error::msg("you cannot book time with yourself"));
    }
    ensure_personal(&host)?;
    let calendar = crate::events::personal_calendar(&host)?;
    let start = norm_datetime(&Viewer::utc(), "the start", &input.starts_at)?;
    let end = norm_datetime(&Viewer::utc(), "the end", &input.ends_at)?;
    if end <= start {
        return Err(Error::msg("a meeting cannot end before it starts"));
    }
    if busy_during(&host, start, end, None)? {
        return Err(Error::msg("that time is no longer free"));
    }
    let people = org_people()?;
    let event: Record = db::create(
        "cal_event",
        &json!({
            "calendar": key(id_text(&calendar)), "title": clean("a title", &input.title, 200)?, "description": input.description.as_deref().map(str::trim),
            "location": input.location.as_deref().map(str::trim), "starts_at": stamp(start), "ends_at": stamp(end), "all_day": false, "repeat": "none",
            "attendees": [me.clone()], "attendee_keys": keys_text(&[key(&me).to_string()]), "attendee_names": name_of(&people, &me), "created_by": me,
            "tz": Viewer::of(&host)?.name(),
        }),
    )?;
    let id = key(id_text(&event)).to_string();
    crate::events::answer_event(&event, &me, "accepted")?;
    notify(&host, &format!("{} booked “{}”", name_of(&people, &me), text(&event, "title").unwrap_or("a meeting")), &format!("/calendar/event/{id}"))?;
    let _ = conflicts(&event);
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct Cancel {
    event: String,
}

/// Takes a booked meeting out of the calendar. Only the person who booked it or the owner of its calendar can.
pub fn cancel_booked(input: Cancel) -> Result<Value> {
    let me = me()?;
    let Some(event) = db::get::<Record>("cal_event", key(&input.event))? else { return Ok(json!({ "deleted": false })) };
    let owner = role_of(text(&event, "calendar").unwrap_or_default(), &me)? == Some(Role::Owner);
    if !owner && !text(&event, "created_by").is_some_and(|c| key(c) == key(&me)) {
        return Err(Error::msg("only who booked it or the host can cancel this"));
    }
    purge_event(&event)?;
    Ok(json!({ "deleted": true }))
}
