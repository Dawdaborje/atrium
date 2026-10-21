//! What a person offers: a kind of meeting, its length and the hours it can be booked in.

use aether_sdk::dates;
use aether_sdk::prelude::*;

use crate::common::*;

pub const MAX_SERVICES: usize = 20;

/// A number typed on a page may arrive as a number or as text.
pub fn int(value: &Option<Value>, label: &str, min: i64, max: i64) -> Result<Option<i64>> {
    let Some(value) = value else { return Ok(None) };
    let n = match value {
        Value::Null => return Ok(None),
        Value::Number(n) => n.as_i64(),
        Value::String(s) if s.trim().is_empty() => return Ok(None),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .ok_or_else(|| Error::msg(format!("{label} is a whole number")))?;
    if n < min || n > max {
        return Err(Error::msg(format!("{label} is between {min} and {max}")));
    }
    Ok(Some(n))
}

pub fn clock(label: &str, value: &str) -> Result<String> {
    let t = dates::parse_time(value.trim()).map_err(|_| Error::msg(format!("{label} is a time like 09:00")))?;
    Ok(t.format("%H:%M").to_string())
}

/// Weekdays as the numbers 1 (Monday) to 7 (Sunday), sorted, without repeats. Numbers or the names of the days.
pub fn weekdays_of(value: &Value) -> Result<Vec<i64>> {
    let items: Vec<Value> = match value {
        Value::Array(a) => a.clone(),
        Value::String(s) => s.split(',').map(|x| Value::String(x.trim().to_string())).filter(|x| x != "").collect(),
        Value::Null => Vec::new(),
        _ => return Err(Error::msg("the days are a list of weekdays")),
    };
    let mut out = Vec::new();
    for item in items {
        let n = match &item {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => s.trim().parse().ok().or_else(|| names().iter().position(|d| d.eq_ignore_ascii_case(s.trim())).map(|i| i as i64 + 1)),
            _ => None,
        };
        match n {
            Some(n) if (1..=7).contains(&n) => {
                if !out.contains(&n) {
                    out.push(n)
                }
            }
            _ => return Err(Error::msg("a weekday is 1 (Monday) to 7 (Sunday)")),
        }
    }
    out.sort();
    Ok(out)
}

fn names() -> [&'static str; 7] {
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"]
}

pub fn pick_weekdays() -> Result<Vec<Value>> {
    Ok(names().iter().enumerate().map(|(i, n)| json!({ "id": (i + 1).to_string(), "name": n })).collect())
}

fn check_hours(from: &str, to: &str) -> Result<()> {
    if from >= to {
        return Err(Error::msg("the hours must end after they start"));
    }
    Ok(())
}

fn fits(minutes: i64, from: &str, to: &str) -> Result<()> {
    let m = |t: &str| t[..2].parse::<i64>().unwrap_or(0) * 60 + t[3..].parse::<i64>().unwrap_or(0);
    if m(to) - m(from) < minutes {
        return Err(Error::msg("the hours are shorter than one meeting"));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct ServiceOnly {
    pub id: String,
}

#[derive(Deserialize)]
pub struct Search {
    #[serde(default)]
    q: Option<String>,
}

#[derive(Deserialize)]
pub struct NewService {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    minutes: Option<Value>,
    #[serde(default)]
    buffer: Option<Value>,
    #[serde(default)]
    notice_hours: Option<Value>,
    #[serde(default)]
    days_ahead: Option<Value>,
    #[serde(default)]
    weekdays: Option<Value>,
    #[serde(default)]
    from_time: Option<String>,
    #[serde(default)]
    to_time: Option<String>,
    #[serde(default)]
    tz: Option<String>,
}

pub fn mine(id: &str) -> Result<Record> {
    let me = me()?;
    let service = db::get::<Record>("bk_service", key(id))?.ok_or_else(|| Error::msg("there is no such service"))?;
    if !text(&service, "owner").is_some_and(|o| key(o) == key(&me)) {
        return Err(Error::msg("there is no such service"));
    }
    Ok(service)
}

pub fn any_active(id: &str) -> Result<Record> {
    let service = db::get::<Record>("bk_service", key(id))?.ok_or_else(|| Error::msg("there is no such service"))?;
    Ok(service)
}

fn days_text(service: &Record) -> String {
    days_label(&weekdays_of(service.get("weekdays").unwrap_or(&Value::Null)).unwrap_or_default())
}

pub fn days_label(days: &[i64]) -> String {
    let short = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    if days == [1, 2, 3, 4, 5] {
        return "Weekdays".to_string();
    }
    days.iter().map(|d| short[*d as usize - 1]).collect::<Vec<_>>().join(", ")
}

fn row(service: &Record, people: &[Person], upcoming: usize) -> Value {
    json!({
        "id": key(id_text(service)), "title": text(service, "title"), "description": text(service, "description"), "location": text(service, "location"),
        "minutes": service.get("minutes"), "length": format!("{} min", service.get("minutes").and_then(Value::as_i64).unwrap_or(30)),
        "days": days_text(service), "hours": format!("{}–{} {}", text(service, "from_time").unwrap_or("09:00"), text(service, "to_time").unwrap_or("17:00"), text(service, "tz").filter(|z| !z.is_empty()).unwrap_or("UTC")),
        "host": name_of(people, text(service, "owner").unwrap_or_default()), "host_id": text(service, "owner"), "active": flag(service, "active"), "upcoming": upcoming,
    })
}

pub fn my_services(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let now = now()?;
    let rows: Vec<Record> = db::find("bk_service").filter("owner", me.as_str()).limit(100).all()?;
    let bookings: Vec<Record> = db::find("bk_booking").filter("host", me.as_str()).filter("status", "confirmed").limit(5000).all()?;
    Ok(rows
        .iter()
        .map(|s| row(s, &people, bookings.iter().filter(|b| text(b, "service").is_some_and(|k| key(k) == key(id_text(s)))).filter(|b| text(b, "starts_at").unwrap_or_default() >= now.as_str()).count()))
        .collect())
}

pub fn get_service(input: ServiceOnly) -> Result<Value> {
    let service = mine(&input.id)?;
    let mut out = service.clone();
    out["id"] = json!(key(id_text(&service)));
    out["weekdays"] = json!(weekdays_of(service.get("weekdays").unwrap_or(&Value::Null))?.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    out["public_path"] = json!(match (flag(&service, "public"), text(&service, "link").filter(|l| !l.is_empty())) {
        (true, Some(link)) => format!("/book/{link}/{}", key(id_text(&service))),
        _ => String::new(),
    });
    Ok(out)
}

pub fn service_header(input: ServiceOnly) -> Result<Value> {
    let service = any_active(&input.id)?;
    let people = org_people()?;
    let mut out = row(&service, &people, 0);
    out["subtitle"] = json!(format!("with {} · {} · {} {}", out["host"].as_str().unwrap_or_default(), out["length"].as_str().unwrap_or_default(), out["days"].as_str().unwrap_or_default(), out["hours"].as_str().unwrap_or_default()));
    Ok(out)
}

/// Active services of the other people in the organization, searched by title, host or text.
pub fn find_services(input: Search) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let rows: Vec<Record> = db::find("bk_service").limit(2000).all()?;
    let mut out: Vec<Value> = rows
        .iter()
        .filter(|s| flag(s, "active") && !text(s, "owner").is_some_and(|o| key(o) == key(&me)))
        .map(|s| row(s, &people, 0))
        .filter(|r| needle.is_empty() || ["title", "host", "description"].iter().any(|f| r[*f].as_str().is_some_and(|v| v.to_lowercase().contains(&needle))))
        .collect();
    out.sort_by_key(|r| (r["host"].as_str().unwrap_or_default().to_lowercase(), r["title"].as_str().unwrap_or_default().to_lowercase()));
    Ok(out)
}

/// The zone typed on the page, as its proper name; empty is the host's own calendar zone when they have one, else UTC.
fn zone_name(typed: &Option<String>, base: Option<&Record>) -> Result<String> {
    match typed.as_deref().map(str::trim) {
        Some(t) if !t.is_empty() => Ok(zone_of(t).ok_or_else(|| Error::msg("that is not a time zone; use a name like Europe/Oslo"))?.name().to_string()),
        _ => match base.and_then(|b| text(b, "tz")).filter(|z| !z.is_empty()) {
            Some(z) => Ok(z.to_string()),
            None => {
                let mine: Value = plugins::call("atrium_calendar", "my_timezone", &json!({})).unwrap_or_else(|_| json!({}));
                Ok(mine.get("tz").and_then(Value::as_str).filter(|z| !z.is_empty()).unwrap_or("UTC").to_string())
            }
        },
    }
}

/// The caller's name, kept on the service for its public page.
fn my_name(me: &str) -> Result<String> {
    Ok(name_of(&org_people()?, me))
}

/// A service made public gets the host's booking link in the calendar, whose token is part of its address.
fn link_token() -> Result<String> {
    let made: Value = plugins::call("atrium_calendar", "make_link", &json!({}))?;
    made.get("token").and_then(Value::as_str).map(str::to_string).ok_or_else(|| Error::msg("the calendar did not make a booking link"))
}

struct Fields {
    minutes: i64,
    buffer: i64,
    notice_hours: i64,
    days_ahead: i64,
    weekdays: Vec<i64>,
    from_time: String,
    to_time: String,
}

fn fields(i: &NewService, base: Option<&Record>) -> Result<Fields> {
    let old_int = |f: &str, d: i64| base.and_then(|b| b.get(f)).and_then(Value::as_i64).unwrap_or(d);
    let minutes = int(&i.minutes, "the length", 5, 480)?.unwrap_or_else(|| old_int("minutes", 30));
    let buffer = int(&i.buffer, "the break after a meeting", 0, 240)?.unwrap_or_else(|| old_int("buffer", 0));
    let notice_hours = int(&i.notice_hours, "the notice", 0, 24 * 30)?.unwrap_or_else(|| old_int("notice_hours", 4));
    let days_ahead = int(&i.days_ahead, "how far ahead", 1, 60)?.unwrap_or_else(|| old_int("days_ahead", 30));
    let weekdays = match &i.weekdays {
        Some(v) if !v.is_null() => weekdays_of(v)?,
        _ => match base {
            Some(b) => weekdays_of(b.get("weekdays").unwrap_or(&Value::Null))?,
            None => vec![1, 2, 3, 4, 5],
        },
    };
    if weekdays.is_empty() {
        return Err(Error::msg("choose at least one day"));
    }
    let pick = |new: &Option<String>, f: &str, d: &str| -> String { new.clone().filter(|t| !t.trim().is_empty()).or_else(|| base.and_then(|b| text(b, f)).map(str::to_string)).unwrap_or_else(|| d.to_string()) };
    let from_time = clock("the start of the day", &pick(&i.from_time, "from_time", "09:00"))?;
    let to_time = clock("the end of the day", &pick(&i.to_time, "to_time", "17:00"))?;
    check_hours(&from_time, &to_time)?;
    fits(minutes, &from_time, &to_time)?;
    Ok(Fields { minutes, buffer, notice_hours, days_ahead, weekdays, from_time, to_time })
}

pub fn create_service(input: NewService) -> Result<Value> {
    let me = me()?;
    let count = db::find::<Record>("bk_service").filter("owner", me.as_str()).limit(100).all()?.len();
    if count >= MAX_SERVICES {
        return Err(Error::msg(format!("you can offer at most {MAX_SERVICES} kinds of meeting")));
    }
    let f = fields(&input, None)?;
    let service: Record = db::create(
        "bk_service",
        &json!({
            "owner": me, "title": clean("a title", &input.title, 120)?, "description": input.description.as_deref().map(str::trim), "location": input.location.as_deref().map(str::trim),
            "minutes": f.minutes, "buffer": f.buffer, "notice_hours": f.notice_hours, "days_ahead": f.days_ahead, "weekdays": f.weekdays, "from_time": f.from_time, "to_time": f.to_time, "active": true,
            "tz": zone_name(&input.tz, None)?, "public": false, "owner_name": my_name(&me)?,
        }),
    )?;
    Ok(json!({ "id": key(id_text(&service)) }))
}

#[derive(Deserialize)]
pub struct UpdateService {
    id: String,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    minutes: Option<Value>,
    #[serde(default)]
    buffer: Option<Value>,
    #[serde(default)]
    notice_hours: Option<Value>,
    #[serde(default)]
    days_ahead: Option<Value>,
    #[serde(default)]
    weekdays: Option<Value>,
    #[serde(default)]
    from_time: Option<String>,
    #[serde(default)]
    to_time: Option<String>,
    #[serde(default)]
    tz: Option<String>,
    /// Open to people outside the organization.
    #[serde(default)]
    public: Option<bool>,
}

pub fn update_service(input: UpdateService) -> Result<Value> {
    let service = mine(&input.id)?;
    let n = NewService { title: String::new(), description: None, location: None, minutes: input.minutes, buffer: input.buffer, notice_hours: input.notice_hours, days_ahead: input.days_ahead, weekdays: input.weekdays, from_time: input.from_time, to_time: input.to_time, tz: None };
    let f = fields(&n, Some(&service))?;
    let mut data = json!({ "minutes": f.minutes, "buffer": f.buffer, "notice_hours": f.notice_hours, "days_ahead": f.days_ahead, "weekdays": f.weekdays, "from_time": f.from_time, "to_time": f.to_time });
    if let Some(t) = input.title {
        data["title"] = json!(clean("a title", &t, 120)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(l) = input.location {
        data["location"] = json!(l.trim());
    }
    if let Some(a) = input.active {
        data["active"] = json!(a);
    }
    if input.tz.as_deref().is_some_and(|t| !t.trim().is_empty()) {
        data["tz"] = json!(zone_name(&input.tz, Some(&service))?);
    }
    if let Some(public) = input.public {
        data["public"] = json!(public);
        if public {
            data["link"] = json!(link_token()?);
            data["owner_name"] = json!(my_name(&service_owner(&service))?);
        }
    }
    db::update::<Record>("bk_service", key(id_text(&service)), &data)?;
    Ok(json!({ "ok": true }))
}

fn service_owner(service: &Record) -> String {
    text(service, "owner").unwrap_or_default().to_string()
}

pub fn delete_service(input: ServiceOnly) -> Result<Value> {
    let service = mine(&input.id)?;
    let now = now()?;
    let id = key(id_text(&service)).to_string();
    let rows: Vec<Record> = db::find("bk_booking").filter("service", id.as_str()).limit(5000).all()?;
    if rows.iter().any(|b| text(b, "status") == Some("confirmed") && text(b, "starts_at").unwrap_or_default() >= now.as_str()) {
        return Err(Error::msg("people have booked this: cancel the meetings first, or switch the service off"));
    }
    for b in rows {
        db::delete::<Record>("bk_booking", key(id_text(&b)))?;
    }
    db::delete::<Record>("bk_service", &id)?;
    Ok(json!({ "deleted": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_come_as_numbers_or_text() {
        assert_eq!(int(&Some(json!(30)), "x", 5, 60).unwrap(), Some(30));
        assert_eq!(int(&Some(json!(" 45 ")), "x", 5, 60).unwrap(), Some(45));
        assert_eq!(int(&Some(json!("")), "x", 5, 60).unwrap(), None);
        assert!(int(&Some(json!(2)), "x", 5, 60).is_err() && int(&Some(json!("abc")), "x", 5, 60).is_err());
    }

    #[test]
    fn weekdays_are_numbers_or_names_without_repeats() {
        assert_eq!(weekdays_of(&json!(["5", 1, "monday", "Tuesday"])).unwrap(), vec![1, 2, 5]);
        assert_eq!(weekdays_of(&json!("1, 3")).unwrap(), vec![1, 3]);
        assert!(weekdays_of(&json!(["8"])).is_err());
    }

    #[test]
    fn hours_must_fit_a_meeting() {
        assert!(check_hours("09:00", "09:00").is_err());
        assert!(fits(60, "09:00", "09:30").is_err());
        assert!(fits(30, "09:00", "09:30").is_ok());
        assert_eq!(clock("t", "9:05").unwrap(), "09:05");
    }
}
