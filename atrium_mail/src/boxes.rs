//! Boxes (the addresses mail comes to) and the people who work them.

use aether_sdk::prelude::*;

use crate::common::*;

pub const ADMIN_ROLE: &str = "org_admin";
const MAX_BOXES: u32 = 200;

fn s<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

fn is_admin() -> Result<bool> {
    Ok(context::current()?.has_role(ADMIN_ROLE))
}

fn must_admin() -> Result<()> {
    if is_admin()? { Ok(()) } else { Err(Error::msg("only an administrator can do this")) }
}

pub fn all_boxes() -> Result<Vec<Record>> {
    db::find("mail_box").limit(MAX_BOXES).all()
}

pub fn members_of(box_key: &str) -> Result<Vec<Record>> {
    db::find("mail_member").filter("box", key(box_key)).limit(500).all()
}

pub fn is_member(box_key: &str, user: &str) -> Result<bool> {
    Ok(members_of(box_key)?.iter().any(|m| text(m, "user").is_some_and(|u| key(u) == key(user))))
}

/// The box, if the caller works it. A box the caller is not in does not exist for them.
pub fn require_member(box_key: &str) -> Result<(Record, String)> {
    let me = me()?;
    let record = db::get::<Record>("mail_box", key(box_key))?.ok_or_else(|| Error::msg("there is no such box"))?;
    if !is_member(key(id_text(&record)), &me)? {
        return Err(Error::msg("there is no such box"));
    }
    Ok((record, me))
}

pub fn my_box_keys(me: &str) -> Result<Vec<String>> {
    let rows: Vec<Record> = db::find("mail_member").limit(10000).all()?;
    Ok(rows.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(me))).filter_map(|m| text(m, "box")).map(|b| key(b).to_string()).collect())
}

pub fn rule_my_boxes() -> Result<Vec<String>> {
    match context::current()?.actor.id {
        Some(me) => my_box_keys(&me),
        None => Ok(Vec::new()),
    }
}

pub fn tag_of(tag: &str) -> String {
    tag.trim().to_lowercase()
}

/// `inbox@` or `inbox+tag@`, where `domain` is what comes after the @ of the organization's mail address.
pub fn address(tag: &str, domain: &str) -> String {
    if tag.is_empty() { format!("inbox@{domain}") } else { format!("inbox+{tag}@{domain}") }
}

pub fn my_boxes() -> Result<Vec<Value>> {
    let me = me()?;
    let mine = my_box_keys(&me)?;
    let threads: Vec<Record> = db::find("mail_thread").filter("status", "open").limit(5000).all()?;
    let mut rows: Vec<Value> = Vec::new();
    for b in all_boxes()? {
        let k = key(id_text(&b)).to_string();
        if !mine.contains(&k) {
            continue;
        }
        let open = threads.iter().filter(|t| text(t, "box").is_some_and(|x| key(x) == k)).count();
        rows.push(json!({ "id": k, "name": s(&b, "name"), "tag": s(&b, "tag"), "to": address(s(&b, "tag"), "<mail domain>"), "people": members_of(&k)?.len(), "open": open }));
    }
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

pub fn box_options() -> Result<Vec<Value>> {
    Ok(my_boxes()?.into_iter().map(|b| json!({ "id": b["id"], "name": b["name"] })).collect())
}

pub fn mail_access() -> Result<Value> {
    Ok(json!({ "is_admin": is_admin()?, "boxes": my_box_keys(&me()?)?.len() }))
}

#[derive(Deserialize)]
pub struct NewBox {
    name: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    users: Vec<String>,
}

pub fn create_box(input: NewBox) -> Result<Value> {
    let me = me()?;
    must_admin()?;
    let tag = tag_of(input.tag.as_deref().unwrap_or_default());
    if !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') || tag.len() > 40 {
        return Err(Error::msg("a tag is letters, digits, - and _ only"));
    }
    if all_boxes()?.iter().any(|b| s(b, "tag") == tag) {
        return Err(Error::msg(if tag.is_empty() { "the box for plain inbox@ exists already; give this one a tag" } else { "a box has this tag already" }));
    }
    if all_boxes()?.len() as u32 >= MAX_BOXES {
        return Err(Error::msg("there are too many boxes"));
    }
    let record: Record = db::create("mail_box", &json!({ "name": clean("a name", &input.name, 120)?, "tag": tag }))?;
    let id = key(id_text(&record)).to_string();
    let people = org_people()?;
    let mut users = vec![me];
    for u in input.users {
        if people.iter().any(|p| key(&p.id) == key(&u)) && !users.iter().any(|x| key(x) == key(&u)) {
            users.push(u);
        }
    }
    for u in users {
        db::create::<Record>("mail_member", &json!({ "box": id, "user": u }))?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct BoxId {
    pub id: String,
}

pub fn delete_box(input: BoxId) -> Result<Value> {
    must_admin()?;
    let b = db::get::<Record>("mail_box", key(&input.id))?.ok_or_else(|| Error::msg("there is no such box"))?;
    let k = key(id_text(&b)).to_string();
    if !db::find::<Record>("mail_thread").filter("box", k.as_str()).limit(1).all()?.is_empty() {
        return Err(Error::msg("this box has conversations; close and keep them, or ask for them to be removed first"));
    }
    for m in members_of(&k)? {
        db::delete::<Record>("mail_member", key(id_text(&m)))?;
    }
    db::delete::<Record>("mail_box", &k)?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct BoxScope {
    #[serde(default)]
    box_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
}

pub fn box_members(input: BoxScope) -> Result<Vec<Value>> {
    let k = input.box_id.or(input.id).ok_or_else(|| Error::msg("which box?"))?;
    require_member(&k)?;
    let people = org_people()?;
    Ok(members_of(&k)?.iter().map(|m| json!({ "id": key(id_text(m)), "name": name_of(&people, s(m, "user")) })).collect())
}

#[derive(Deserialize)]
pub struct AddMembers {
    box_id: String,
    users: Vec<String>,
}

pub fn add_box_members(input: AddMembers) -> Result<Value> {
    must_admin()?;
    db::get::<Record>("mail_box", key(&input.box_id))?.ok_or_else(|| Error::msg("there is no such box"))?;
    let people = org_people()?;
    let mut added = 0;
    for u in input.users {
        if people.iter().any(|p| key(&p.id) == key(&u)) && !is_member(&input.box_id, &u)? {
            db::create::<Record>("mail_member", &json!({ "box": key(&input.box_id), "user": u }))?;
            added += 1;
        }
    }
    Ok(json!({ "added": added }))
}

pub fn remove_box_member(input: BoxId) -> Result<Value> {
    must_admin()?;
    let m = db::get::<Record>("mail_member", key(&input.id))?.ok_or_else(|| Error::msg("there is no such member"))?;
    db::delete::<Record>("mail_member", key(id_text(&m)))?;
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
    me()?;
    let needle = input.q.or(input.search).unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?.into_iter().filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle)).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}
