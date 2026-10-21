//! Calendars and the people in them.

use aether_sdk::prelude::*;

use crate::access::*;
use crate::common::*;

const MAX_MEMBERS: usize = 200;
const MAX_CALENDARS: usize = 50;

pub fn notify(user: &str, title: &str, link: &str) -> Result<()> {
    Notification::new(title.to_string()).link(link.to_string()).to(Audience::Actors(vec![user.to_string()])).send()?;
    Ok(())
}

/// Org members by full id; each one must belong to this organization.
pub fn check_users(users: &[String]) -> Result<Vec<String>> {
    let people = org_people()?;
    let mut out = Vec::new();
    for user in users {
        let full = people.iter().find(|p| key(&p.id) == key(user)).map(|p| p.id.clone()).ok_or_else(|| Error::msg("that person is not a member of this organization"))?;
        if !out.contains(&full) {
            out.push(full);
        }
    }
    Ok(out)
}

fn add_member(calendar: &str, user: &str, role: &str) -> Result<()> {
    if member_row(calendar, user)?.is_none() {
        db::create::<Record>("cal_member", &json!({ "calendar": key(calendar), "user": user, "role": role }))?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewCalendar {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    members: Vec<String>,
}

pub fn create_calendar(input: NewCalendar) -> Result<Value> {
    let me = me()?;
    ensure_personal(&me)?;
    if my_calendar_keys(&me)?.len() >= MAX_CALENDARS {
        return Err(Error::msg(format!("you can be in at most {MAX_CALENDARS} calendars")));
    }
    let name = clean("a name", &input.name, 120)?;
    let members = check_users(&input.members)?;
    let calendar: Record = db::create(
        "cal_calendar",
        &json!({ "name": name, "description": input.description.as_deref().map(str::trim), "color": input.color.as_deref().map(str::trim).filter(|c| !c.is_empty()).unwrap_or("#2563eb"), "owner": me, "personal": false }),
    )?;
    let id = key(id_text(&calendar)).to_string();
    add_member(&id, &me, "owner")?;
    let people = org_people()?;
    for u in members.iter().filter(|u| key(u) != key(&me)) {
        add_member(&id, u, "editor")?;
        notify(u, &format!("{} shared the calendar {name} with you", name_of(&people, &me)), &format!("/calendar/c/{id}"))?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct CalendarOnly {
    pub calendar: String,
}

#[derive(Deserialize)]
pub struct EditCalendar {
    calendar: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
}

pub fn edit_calendar(input: EditCalendar) -> Result<Value> {
    let (calendar, _, _) = require(&input.calendar, Role::Owner)?;
    let mut data = json!({});
    if let Some(n) = input.name {
        data["name"] = json!(clean("a name", &n, 120)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(c) = input.color {
        data["color"] = json!(c.trim());
    }
    db::update::<Record>("cal_calendar", key(id_text(&calendar)), &data)?;
    Ok(json!({ "ok": true }))
}

/// Delete a calendar with its events and people. Only the owner; a personal calendar stays.
pub fn delete_calendar(input: CalendarOnly) -> Result<Value> {
    let (calendar, _, _) = require(&input.calendar, Role::Owner)?;
    if flag(&calendar, "personal") {
        return Err(Error::msg("your own calendar cannot be deleted"));
    }
    let id = key(id_text(&calendar)).to_string();
    for event in db::find::<Record>("cal_event").filter("calendar", id.as_str()).limit(20000).all()? {
        crate::events::purge_event(&event)?;
    }
    for row in members_of(&id)? {
        db::delete::<Record>("cal_member", key(id_text(&row)))?;
    }
    db::delete::<Record>("cal_calendar", &id)?;
    Ok(json!({ "deleted": true }))
}

// ---- people -----------------------------------------------------------------------------------------------

pub fn calendar_members(input: CalendarOnly) -> Result<Vec<Value>> {
    let (calendar, _, _) = require(&input.calendar, Role::Viewer)?;
    let people = org_people()?;
    let mut rows: Vec<Value> = members_of(id_text(&calendar))?
        .iter()
        .map(|m| {
            let user = text(m, "user").unwrap_or_default();
            json!({ "id": user, "name": name_of(&people, user), "role": text(m, "role") })
        })
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

#[derive(Deserialize)]
pub struct AddMembers {
    calendar: String,
    users: Vec<String>,
    #[serde(default)]
    role: Option<String>,
}

pub fn add_calendar_members(input: AddMembers) -> Result<Value> {
    let (calendar, _, me) = require(&input.calendar, Role::Owner)?;
    let role = parse_role(input.role.as_deref().unwrap_or("editor"))?;
    let users = check_users(&input.users)?;
    let id = key(id_text(&calendar)).to_string();
    if members_of(&id)?.len() + users.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a calendar has at most {MAX_MEMBERS} people")));
    }
    let people = org_people()?;
    let mut added = 0;
    for u in &users {
        if member_row(&id, u)?.is_none() {
            add_member(&id, u, role_name(role))?;
            added += 1;
            notify(u, &format!("{} shared the calendar {} with you", name_of(&people, &me), text(&calendar, "name").unwrap_or("a calendar")), &format!("/calendar/c/{id}"))?;
        }
    }
    Ok(json!({ "added": added }))
}

#[derive(Deserialize)]
pub struct RemoveMember {
    calendar: String,
    id: String,
}

fn owners(calendar: &str) -> Result<usize> {
    Ok(members_of(calendar)?.iter().filter(|m| text(m, "role") == Some("owner")).count())
}

pub fn remove_calendar_member(input: RemoveMember) -> Result<Value> {
    let (calendar, _, me) = require(&input.calendar, Role::Viewer)?;
    // Anyone may leave; removing someone else takes the owner.
    if key(&input.id) != key(&me) {
        require(&input.calendar, Role::Owner)?;
    }
    let id = key(id_text(&calendar)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this calendar"))?;
    if text(&row, "role") == Some("owner") && owners(&id)? <= 1 {
        return Err(Error::msg("a calendar keeps at least one owner"));
    }
    if flag(&calendar, "personal") && text(&calendar, "owner").is_some_and(|o| key(o) == key(&input.id)) {
        return Err(Error::msg("people cannot leave their own calendar"));
    }
    db::delete::<Record>("cal_member", key(id_text(&row)))?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize)]
pub struct SetRole {
    calendar: String,
    id: String,
    role: String,
}

pub fn set_calendar_role(input: SetRole) -> Result<Value> {
    let (calendar, _, _) = require(&input.calendar, Role::Owner)?;
    let role = parse_role(&input.role)?;
    let id = key(id_text(&calendar)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this calendar"))?;
    if text(&row, "role") == Some("owner") && role != Role::Owner && owners(&id)? <= 1 {
        return Err(Error::msg("a calendar keeps at least one owner"));
    }
    db::update::<Record>("cal_member", key(id_text(&row)), &json!({ "role": role_name(role) }))?;
    Ok(json!({ "ok": true }))
}
