//! Booking links: a way for people outside the organization to book a free time with a member, and nothing more.
//!
//! A member has one link (`cal_link`; its key is the unguessable token in the address). The three public functions
//! answer anyone who holds the token and only about the member it belongs to: when they are busy (never why),
//! putting a meeting on their calendar, and taking one back that a guest booked. The booking plugin builds its public
//! pages on them.

use aether_sdk::dates::Duration;
use aether_sdk::prelude::*;

use crate::access::{ensure_personal, role_of, Role};
use crate::calendars::notify;
use crate::common::*;
use crate::events::{norm_datetime, personal_calendar, purge_event};
use crate::schedule::{busy_during, events_for, stamp, Scope};
use crate::zones::Viewer;

/// What a guest writes is cut to these lengths.
const MAX_NAME: usize = 120;
const MAX_NOTE: usize = 1000;
const MAX_AHEAD_DAYS: i64 = 120;
const MAX_FUTURE_GUEST_EVENTS: usize = 150;
const MAX_PER_GUEST: usize = 3;

/// The caller's link, made the first time it is asked for.
pub fn make_link(_input: Empty) -> Result<Value> {
    let me = me()?;
    let found: Option<Record> = db::find("cal_link").filter("host", me.as_str()).first()?;
    let link = match found {
        Some(l) => l,
        None => db::create("cal_link", &json!({ "host": me }))?,
    };
    Ok(json!({ "token": key(id_text(&link)) }))
}

/// Switch the caller's link off: addresses already shared stop working.
pub fn revoke_link(_input: Empty) -> Result<Value> {
    let me = me()?;
    let mut removed = 0;
    for l in db::find::<Record>("cal_link").filter("host", me.as_str()).limit(10).all()? {
        db::delete::<Record>("cal_link", key(id_text(&l)))?;
        removed += 1;
    }
    Ok(json!({ "removed": removed }))
}

/// The member a token belongs to. A wrong token is "no such link", whatever else is true.
fn host_of(token: &str) -> Result<String> {
    let link = db::get::<Record>("cal_link", key(token.trim()))?.ok_or_else(|| Error::msg("this link does not work (any more)"))?;
    text(&link, "host").filter(|h| !h.is_empty()).map(str::to_string).ok_or_else(|| Error::msg("this link does not work (any more)"))
}

#[derive(Deserialize)]
pub struct BusyIn {
    token: String,
    start: String,
    end: String,
}

/// When the member is busy between two moments, as intervals only.
pub fn link_busy(input: BusyIn) -> Result<Vec<Value>> {
    let host = host_of(&input.token)?;
    let utc = Viewer::utc();
    let from = norm_datetime(&utc, "the start", &input.start)?;
    let to = norm_datetime(&utc, "the end", &input.end)?;
    if to <= from || (to - from).num_days() > 62 {
        return Err(Error::msg("ask for at most two months, ending after the start"));
    }
    Ok(events_for(&host, from, to, None, Scope::Busy)?.into_iter().filter(|i| !flag(&i.event, "all_day")).map(|i| json!({ "start": stamp(i.start), "end": stamp(i.end) })).collect())
}

#[derive(Deserialize)]
pub struct GuestBook {
    token: String,
    title: String,
    starts_at: String,
    ends_at: String,
    guest_name: String,
    guest_email: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    /// The zone the member's hours are in, kept so the meeting is shown where it was meant.
    #[serde(default)]
    tz: Option<String>,
}

/// A meeting a guest booked, in the member's own calendar. Refused when the member is busy then.
pub fn link_book(input: GuestBook) -> Result<Value> {
    let host = host_of(&input.token)?;
    let utc = Viewer::utc();
    let start = norm_datetime(&utc, "the start", &input.starts_at)?;
    let end = norm_datetime(&utc, "the end", &input.ends_at)?;
    if end <= start || end - start > Duration::hours(12) {
        return Err(Error::msg("a meeting ends after it starts and lasts at most twelve hours"));
    }
    if busy_during(&host, start, end, None)? {
        return Err(Error::msg("that time is no longer free"));
    }
    let today = crate::schedule::parse(&now()?)?;
    if start < today || start > today + Duration::days(MAX_AHEAD_DAYS) {
        return Err(Error::msg("that time is not open for booking"));
    }
    ensure_personal(&host)?;
    let calendar = personal_calendar(&host)?;
    // The link is open to anyone who has it: a guest can hold only a few future meetings, and a member only so many.
    let mine: Vec<Record> = db::find("cal_event").filter("calendar", key(id_text(&calendar))).filter("created_by", "guest").limit(1000).all()?;
    let future: Vec<&Record> = mine.iter().filter(|e| text(e, "ends_at").unwrap_or_default() >= stamp(today).as_str()).collect();
    let email_key = input.guest_email.trim().to_lowercase();
    if future.len() >= MAX_FUTURE_GUEST_EVENTS || future.iter().filter(|e| text(e, "description").is_some_and(|d| d.to_lowercase().contains(&format!("<{email_key}>")))).count() >= MAX_PER_GUEST {
        return Err(Error::msg("there are too many bookings waiting: cancel one, or ask the host"));
    }
    let name = clean("a name", &input.guest_name, MAX_NAME)?;
    let email = clean("an email address", &input.guest_email, 200)?;
    let note = input.description.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(|n| n.chars().take(MAX_NOTE).collect::<String>());
    let description = match note {
        Some(n) => format!("Booked by {name} <{email}>\n\n{n}"),
        None => format!("Booked by {name} <{email}>"),
    };
    let event: Record = db::create(
        "cal_event",
        &json!({
            "calendar": key(id_text(&calendar)), "title": clean("a title", &input.title, 200)?, "description": description,
            "location": input.location.as_deref().map(|l| l.trim().chars().take(200).collect::<String>()), "starts_at": stamp(start), "ends_at": stamp(end), "all_day": false, "repeat": "none",
            "attendees": [], "attendee_keys": "", "attendee_names": "", "created_by": "guest", "tz": input.tz.as_deref().and_then(crate::zones::parse_zone).map(|z| z.name().to_string()).unwrap_or_else(|| "UTC".to_string()),
        }),
    )?;
    let id = key(id_text(&event)).to_string();
    notify(&host, &format!("{name} booked “{}”", text(&event, "title").unwrap_or("a meeting")), &format!("/calendar/event/{id}"))?;
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct GuestCancel {
    token: String,
    event: String,
}

/// Takes back a meeting a guest booked with this member (and only such a meeting).
pub fn link_cancel(input: GuestCancel) -> Result<Value> {
    let host = host_of(&input.token)?;
    let Some(event) = db::get::<Record>("cal_event", key(&input.event))? else { return Ok(json!({ "deleted": false })) };
    if text(&event, "created_by") != Some("guest") || role_of(text(&event, "calendar").unwrap_or_default(), &host)? != Some(Role::Owner) {
        return Err(Error::msg("this meeting cannot be cancelled with this link"));
    }
    purge_event(&event)?;
    Ok(json!({ "deleted": true }))
}
