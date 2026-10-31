//! Meetings, their guests, and the link that opens the room.

use aether_sdk::{bridge, dates, sfu};
use aether_sdk::prelude::*;

use crate::common::*;

const MAX_GUESTS: usize = 500;
const LINK_SECS: u64 = 4 * 3600;

fn s<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

fn guests_of(meeting: &str) -> Result<Vec<Record>> {
    db::find("meet_guest").filter("meeting", key(meeting)).limit(1000).all()
}

fn meeting_for(id: &str) -> Result<Record> {
    db::get::<Record>("meet_meeting", key(id))?.ok_or_else(|| Error::msg("there is no such meeting"))
}

/// The meeting, if the caller is the host or invited; otherwise it does not exist for them.
fn visible(id: &str) -> Result<(Record, String, bool)> {
    let me = me()?;
    let m = meeting_for(id)?;
    let host = key(s(&m, "host")) == key(&me);
    if !host && !guests_of(key(id_text(&m)))?.iter().any(|g| text(g, "user").is_some_and(|u| key(u) == key(&me))) {
        return Err(Error::msg("there is no such meeting"));
    }
    Ok((m, me, host))
}

/// The starts of some meetings as the caller reads them (their calendar zone); stored UTC when the calendar cannot say.
fn read_starts(starts: &[String]) -> Vec<String> {
    let plain = |t: &String| t.chars().take(16).collect::<String>().replace('T', " ");
    let asked: Vec<&String> = starts.iter().filter(|t| !t.is_empty()).collect();
    if asked.is_empty() {
        return starts.iter().map(|_| "Now".to_string()).collect();
    }
    let read: Vec<String> = plugins::call::<Vec<Value>>("atrium_calendar", "read_times", &json!({ "values": asked }))
        .ok()
        .filter(|r| r.len() == asked.len())
        .map(|r| r.iter().map(|v| v.as_str().unwrap_or_default().to_string()).collect())
        .unwrap_or_else(|| asked.iter().map(|t| plain(t)).collect());
    let mut read = read.into_iter();
    starts.iter().map(|t| if t.is_empty() { "Now".to_string() } else { read.next().unwrap_or_else(|| plain(t)) }).collect()
}

fn row(m: &Record, people: &[Person], me: &str, when: &str) -> Value {
    json!({
        "id": key(id_text(m)), "title": s(m, "title"), "when": when, "host": name_of(people, s(m, "host")),
        "minutes": m.get("minutes").and_then(Value::as_i64).unwrap_or(30), "role": if key(s(m, "host")) == key(me) { "Host" } else { "Invited" },
    })
}

/// Meetings of the caller that have not ended long ago (an instant meeting stays listed for a day), soonest first.
pub fn my_meetings() -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let now = now()?;
    let day_ago = dates::format_datetime(dates::parse_datetime(&now)? - dates::Duration::hours(24));
    let mine_as_guest: Vec<String> = db::find::<Record>("meet_guest").filter("user", me.as_str()).limit(2000).all()?.iter().filter_map(|g| text(g, "meeting").map(|k| key(k).to_string())).collect();
    let mut rows: Vec<Record> = db::find::<Record>("meet_meeting").limit(5000).all()?;
    rows.retain(|m| key(s(m, "host")) == key(&me) || mine_as_guest.iter().any(|k| k == key(id_text(m))));
    rows.retain(|m| s(m, "starts_at").is_empty() || s(m, "starts_at") >= day_ago.as_str());
    rows.sort_by(|a, b| s(a, "starts_at").cmp(s(b, "starts_at")));
    let when = read_starts(&rows.iter().map(|m| s(m, "starts_at").to_string()).collect::<Vec<_>>());
    Ok(rows.iter().zip(&when).map(|(m, w)| row(m, &people, &me, w)).collect())
}

#[derive(Deserialize)]
pub struct MeetingId {
    pub id: String,
}

pub fn get_meeting(input: MeetingId) -> Result<Value> {
    let (m, me, host) = visible(&input.id)?;
    let people = org_people()?;
    let guests: Vec<String> = guests_of(key(id_text(&m)))?.iter().map(|g| name_of(&people, s(g, "user"))).collect();
    let when = read_starts(&[s(&m, "starts_at").to_string()]).remove(0);
    let mut out = row(&m, &people, &me, &when);
    out["about"] = json!(s(&m, "about"));
    out["guests"] = json!(if guests.is_empty() { "Nobody else".to_string() } else { guests.join(", ") });
    out["host_view"] = json!(host);
    out["provider"] = json!(provider_of(&m));
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewMeeting {
    title: String,
    /// `YYYY-MM-DDTHH:MM`; empty means now.
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    minutes: Option<i64>,
    #[serde(default)]
    about: Option<String>,
    #[serde(default)]
    users: Vec<String>,
    /// `builtin` (the default) or `jitsi`.
    #[serde(default)]
    provider: Option<String>,
}

/// Where a meeting is held. A meeting made before the built-in server existed has no provider and stays on Jitsi.
fn provider_of(m: &Record) -> &'static str {
    match s(m, "provider") {
        "builtin" => "builtin",
        _ => "jitsi",
    }
}

fn chosen_provider(asked: Option<&str>) -> Result<&'static str> {
    match asked.map(str::trim).filter(|p| !p.is_empty()) {
        None | Some("builtin") => Ok("builtin"),
        Some("jitsi") => Ok("jitsi"),
        Some(_) => Err(Error::msg("a meeting is held on `builtin` or `jitsi`")),
    }
}

fn make(me: &str, input: NewMeeting, instant: bool) -> Result<Value> {
    let title = clean("a title", &input.title, 200)?;
    let mut starts = if instant { String::new() } else { input.starts_at.as_deref().map(str::trim).unwrap_or_default().to_string() };
    if !starts.is_empty() {
        // The host types the start in their own time zone (the calendar knows it); without a calendar it is read as UTC.
        starts = match plugins::call::<Value>("atrium_calendar", "typed_to_utc", &json!({ "value": starts })) {
            Ok(v) if v["utc"].as_str().is_some() => v["utc"].as_str().unwrap_or_default().to_string(),
            _ if starts.len() == 16 => format!("{starts}:00Z"),
            _ => starts,
        };
    }
    if !starts.is_empty() {
        dates::parse_datetime(&starts).map_err(|_| Error::msg("the start is a date and a time, such as 2026-11-02T14:00"))?;
    }
    let minutes = input.minutes.unwrap_or(30);
    if !(5..=480).contains(&minutes) {
        return Err(Error::msg("a meeting lasts 5 minutes to 8 hours"));
    }
    if input.users.len() > MAX_GUESTS {
        return Err(Error::msg("too many people are invited"));
    }
    let record: Record = db::create(
        "meet_meeting",
        &json!({ "title": title, "host": me, "room": "pending", "provider": chosen_provider(input.provider.as_deref())?, "starts_at": starts, "minutes": minutes, "about": input.about.as_deref().map(str::trim).unwrap_or_default().chars().take(2000).collect::<String>() }),
    )?;
    let id = key(id_text(&record)).to_string();
    // The room name is not guessable: it is made of the record's own random key.
    db::update::<Record>("meet_meeting", &id, &json!({ "room": format!("aether-{id}") }))?;
    let people = org_people()?;
    let mut invited: Vec<String> = Vec::new();
    for u in input.users {
        if key(&u) != key(me) && people.iter().any(|p| key(&p.id) == key(&u)) && !invited.iter().any(|x| key(x) == key(&u)) {
            db::create::<Record>("meet_guest", &json!({ "meeting": id, "user": u }))?;
            invited.push(u);
        }
    }
    if !starts.is_empty() {
        // The meeting also goes into the calendar of the host and of the guests, with the way back to the room.
        let event = calendar_event(&id, &title, &starts, minutes, input.about.as_deref(), &invited);
        if let Some(event) = event {
            db::update::<Record>("meet_meeting", &id, &json!({ "event": event }))?;
        }
    }
    if !invited.is_empty() {
        let note = if starts.is_empty() { format!("{} started a meeting: {title}", name_of(&people, me)) } else { format!("{} invites you: {title} ({})", name_of(&people, me), starts.replace('T', " ")) };
        Notification::new(note).link(format!("/meet/m/{id}")).to(Audience::Actors(invited)).send()?;
    }
    Ok(json!({ "id": id }))
}

/// Put a planned meeting into the calendar. A calendar that cannot take it (a clash is only a warning) never stops the meeting.
fn calendar_event(id: &str, title: &str, starts: &str, minutes: i64, about: Option<&str>, guests: &[String]) -> Option<String> {
    let start = dates::parse_datetime(starts).ok()?;
    let end = start + dates::Duration::minutes(minutes);
    let stamp = |t: dates::NaiveDateTime| t.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut description = format!("Video meeting. Open it in Meetings: /meet/m/{id}");
    if let Some(note) = about.map(str::trim).filter(|n| !n.is_empty()) {
        description = format!("{note}\n\n{description}");
    }
    let made: Value = plugins::call(
        "atrium_calendar",
        "create_event",
        &json!({ "title": title, "starts_at": stamp(start), "ends_at": stamp(end), "description": description, "location": format!("/meet/m/{id}"), "attendees": guests }),
    )
    .ok()?;
    made["id"].as_str().map(str::to_string)
}

pub fn create_meeting(input: NewMeeting) -> Result<Value> {
    let me = me()?;
    make(&me, input, false)
}

#[derive(Deserialize, Default)]
pub struct Instant {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    users: Vec<String>,
    #[serde(default)]
    provider: Option<String>,
}

/// A meeting that starts now: the host gets the link to join at once.
pub fn start_now(input: Instant) -> Result<Value> {
    let me = me()?;
    let made = make(&me, NewMeeting { title: input.title.unwrap_or_else(|| "Quick meeting".into()), starts_at: None, minutes: Some(60), about: None, users: input.users, provider: input.provider }, true)?;
    let mut joined = join_meeting(MeetingId { id: made["id"].as_str().unwrap_or_default().to_string() })?;
    joined["id"] = made["id"].clone();
    Ok(joined)
}

/// How this person gets into the room. A built-in meeting is a page of the app; a Jitsi meeting is a link made for this
/// person and this room, where the host moderates.
pub fn join_meeting(input: MeetingId) -> Result<Value> {
    let (m, me, host) = visible(&input.id)?;
    if provider_of(&m) == "builtin" {
        return Ok(json!({ "open_page": format!("/meet/room/{}", key(id_text(&m))) }));
    }
    let people = org_people()?;
    let link: Value = bridge::call("jitsi", "join_link", &json!({ "room": s(&m, "room"), "name": name_of(&people, &me), "moderator": host, "ttl_secs": LINK_SECS }))?;
    Ok(json!({ "open_url": link["url"] }))
}

#[derive(Deserialize)]
pub struct Join {
    /// The `{meeting}` of the room name `meeting.{meeting}`.
    #[serde(default)]
    params: Value,
}

/// The kernel asks this on every join of a built-in room, as the person who wants in: the host, or someone invited.
/// The host moderates the call. Anyone else does not get to know the meeting exists.
pub fn can_join_meeting(input: Join) -> Result<Value> {
    let id = input.params.get("meeting").and_then(Value::as_str).unwrap_or_default();
    let Ok((m, me, host)) = visible(id) else {
        return Ok(json!({ "allow": false, "reason": "there is no such meeting" }));
    };
    if provider_of(&m) != "builtin" {
        return Ok(json!({ "allow": false, "reason": "this meeting is held elsewhere" }));
    }
    let people = org_people()?;
    Ok(json!({ "allow": true, "role": if host { "host" } else { "member" }, "name": name_of(&people, &me) }))
}

#[derive(Deserialize)]
pub struct Moderate {
    /// The `{meeting}` of the call page.
    meeting: String,
    /// `mute`, `remove` or `end`.
    action: String,
    /// Whom it is for (the `user` of the person in the call); not needed to end the call.
    #[serde(default)]
    user: String,
}

/// What the host does to a running call: silence someone, send someone out, or end it for everyone.
pub fn moderate_call(input: Moderate) -> Result<Value> {
    let (m, me, host) = visible(&input.meeting)?;
    if !host {
        return Err(Error::msg("only the host can do that"));
    }
    if provider_of(&m) != "builtin" {
        return Err(Error::msg("this meeting is held elsewhere"));
    }
    let room = format!("meeting.{}", input.meeting);
    match input.action.as_str() {
        "end" => sfu::end(&room, "The host ended the meeting")?,
        "mute" | "remove" => {
            if input.user.is_empty() {
                return Err(Error::msg("say whom"));
            }
            if key(&input.user) == key(&me) {
                return Err(Error::msg("you cannot do that to yourself; leave instead"));
            }
            if input.action == "mute" {
                sfu::mute(&room, &input.user, "audio")?;
            } else {
                sfu::remove(&room, &input.user, "The host removed you from the meeting")?;
            }
        }
        _ => return Err(Error::msg("unknown action")),
    }
    Ok(json!({ "ok": true }))
}

pub fn cancel_meeting(input: MeetingId) -> Result<Value> {
    let (m, me, host) = visible(&input.id)?;
    if !host {
        return Err(Error::msg("only the host can cancel a meeting"));
    }
    let id = key(id_text(&m)).to_string();
    let guests: Vec<String> = guests_of(&id)?.iter().filter_map(|g| text(g, "user").map(str::to_string)).collect();
    for g in guests_of(&id)? {
        db::delete::<Record>("meet_guest", key(id_text(&g)))?;
    }
    if let Some(event) = Some(s(&m, "event")).filter(|e| !e.is_empty()) {
        let _ = plugins::call::<Value>("atrium_calendar", "cancel_booked", &json!({ "event": event }));
    }
    db::delete::<Record>("meet_meeting", &id)?;
    if !guests.is_empty() {
        Notification::new(format!("Meeting cancelled: {}", s(&m, "title"))).to(Audience::Actors(guests)).send()?;
    }
    let _ = me;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    search: Option<String>,
}

pub fn pick_people(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.or(input.search).unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?.into_iter().filter(|p| key(&p.id) != key(&me) && (needle.is_empty() || p.name.to_lowercase().contains(&needle))).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}
