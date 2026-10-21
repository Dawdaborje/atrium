//! Spaces and the people in them.

use aether_sdk::prelude::*;

use crate::access::*;
use crate::common::*;

const MAX_MEMBERS: usize = 300;
const MAX_SPACES: usize = 100;

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

fn add_member(space: &str, user: &str, role: &str) -> Result<()> {
    if member_row(space, user)?.is_none() {
        db::create::<Record>("wiki_member", &json!({ "space": key(space), "user": user, "role": role }))?;
    }
    Ok(())
}

pub fn pages_of(space: &str) -> Result<Vec<Record>> {
    db::find("wiki_page").filter("space", key(space)).limit(5000).all()
}

#[derive(Deserialize, Default)]
pub struct Search {
    #[serde(default)]
    pub q: Option<String>,
}

pub fn my_spaces(input: Search) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let mut out = Vec::new();
    for id in my_space_keys(&me)? {
        let Some(s) = db::get::<Record>("wiki_space", &id)? else { continue };
        let name = text(&s, "name").unwrap_or_default();
        if !needle.is_empty() && !name.to_lowercase().contains(&needle) && !text(&s, "description").unwrap_or_default().to_lowercase().contains(&needle) {
            continue;
        }
        let role = role_of(&id, &me)?;
        out.push(json!({
            "id": id, "name": name, "description": text(&s, "description"), "open": flag(&s, "open"), "role": role.map(role_name).unwrap_or("reader"),
            "member": role.is_some(), "pages": pages_of(&id)?.len(), "people": members_of(&id)?.len(),
        }));
    }
    out.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewSpace {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    open: bool,
    #[serde(default)]
    members: Vec<String>,
}

pub fn create_space(input: NewSpace) -> Result<Value> {
    let me = me()?;
    let mine = db::find::<Record>("wiki_member").filter("user", me.as_str()).limit(1000).all()?.len();
    if mine >= MAX_SPACES {
        return Err(Error::msg(format!("you can be in at most {MAX_SPACES} spaces")));
    }
    let name = clean("a name", &input.name, 120)?;
    let members = check_users(&input.members)?;
    let space: Record = db::create("wiki_space", &json!({ "name": name, "description": input.description.as_deref().map(str::trim), "owner": me, "open": input.open }))?;
    let id = key(id_text(&space)).to_string();
    add_member(&id, &me, "owner")?;
    let people = org_people()?;
    for u in members.iter().filter(|u| key(u) != key(&me)) {
        add_member(&id, u, "editor")?;
        notify(u, &format!("{} added you to the space {name}", name_of(&people, &me)), &format!("/wiki/s/{id}"))?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct SpaceOnly {
    pub space: String,
}

pub fn space_header(input: SpaceOnly) -> Result<Value> {
    let (space, role, _) = require(&input.space, Role::Reader)?;
    Ok(json!({ "title": text(&space, "name"), "subtitle": format!("{} pages · you are {}", pages_of(id_text(&space))?.len(), role_name(role)), "can_edit": role >= Role::Editor, "can_manage": role == Role::Owner }))
}

pub fn space_info(input: SpaceOnly) -> Result<Value> {
    let (space, role, _) = require(&input.space, Role::Reader)?;
    Ok(json!({ "id": key(id_text(&space)), "name": text(&space, "name"), "description": text(&space, "description"), "open": flag(&space, "open"), "role": role_name(role) }))
}

#[derive(Deserialize)]
pub struct EditSpace {
    space: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    open: Option<bool>,
}

pub fn edit_space(input: EditSpace) -> Result<Value> {
    let (space, _, _) = require(&input.space, Role::Owner)?;
    let mut data = json!({});
    if let Some(n) = input.name {
        data["name"] = json!(clean("a name", &n, 120)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(o) = input.open {
        data["open"] = json!(o);
    }
    db::update::<Record>("wiki_space", key(id_text(&space)), &data)?;
    Ok(json!({ "ok": true }))
}

pub fn delete_space(input: SpaceOnly) -> Result<Value> {
    let (space, _, _) = require(&input.space, Role::Owner)?;
    let id = key(id_text(&space)).to_string();
    for page in pages_of(&id)? {
        db::delete::<Record>("wiki_page", key(id_text(&page)))?;
    }
    for row in members_of(&id)? {
        db::delete::<Record>("wiki_member", key(id_text(&row)))?;
    }
    db::delete::<Record>("wiki_space", &id)?;
    Ok(json!({ "deleted": true }))
}

// ---- people -----------------------------------------------------------------------------------------------

pub fn space_members(input: SpaceOnly) -> Result<Vec<Value>> {
    let (space, _, _) = require(&input.space, Role::Reader)?;
    let people = org_people()?;
    let mut rows: Vec<Value> = members_of(id_text(&space))?
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
    space: String,
    users: Vec<String>,
    #[serde(default)]
    role: Option<String>,
}

pub fn add_space_members(input: AddMembers) -> Result<Value> {
    let (space, _, me) = require(&input.space, Role::Owner)?;
    let role = parse_role(input.role.as_deref().unwrap_or("editor"))?;
    let users = check_users(&input.users)?;
    let id = key(id_text(&space)).to_string();
    if members_of(&id)?.len() + users.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a space has at most {MAX_MEMBERS} people")));
    }
    let people = org_people()?;
    let mut added = 0;
    for u in &users {
        if member_row(&id, u)?.is_none() {
            add_member(&id, u, role_name(role))?;
            added += 1;
            notify(u, &format!("{} added you to the space {}", name_of(&people, &me), text(&space, "name").unwrap_or("a space")), &format!("/wiki/s/{id}"))?;
        }
    }
    Ok(json!({ "added": added }))
}

#[derive(Deserialize)]
pub struct RemoveMember {
    space: String,
    id: String,
}

fn owners(space: &str) -> Result<usize> {
    Ok(members_of(space)?.iter().filter(|m| text(m, "role") == Some("owner")).count())
}

pub fn remove_space_member(input: RemoveMember) -> Result<Value> {
    let (space, _, me) = require(&input.space, Role::Reader)?;
    if key(&input.id) != key(&me) {
        require(&input.space, Role::Owner)?;
    }
    let id = key(id_text(&space)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this space"))?;
    if text(&row, "role") == Some("owner") && owners(&id)? <= 1 {
        return Err(Error::msg("a space keeps at least one owner"));
    }
    db::delete::<Record>("wiki_member", key(id_text(&row)))?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize)]
pub struct SetRole {
    space: String,
    id: String,
    role: String,
}

pub fn set_space_role(input: SetRole) -> Result<Value> {
    let (space, _, _) = require(&input.space, Role::Owner)?;
    let role = parse_role(&input.role)?;
    let id = key(id_text(&space)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this space"))?;
    if text(&row, "role") == Some("owner") && role != Role::Owner && owners(&id)? <= 1 {
        return Err(Error::msg("a space keeps at least one owner"));
    }
    db::update::<Record>("wiki_member", key(id_text(&row)), &json!({ "role": role_name(role) }))?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize, Default)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    search: Option<String>,
}

pub fn pick_other_people(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.or(input.search).unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?.into_iter().filter(|p| key(&p.id) != key(&me) && (needle.is_empty() || p.name.to_lowercase().contains(&needle))).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}

/// For record rules: the keys of the spaces the caller reads, so they see the conversations of those pages.
pub fn rule_my_spaces() -> Result<Vec<String>> {
    match context::current()?.actor.id {
        Some(me) => my_space_keys(&me),
        None => Ok(Vec::new()),
    }
}
