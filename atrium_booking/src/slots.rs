//! Free times, booking one, and cancelling it.

use aether_sdk::dates::{self, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use aether_sdk::prelude::*;

use chrono_tz::Tz;

use crate::common::*;
use crate::services::{any_active, weekdays_of, ServiceOnly};

const MAX_SLOTS: usize = 300;

pub fn parse(text: &str) -> Result<NaiveDateTime> {
    dates::parse_datetime(text)
}

pub fn stamp(moment: NaiveDateTime) -> String {
    dates::format_datetime(moment)
}

fn hhmm(text: &str) -> NaiveTime {
    NaiveTime::parse_from_str(text, "%H:%M").unwrap_or(NaiveTime::MIN)
}

/// The start times a service offers on one day, before anything is taken: every `minutes + 0` from the opening hour while
/// a whole meeting fits before closing. Not in the past and not sooner than the notice allows.
pub fn day_starts(date: NaiveDate, from: &str, to: &str, minutes: i64, earliest: NaiveDateTime) -> Vec<NaiveDateTime> {
    let (open, close) = (date.and_time(hhmm(from)), date.and_time(hhmm(to)));
    let mut out = Vec::new();
    let mut at = open;
    while at + Duration::minutes(minutes) <= close {
        if at >= earliest {
            out.push(at);
        }
        at += Duration::minutes(minutes);
    }
    out
}

/// A slot is free when no busy time touches it or the break after it (or before it, for the same break).
pub fn is_free(start: NaiveDateTime, minutes: i64, buffer: i64, busy: &[(NaiveDateTime, NaiveDateTime)]) -> bool {
    let (s, e) = (start, start + Duration::minutes(minutes));
    let pad = Duration::minutes(buffer);
    busy.iter().all(|(bs, be)| !(*bs < e + pad && *be + pad > s))
}

/// Where the busy times come from: the host's calendar for a member, or the host's booking link for a guest.
enum Source<'a> {
    Member,
    Link(&'a str),
}

fn busy_of(o: &Offer, source: &Source, from: NaiveDateTime, to: NaiveDateTime) -> Result<Vec<(NaiveDateTime, NaiveDateTime)>> {
    let rows: Vec<Value> = match source {
        Source::Member => plugins::call("atrium_calendar", "busy_of", &json!({ "user": o.host, "start": stamp(from), "end": stamp(to) }))?,
        Source::Link(token) => plugins::call("atrium_calendar", "link_busy", &json!({ "token": token, "start": stamp(from), "end": stamp(to) }))?,
    };
    Ok(rows.iter().filter_map(|r| Some((parse(text(r, "start")?).ok()?, parse(text(r, "end")?).ok()?))).collect())
}

struct Offer {
    key: String,
    host: String,
    host_name: String,
    title: String,
    description: String,
    location: Option<String>,
    minutes: i64,
    buffer: i64,
    notice: i64,
    days: i64,
    weekdays: Vec<i64>,
    from: String,
    to: String,
    /// The zone the days and hours are in.
    zone: Tz,
}

fn offer_of(s: &Record) -> Result<Offer> {
    let n = |f: &str, d: i64| s.get(f).and_then(Value::as_i64).unwrap_or(d);
    Ok(Offer {
        key: key(id_text(s)).to_string(),
        host: text(s, "owner").unwrap_or_default().to_string(),
        host_name: text(s, "owner_name").unwrap_or_default().to_string(),
        title: text(s, "title").unwrap_or("Meeting").to_string(),
        description: text(s, "description").unwrap_or_default().to_string(),
        location: text(s, "location").filter(|l| !l.is_empty()).map(str::to_string),
        minutes: n("minutes", 30),
        buffer: n("buffer", 0),
        notice: n("notice_hours", 4),
        days: n("days_ahead", 30),
        weekdays: weekdays_of(s.get("weekdays").unwrap_or(&Value::Null))?,
        from: text(s, "from_time").unwrap_or("09:00").to_string(),
        to: text(s, "to_time").unwrap_or("17:00").to_string(),
        zone: zone_of(text(s, "tz").unwrap_or_default()).unwrap_or(chrono_tz::UTC),
    })
}

fn offer(id: &str) -> Result<Offer> {
    let s = any_active(id)?;
    if !flag(&s, "active") {
        return Err(Error::msg("this is not offered at the moment"));
    }
    offer_of(&s)
}

/// Every free start for the offer between today and `days` ahead, as UTC moments in time order. `only_day` is a day of the
/// offer's own zone.
fn starts(o: &Offer, source: &Source, now: NaiveDateTime, only_day: Option<NaiveDate>) -> Result<Vec<NaiveDateTime>> {
    let earliest = now + Duration::hours(o.notice);
    let today = utc_to_local(now, o.zone).date();
    let (first, last) = match only_day {
        Some(d) => (d, d),
        None => (today, today + Duration::days(o.days)),
    };
    if last < today || last > today + Duration::days(o.days) {
        return Ok(Vec::new());
    }
    let busy = busy_of(o, source, local_to_utc(first.and_time(NaiveTime::MIN), o.zone) - Duration::days(1), local_to_utc(last.and_time(NaiveTime::MIN), o.zone) + Duration::days(2))?;
    let mut out = Vec::new();
    let mut day = first;
    while day <= last {
        if o.weekdays.contains(&(day.weekday().number_from_monday() as i64)) {
            // The hours are wall-clock times in the offer's zone: the same in summer and winter.
            let local_earliest = utc_to_local(earliest, o.zone);
            out.extend(day_starts(day, &o.from, &o.to, o.minutes, local_earliest).into_iter().map(|s| local_to_utc(s, o.zone)).filter(|s| is_free(*s, o.minutes, o.buffer, &busy)));
        }
        day += Duration::days(1);
    }
    out.sort();
    out.dedup();
    Ok(out)
}

/// Stored UTC moments as the caller reads them (their calendar zone), `2026-11-02 14:00`; UTC when the calendar cannot say.
fn read_times(utc: &[NaiveDateTime]) -> Vec<NaiveDateTime> {
    let wanted: Vec<String> = utc.iter().map(|t| stamp(*t)).collect();
    if wanted.is_empty() {
        return Vec::new();
    }
    plugins::call::<Vec<Value>>("atrium_calendar", "read_times", &json!({ "values": wanted }))
        .ok()
        .filter(|r| r.len() == utc.len())
        .map(|r| r.iter().zip(utc).map(|(v, t)| v.as_str().and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok()).unwrap_or(*t)).collect())
        .unwrap_or_else(|| utc.to_vec())
}

/// "Tue Oct 20" and "09:00 – 09:30" for a slot whose start is read as `local`.
fn slot_labels(local: NaiveDateTime, minutes: i64) -> (String, String) {
    (local.format("%a %b %-d").to_string(), format!("{} – {}", local.format("%H:%M"), (local + Duration::minutes(minutes)).format("%H:%M")))
}

pub fn free_slots(input: ServiceOnly) -> Result<Vec<Value>> {
    me()?;
    let o = offer(&input.id)?;
    let now = parse(&now()?)?;
    let found: Vec<NaiveDateTime> = starts(&o, &Source::Member, now, None)?.into_iter().take(MAX_SLOTS).collect();
    let local = read_times(&found);
    Ok(found
        .iter()
        .zip(&local)
        .map(|(s, l)| {
            let (day, time) = slot_labels(*l, o.minutes);
            json!({ "id": format!("{}|{}", o.key, s.format("%Y%m%dT%H%M")), "day": day, "time": time, "starts_at": stamp(*s) })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct Book {
    /// `service|20261020T0930` (UTC), as a row of the free times has it.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// A time as a row of the list or a plain field names it: `20261020T0930` (UTC) or a full timestamp.
fn parse_start(text: &str) -> Result<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y%m%dT%H%M").or_else(|_| parse(text)).map_err(|_| Error::msg("that is not a time"))
}

pub fn book(input: Book) -> Result<Value> {
    let me = me()?;
    let (service, start) = match (&input.id, &input.service, &input.starts_at) {
        (Some(id), _, _) if id.contains('|') => {
            let (a, b) = id.split_once('|').unwrap_or_default();
            (a.to_string(), b.to_string())
        }
        (_, Some(s), Some(t)) => (s.clone(), t.clone()),
        _ => return Err(Error::msg("choose a time")),
    };
    let o = offer(&service)?;
    if key(&o.host) == key(&me) {
        return Err(Error::msg("you cannot book your own time"));
    }
    // A row of the list names its time as 20261020T0930 (the page turns anything after a colon in a row's id into the id);
    // the plain fields may also carry a full timestamp.
    let start = parse_start(&start)?;
    let now = parse(&now()?)?;
    if !starts(&o, &Source::Member, now, Some(utc_to_local(start, o.zone).date()))?.contains(&start) {
        return Err(Error::msg("that time is not free any more: pick another"));
    }
    let end = start + Duration::minutes(o.minutes);
    let people = org_people()?;
    let title = format!("{}: {}", o.title, name_of(&people, &me));
    let made: Value = plugins::call(
        "atrium_calendar",
        "book_slot",
        &json!({ "host": o.host, "title": title, "starts_at": stamp(start), "ends_at": stamp(end), "location": o.location, "description": input.note.as_deref().map(str::trim) }),
    )?;
    let event = text(&made, "id").unwrap_or_default().to_string();
    let booking: Record = db::create(
        "bk_booking",
        &json!({ "service": o.key, "title": o.title, "host": o.host, "booker": me, "starts_at": stamp(start), "ends_at": stamp(end), "status": "confirmed", "event": event, "note": input.note.as_deref().map(str::trim) }),
    )?;
    Ok(json!({ "id": key(id_text(&booking)), "event": event }))
}

// ---- people outside the organization ---------------------------------------------------------------------------------

/// How many meetings a guest (by address) may have waiting on one offer, and the longest a note may be.
const MAX_GUEST_BOOKINGS: usize = 3;

#[derive(Deserialize)]
pub struct PublicRef {
    link: String,
    service: String,
}

/// The offer a public address points to: public, switched on, and with the link the host gave it.
fn public_offer(r: &PublicRef) -> Result<(Offer, Record)> {
    let s = db::get::<Record>("bk_service", key(&r.service))?.filter(|s| flag(s, "public") && flag(s, "active") && text(s, "link").is_some_and(|l| !l.is_empty() && l == r.link.trim())).ok_or_else(|| Error::msg("this page does not exist (any more)"))?;
    Ok((offer_of(&s)?, s))
}

pub fn public_service(input: PublicRef) -> Result<Value> {
    let (o, _) = public_offer(&input)?;
    let (days, hours) = (crate::services::days_label(&o.weekdays), format!("{}–{}", o.from, o.to));
    Ok(json!({
        "title": o.title, "description": o.description, "host": o.host_name, "location": o.location, "length": format!("{} min", o.minutes),
        "subtitle": format!("with {} · {} min · {} {} · times in {}", o.host_name, o.minutes, days, hours, o.zone.name()),
    }))
}

/// The free times of a public offer, as a list to pick from; each reads in the offer's own zone.
pub fn public_slots(input: PublicRef) -> Result<Vec<Value>> {
    let (o, _) = public_offer(&input)?;
    let now = parse(&now()?)?;
    Ok(starts(&o, &Source::Link(input.link.trim()), now, None)?
        .into_iter()
        .take(MAX_SLOTS)
        .map(|s| {
            let (day, time) = slot_labels(utc_to_local(s, o.zone), o.minutes);
            json!({ "id": s.format("%Y%m%dT%H%M").to_string(), "name": format!("{day}, {time}") })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct PublicBook {
    link: String,
    service: String,
    /// `20261020T0930` (UTC), as the list of free times gives it.
    slot: String,
    name: String,
    email: String,
    #[serde(default)]
    note: Option<String>,
}

fn looks_like_email(address: &str) -> bool {
    let a = address.trim();
    let Some((local, domain)) = a.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !a.contains(char::is_whitespace) && a.chars().count() <= 200
}

pub fn public_book(input: PublicBook) -> Result<Value> {
    let (o, service) = public_offer(&PublicRef { link: input.link.clone(), service: input.service.clone() })?;
    let name = clean("your name", &input.name, 120)?;
    if !looks_like_email(&input.email) {
        return Err(Error::msg("give an email address, like you@example.org"));
    }
    let email = input.email.trim().to_lowercase();
    let start = parse_start(input.slot.trim())?;
    let now = parse(&now()?)?;
    let source = Source::Link(input.link.trim());
    if !starts(&o, &source, now, Some(utc_to_local(start, o.zone).date()))?.contains(&start) {
        return Err(Error::msg("that time is not free any more: pick another"));
    }
    let waiting = db::find::<Record>("bk_booking")
        .filter("service", o.key.as_str())
        .filter("guest_email", email.as_str())
        .filter("status", "confirmed")
        .limit(100)
        .all()?
        .iter()
        .filter(|b| text(b, "ends_at").unwrap_or_default() >= stamp(now).as_str())
        .count();
    if waiting >= MAX_GUEST_BOOKINGS {
        return Err(Error::msg("you already have several meetings booked: cancel one first"));
    }
    let end = start + Duration::minutes(o.minutes);
    let note: String = input.note.as_deref().map(str::trim).unwrap_or_default().chars().take(1000).collect();
    let made: Value = plugins::call(
        "atrium_calendar",
        "link_book",
        &json!({
            "token": input.link.trim(), "title": format!("{}: {}", o.title, name), "starts_at": stamp(start), "ends_at": stamp(end), "location": o.location,
            "description": note, "guest_name": name, "guest_email": email, "tz": o.zone.name(),
        }),
    )?;
    let event = text(&made, "id").unwrap_or_default().to_string();
    let booking: Record = db::create(
        "bk_booking",
        &json!({
            "service": o.key, "title": o.title, "host": o.host, "booker": "", "guest_name": name, "guest_email": email, "link": input.link.trim(),
            "starts_at": stamp(start), "ends_at": stamp(end), "status": "confirmed", "event": event, "note": note,
        }),
    )?;
    let token = key(id_text(&booking)).to_string();
    Notification::new(format!("{name} booked “{}”", o.title)).link("/booking/mine".to_string()).to(Audience::Actors(vec![text(&service, "owner").unwrap_or_default().to_string()])).send()?;
    Ok(json!({ "token": token }))
}

#[derive(Deserialize)]
pub struct Token {
    token: String,
}

fn guest_booking(token: &str) -> Result<Record> {
    db::get::<Record>("bk_booking", key(token.trim()))?.filter(|b| text(b, "booker").is_none_or(str::is_empty)).ok_or_else(|| Error::msg("there is no such booking"))
}

/// A guest's booking, found by the token they were given.
pub fn public_booking(input: Token) -> Result<Value> {
    let b = guest_booking(&input.token)?;
    let service = db::get::<Record>("bk_service", text(&b, "service").map(key).unwrap_or_default())?;
    let zone = service.as_ref().and_then(|s| zone_of(text(s, "tz").unwrap_or_default())).unwrap_or(chrono_tz::UTC);
    let start = text(&b, "starts_at").and_then(|t| parse(t).ok()).map(|t| utc_to_local(t, zone));
    Ok(json!({
        "title": text(&b, "title"), "with": service.as_ref().and_then(|s| text(s, "owner_name")), "status": text(&b, "status"), "guest": text(&b, "guest_name"),
        "when": start.map(|s| format!("{} ({})", s.format("%a %b %-d, %H:%M"), zone.name())), "token": key(id_text(&b)),
    }))
}

pub fn public_cancel(input: Token) -> Result<Value> {
    let b = guest_booking(&input.token)?;
    if text(&b, "status") != Some("confirmed") {
        return Err(Error::msg("this booking is already cancelled"));
    }
    if let (Some(event), Some(link)) = (text(&b, "event").filter(|e| !e.is_empty()), text(&b, "link").filter(|l| !l.is_empty())) {
        plugins::call::<Value>("atrium_calendar", "link_cancel", &json!({ "token": link, "event": event }))?;
    }
    db::update::<Record>("bk_booking", key(id_text(&b)), &json!({ "status": "cancelled" }))?;
    if let Some(host) = text(&b, "host") {
        Notification::new(format!("{} cancelled “{}”", text(&b, "guest_name").unwrap_or("A guest"), text(&b, "title").unwrap_or("a meeting"))).link("/booking/mine".to_string()).to(Audience::Actors(vec![host.to_string()])).send()?;
    }
    Ok(json!({ "ok": true }))
}

fn notify(user: &str, title: &str) -> Result<()> {
    Notification::new(title.to_string()).link("/booking/mine".to_string()).to(Audience::Actors(vec![user.to_string()])).send()?;
    Ok(())
}

pub fn my_bookings(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let now = now()?;
    let mut rows: Vec<Record> = db::find("bk_booking").filter("booker", me.as_str()).limit(2000).all()?;
    rows.extend(db::find::<Record>("bk_booking").filter("host", me.as_str()).limit(2000).all()?);
    rows.retain(|b| text(b, "status") == Some("confirmed") && text(b, "ends_at").unwrap_or_default() >= now.as_str());
    rows.sort_by_key(|b| text(b, "starts_at").unwrap_or_default().to_string());
    let mut seen = Vec::new();
    let mut mine: Vec<Record> = Vec::new();
    for b in rows {
        if !seen.contains(&key(id_text(&b)).to_string()) {
            seen.push(key(id_text(&b)).to_string());
            mine.push(b);
        }
    }
    // Times as the caller reads them.
    let starts: Vec<NaiveDateTime> = mine.iter().map(|b| text(b, "starts_at").and_then(|t| parse(t).ok()).unwrap_or_default()).collect();
    let local = read_times(&starts);
    let mut out = Vec::new();
    for (b, start) in mine.iter().zip(&local) {
        let hosting = text(b, "host").is_some_and(|h| key(h) == key(&me));
        let other = if hosting { text(b, "booker") } else { text(b, "host") }.unwrap_or_default();
        let with = if other.is_empty() { format!("{} (guest)", text(b, "guest_name").unwrap_or("Someone")) } else { name_of(&people, other) };
        out.push(json!({
            "id": key(id_text(b)), "title": text(b, "title"), "when": start.format("%a %b %-d, %H:%M").to_string(), "with": with,
            "you_are": if hosting { "host" } else { "guest" }, "note": text(b, "note"),
        }));
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct CancelIn {
    id: String,
}

pub fn cancel_booking(input: CancelIn) -> Result<Value> {
    let me = me()?;
    let b = db::get::<Record>("bk_booking", key(&input.id))?.ok_or_else(|| Error::msg("there is no such booking"))?;
    let hosting = text(&b, "host").is_some_and(|h| key(h) == key(&me));
    let booked = text(&b, "booker").is_some_and(|h| key(h) == key(&me));
    if !hosting && !booked {
        return Err(Error::msg("there is no such booking"));
    }
    if text(&b, "status") != Some("confirmed") {
        return Err(Error::msg("this booking is already cancelled"));
    }
    if let Some(event) = text(&b, "event").filter(|e| !e.is_empty()) {
        plugins::call::<Value>("atrium_calendar", "cancel_booked", &json!({ "event": event }))?;
    }
    db::update::<Record>("bk_booking", key(id_text(&b)), &json!({ "status": "cancelled" }))?;
    let people = org_people()?;
    let other = if hosting { text(&b, "booker") } else { text(&b, "host") }.unwrap_or_default();
    if !other.is_empty() {
        notify(other, &format!("{} cancelled “{}”", name_of(&people, &me), text(&b, "title").unwrap_or("a meeting")))?;
    }
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        parse(s).unwrap()
    }

    #[test]
    fn a_day_is_cut_into_whole_meetings() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let s = day_starts(day, "09:00", "10:30", 30, at("2026-10-01T00:00:00Z"));
        assert_eq!(s.iter().map(|x| x.format("%H:%M").to_string()).collect::<Vec<_>>(), ["09:00", "09:30", "10:00"]);
        assert!(day_starts(day, "09:00", "09:20", 30, at("2026-10-01T00:00:00Z")).is_empty());
    }

    #[test]
    fn notice_hides_what_is_too_soon() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let s = day_starts(day, "09:00", "11:00", 60, at("2026-10-20T09:30:00Z"));
        assert_eq!(s, vec![at("2026-10-20T10:00:00Z")]);
    }

    #[test]
    fn hours_are_wall_clock_times_in_the_offers_zone() {
        let oslo = zone_of("Europe/Oslo").unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let summer: Vec<_> = day_starts(day, "09:00", "10:00", 60, at("2026-01-01T00:00:00Z")).into_iter().map(|s| local_to_utc(s, oslo)).collect();
        assert_eq!(summer, vec![at("2026-07-01T07:00:00Z")]);
        let winter: Vec<_> = day_starts(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap(), "09:00", "10:00", 60, at("2026-01-01T00:00:00Z")).into_iter().map(|s| local_to_utc(s, oslo)).collect();
        assert_eq!(winter, vec![at("2026-12-01T08:00:00Z")]);
        assert_eq!(zone_of("").map(|z| z.name()), Some("UTC"));
        assert!(zone_of("Mars/Base").is_none());
    }

    #[test]
    fn an_email_address_looks_like_one() {
        assert!(looks_like_email("ann@example.org"));
        assert!(!looks_like_email("ann@example") && !looks_like_email("@example.org") && !looks_like_email("an n@example.org") && !looks_like_email("ann"));
    }

    #[test]
    fn busy_time_and_the_break_around_it_take_slots() {
        let busy = vec![(at("2026-10-20T10:00:00Z"), at("2026-10-20T11:00:00Z"))];
        assert!(is_free(at("2026-10-20T09:00:00Z"), 60, 0, &busy));
        assert!(!is_free(at("2026-10-20T09:00:00Z"), 60, 15, &busy));
        assert!(!is_free(at("2026-10-20T10:30:00Z"), 30, 0, &busy));
        assert!(is_free(at("2026-10-20T11:00:00Z"), 30, 0, &busy));
        assert!(!is_free(at("2026-10-20T11:00:00Z"), 30, 10, &busy));
    }
}
