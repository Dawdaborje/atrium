//! Teams, channels, group and direct chats, and who belongs to them.

use std::collections::HashMap;

use aether_sdk::{db::Filter, prelude::*};

use crate::common::*;
use crate::people::{image_of, is_person, name_of, org_people, Person};

const MAX_MEMBERS: usize = 500;

// ---- reading --------------------------------------------------------------------------------------------

pub fn channel(id: &str) -> Result<Record> {
    let ch = records::require("chat_channel", key(id), "channel")?;
    Ok(ch)
}

pub fn member_of(channel: &str, user: &str) -> Result<Option<Record>> {
    db::find("chat_member").filter("channel", channel_id(channel)).filter("user", user).first()
}

pub fn members_of(channel: &str) -> Result<Vec<Record>> {
    db::find("chat_member").filter("channel", channel_id(channel)).limit(MAX_MEMBERS as u32).all()
}

pub fn is_open(ch: &Record) -> bool {
    text(ch, "kind") == Some("channel") && !flag(ch, "is_private") && !flag(ch, "archived")
}

/// May `user` read this channel: a member, or anyone when it is a public channel that is not archived.
pub fn can_read(ch: &Record, user: &str) -> Result<bool> {
    Ok(is_open(ch) || member_of(id_text(ch), user)?.is_some())
}

pub fn require_read(ch: &Record) -> Result<String> {
    let user = me()?;
    if can_read(ch, &user)? {
        Ok(user)
    } else {
        Err(Error::msg("you are not in this conversation"))
    }
}

/// May `user` manage the channel (rename, add and remove people, archive): its owner, or an administrator.
fn can_manage(ch: &Record, user: &str) -> Result<bool> {
    if is_admin()? {
        return Ok(true);
    }
    Ok(member_of(id_text(ch), user)?.is_some_and(|m| text(&m, "role") == Some("owner")))
}

/// The other person of a direct chat, from its key `a|b`.
fn other_of_direct(ch: &Record, me: &str) -> Option<String> {
    text(ch, "direct_key")?.split('|').find(|u| key(u) != key(me)).map(str::to_string)
}

fn display_name(ch: &Record, me: &str, people: &[Person], names: &HashMap<String, String>) -> String {
    match text(ch, "kind") {
        Some("direct") => other_of_direct(ch, me).map(|u| name_of(people, &u)).unwrap_or_else(|| "Direct message".into()),
        Some("group") if text(ch, "name").is_none_or(|n| n.is_empty()) => names.get(id_text(ch)).cloned().unwrap_or_else(|| "Group".into()),
        _ => text(ch, "name").unwrap_or("channel").to_string(),
    }
}

/// What a conversation is called for `me`: its name, the other person for a direct chat, or the people of a group.
pub fn title_of(ch: &Record, me: &str) -> Result<String> {
    let people = org_people()?;
    let mut names = HashMap::new();
    if text(ch, "kind") == Some("group") && text(ch, "name").is_none_or(str::is_empty) {
        let others: Vec<String> = members_of(id_text(ch))?
            .iter()
            .filter_map(|x| text(x, "user"))
            .filter(|u| key(u) != key(me))
            .map(|u| name_of(&people, u))
            .collect();
        names.insert(id_text(ch).to_string(), others.join(", "));
    }
    Ok(display_name(ch, me, &people, &names))
}

fn group_rank(group: &str) -> (u8, String) {
    match group {
        "Channels" => (1, String::new()),
        "Groups" => (2, String::new()),
        other => (0, other.to_lowercase()),
    }
}

// ---- the rail --------------------------------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct Browse {
    #[serde(default)]
    q: Option<String>,
}

/// What the rail lists: every conversation the caller is in, grouped by team, with unread counts.
pub fn my_rail() -> Result<Vec<Value>> {
    let user = me()?;
    let people = org_people()?;
    let memberships: Vec<Record> = db::find("chat_member").filter("user", user.as_str()).limit(1000).all()?;
    let teams: HashMap<String, String> = db::find::<Record>("chat_team")
        .limit(500)
        .all()?
        .iter()
        .map(|t| (key(id_text(t)).to_string(), text(t, "name").unwrap_or("Team").to_string()))
        .collect();
    let mentions: Vec<Record> = db::find("chat_mention").filter("user", user.as_str()).limit(2000).all()?;
    let mut rows = Vec::new();
    for m in &memberships {
        let Some(channel_ref) = text(m, "channel") else { continue };
        let Some(ch) = db::get::<Record>("chat_channel", key(channel_ref))? else { continue };
        if flag(&ch, "archived") {
            continue;
        }
        let kind = text(&ch, "kind").unwrap_or("channel");
        let group = match kind {
            "direct" => String::new(),
            "group" => "Groups".to_string(),
            _ => text(&ch, "team").and_then(|t| teams.get(key(t))).cloned().unwrap_or_else(|| "Channels".to_string()),
        };
        let mut names = HashMap::new();
        if kind == "group" && text(&ch, "name").is_none_or(|n| n.is_empty()) {
            let others: Vec<String> = members_of(id_text(&ch))?
                .iter()
                .filter_map(|x| text(x, "user"))
                .filter(|u| key(u) != key(&user))
                .map(|u| name_of(&people, u))
                .collect();
            names.insert(id_text(&ch).to_string(), others.join(", "));
        }
        let unread = (int(&ch, "last_n") - int(m, "last_read_n")).max(0);
        let mentioned = mentions
            .iter()
            .filter(|x| text(x, "channel").map(key) == Some(key(id_text(&ch))) && int(x, "n") > int(m, "last_read_n"))
            .count();
        let mut row = json!({
            "id": key(id_text(&ch)),
            "name": display_name(&ch, &user, &people, &names),
            "unread": unread,
            "mentions": mentioned,
            "muted": flag(m, "muted"),
            "kind": kind,
            "user": other_of_direct(&ch, &user),
        });
        if let Some(image) = other_of_direct(&ch, &user).and_then(|other| image_of(&people, &other)) {
            row["image"] = json!(image);
        }
        // Direct messages carry no group, so the rail lists them flat above the folding sections.
        if !group.is_empty() {
            row["group"] = json!(group);
        }
        rows.push(row);
    }
    rows.sort_by_key(|r| {
        let g = r["group"].as_str().unwrap_or_default();
        (group_rank(g), r["name"].as_str().unwrap_or_default().to_lowercase())
    });
    Ok(rows)
}

/// Public channels the caller has not joined.
fn browse(input: Browse) -> Result<Vec<Value>> {
    let user = me()?;
    let joined: Vec<String> = db::find::<Record>("chat_member")
        .filter("user", user.as_str())
        .limit(1000)
        .all()?
        .iter()
        .filter_map(|m| text(m, "channel").map(|c| key(c).to_string()))
        .collect();
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let all: Vec<Record> = db::find("chat_channel")
        .filter("kind", "channel")
        .filter("is_private", false)
        .filter("archived", false)
        .order_by("name")
        .limit(500)
        .all()?;
    Ok(all
        .iter()
        .filter(|c| !joined.iter().any(|j| j == key(id_text(c))))
        .filter(|c| needle.is_empty() || text(c, "name").unwrap_or_default().to_lowercase().contains(&needle))
        .map(|c| json!({ "id": key(id_text(c)), "name": text(c, "name"), "topic": text(c, "topic") }))
        .collect())
}

// ---- making things ----------------------------------------------------------------------------------------

pub fn add_member(channel: &str, user: &str, role: &str) -> Result<()> {
    if member_of(channel, user)?.is_some() {
        return Ok(());
    }
    let last = int(&channel_record(channel)?, "last_n");
    db::create::<Record>(
        "chat_member",
        &json!({ "channel": channel_id(channel), "user": user, "role": role, "last_read_n": last, "muted": false }),
    )?;
    refresh_rail(user, channel)?;
    Ok(())
}

/// Tell a person their list of conversations changed (they were added to one or removed from one), so their rail
/// asks again without a reload.
fn refresh_rail(user: &str, channel: &str) -> Result<()> {
    realtime::publish(&inbox_topic(user), "membership", &json!({ "channel": key(channel) }))?;
    Ok(())
}

fn channel_record(id: &str) -> Result<Record> {
    channel(id)
}

fn check_users(users: &[String]) -> Result<Vec<String>> {
    let people = org_people()?;
    let mut out: Vec<String> = Vec::new();
    for user in users {
        if !is_person(&people, user) {
            return Err(Error::msg(format!("`{user}` is not a member of this organization")));
        }
        let full = people.iter().find(|p| key(&p.id) == key(user)).map(|p| p.id.clone()).unwrap_or_else(|| user.clone());
        if !out.contains(&full) {
            out.push(full);
        }
    }
    if out.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a conversation has at most {MAX_MEMBERS} people")));
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewChannel {
    name: String,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    members: Vec<String>,
}

pub fn create_channel(input: NewChannel) -> Result<Value> {
    let user = me()?;
    let name = clean("a name", input.name.trim_start_matches('#'), 80)?;
    let team = input.team.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if let Some(team) = team {
        records::require("chat_team", key(team), "team")?;
    }
    let team_id = team.map(|t| key(t).to_string());
    let same: Vec<Record> = db::find("chat_channel").filter("kind", "channel").filter("name", name.as_str()).limit(50).all()?;
    if same.iter().any(|c| text(c, "team").map(str::to_string) == team_id) {
        return Err(Error::msg(format!("there is already a channel named {name}")));
    }
    let members = check_users(&input.members)?;
    let ch: Record = db::create(
        "chat_channel",
        &json!({
            "name": name, "team": team_id, "kind": "channel", "is_private": input.private,
            "topic": input.topic.as_deref().map(str::trim).filter(|t| !t.is_empty()), "archived": false, "last_n": 0, "created_by": user,
        }),
    )?;
    let id = id_text(&ch).to_string();
    add_member(&id, &user, "owner")?;
    for member in members.iter().filter(|m| key(m) != key(&user)) {
        add_member(&id, member, "member")?;
    }
    Ok(json!({ "id": key(&id) }))
}

#[derive(Deserialize)]
pub struct NewGroup {
    members: Vec<String>,
    #[serde(default)]
    name: Option<String>,
}

pub fn create_group(input: NewGroup) -> Result<Value> {
    let user = me()?;
    let members: Vec<String> = check_users(&input.members)?.into_iter().filter(|m| key(m) != key(&user)).collect();
    if members.len() < 2 {
        return Err(Error::msg("a group chat needs at least two other people; use a direct message for one"));
    }
    let ch: Record = db::create(
        "chat_channel",
        &json!({
            "name": input.name.as_deref().map(str::trim).filter(|n| !n.is_empty()), "kind": "group", "is_private": true,
            "archived": false, "last_n": 0, "created_by": user,
        }),
    )?;
    let id = id_text(&ch).to_string();
    add_member(&id, &user, "owner")?;
    for member in &members {
        add_member(&id, member, "member")?;
    }
    Ok(json!({ "id": key(&id) }))
}

#[derive(Deserialize)]
pub struct Direct {
    user: String,
}

/// A direct message with one person: the same pair always finds the same chat.
pub fn open_direct(input: Direct) -> Result<Value> {
    let user = me()?;
    let other = check_users(&[input.user])?.into_iter().next().ok_or_else(|| Error::msg("choose a person"))?;
    if key(&other) == key(&user) {
        return Err(Error::msg("you cannot message yourself here"));
    }
    let mut pair = [key(&user).to_string(), key(&other).to_string()];
    pair.sort();
    let direct_key = pair.join("|");
    if let Some(existing) = db::find::<Record>("chat_channel").filter("direct_key", direct_key.as_str()).first()? {
        add_member(id_text(&existing), &user, "member")?;
        return Ok(json!({ "id": key(id_text(&existing)) }));
    }
    let ch: Record = db::create(
        "chat_channel",
        &json!({ "name": null, "kind": "direct", "is_private": true, "archived": false, "last_n": 0, "direct_key": direct_key, "created_by": user }),
    )?;
    let id = id_text(&ch).to_string();
    add_member(&id, &user, "member")?;
    add_member(&id, &other, "member")?;
    Ok(json!({ "id": key(&id) }))
}

// ---- belonging ------------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct IdOnly {
    pub id: String,
}

#[derive(Deserialize)]
pub struct ChannelOnly {
    pub channel: String,
}

pub fn join(input: ChannelOnly) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if !is_open(&ch) {
        return Err(Error::msg("this conversation is not open to join; ask someone in it to add you"));
    }
    add_member(id_text(&ch), &user, "member")?;
    Ok(json!({ "joined": true }))
}

fn remove_member_row(ch: &Record, user: &str) -> Result<()> {
    // Compared by key: a list row's action sends the key, the live hub knows the full id.
    for row in members_of(id_text(ch))? {
        let stored = text(&row, "user").unwrap_or_default().to_string();
        if key(&stored) != key(user) {
            continue;
        }
        db::delete::<Record>("chat_member", key(id_text(&row)))?;
        realtime::drop_member(&channel_topic(id_text(ch)), &stored)?;
        refresh_rail(&stored, id_text(ch))?;
    }
    Ok(())
}

pub fn leave(input: ChannelOnly) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if text(&ch, "kind") == Some("direct") {
        return Err(Error::msg("a direct message cannot be left"));
    }
    remove_member_row(&ch, &user)?;
    Ok(json!({ "left": true }))
}

#[derive(Deserialize)]
pub struct People {
    channel: String,
    users: Vec<String>,
}

pub fn add_members(input: People) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    let kind = text(&ch, "kind").unwrap_or("channel");
    if kind == "direct" {
        return Err(Error::msg("people cannot be added to a direct message; start a group instead"));
    }
    // In a group anyone in it may add people; a channel is managed by its owner.
    let allowed = if kind == "group" { member_of(id_text(&ch), &user)?.is_some() || is_admin()? } else { can_manage(&ch, &user)? };
    if !allowed {
        return Err(Error::msg("only the owner of a channel can add people"));
    }
    let users = check_users(&input.users)?;
    if members_of(id_text(&ch))?.len() + users.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a conversation has at most {MAX_MEMBERS} people")));
    }
    for u in &users {
        add_member(id_text(&ch), u, "member")?;
    }
    Ok(json!({ "added": users.len() }))
}

#[derive(Deserialize)]
pub struct One {
    channel: String,
    /// The person; a list row's action sends it as `id`.
    #[serde(alias = "id")]
    user: String,
}

pub fn remove_member(input: One) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if text(&ch, "kind") == Some("direct") {
        return Err(Error::msg("nobody can be removed from a direct message"));
    }
    if key(&input.user) != key(&user) && !can_manage(&ch, &user)? {
        return Err(Error::msg("only the owner of a channel can remove people"));
    }
    remove_member_row(&ch, &input.user)?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize)]
pub struct Edit {
    channel: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    archived: Option<bool>,
}

pub fn edit_channel(input: Edit) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if !can_manage(&ch, &user)? {
        return Err(Error::msg("only the owner of a channel can change it"));
    }
    let mut data = json!({});
    if let Some(name) = input.name {
        data["name"] = json!(clean("a name", &name, 80)?);
    }
    if let Some(topic) = input.topic {
        data["topic"] = json!(topic.trim());
    }
    if let Some(archived) = input.archived {
        data["archived"] = json!(archived);
    }
    db::update::<Record>("chat_channel", key(id_text(&ch)), &data)?;
    Ok(json!({ "ok": true }))
}

pub fn mark_read(input: ChannelOnly) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if let Some(m) = member_of(id_text(&ch), &user)? {
        db::update::<Record>("chat_member", key(id_text(&m)), &json!({ "last_read_n": int(&ch, "last_n") }))?;
        clear_mentions(id_text(&ch), &user)?;
    }
    Ok(json!({ "ok": true }))
}

/// Reading a channel clears the marks of the mentions in it.
pub fn clear_mentions(channel: &str, user: &str) -> Result<()> {
    for row in db::find::<Record>("chat_mention").filter("channel", key(channel)).filter("user", user).limit(500).all()? {
        db::delete::<Record>("chat_mention", key(id_text(&row)))?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewTeam {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

pub fn create_team(input: NewTeam) -> Result<Value> {
    if !is_admin()? {
        return Err(Error::msg("only an administrator makes teams"));
    }
    let name = clean("a name", &input.name, 80)?;
    if db::find::<Record>("chat_team").filter("name", name.as_str()).first()?.is_some() {
        return Err(Error::msg(format!("there is already a team named {name}")));
    }
    let team: Record = db::create(
        "chat_team",
        &json!({ "name": name, "description": input.description.as_deref().map(str::trim), "created_by": me()? }),
    )?;
    Ok(json!({ "id": key(id_text(&team)) }))
}

pub fn browse_channels(input: Browse) -> Result<Vec<Value>> {
    browse(input)
}

pub fn list_teams() -> Result<Vec<Value>> {
    let teams: Vec<Record> = db::find("chat_team").order_by("name").limit(500).all()?;
    Ok(teams.iter().map(|t| json!({ "id": key(id_text(t)), "name": text(t, "name") })).collect())
}

/// Details for a channel header: the channel, its people and the caller's place in it.
pub fn info(input: ChannelOnly) -> Result<Value> {
    let ch = channel(&input.channel)?;
    let user = require_read(&ch)?;
    let people = org_people()?;
    let members = members_of(id_text(&ch))?;
    let mine = members.iter().find(|m| text(m, "user").is_some_and(|u| key(u) == key(&user)));
    Ok(json!({
        "id": key(id_text(&ch)),
        "kind": text(&ch, "kind"),
        "name": text(&ch, "name"),
        "topic": text(&ch, "topic"),
        "private": flag(&ch, "is_private"),
        "archived": flag(&ch, "archived"),
        "members": members.iter().map(|m| {
            let u = text(m, "user").unwrap_or_default();
            json!({ "id": u, "name": name_of(&people, u), "role": text(m, "role") })
        }).collect::<Vec<_>>(),
        "joined": mine.is_some(),
        "role": mine.and_then(|m| text(m, "role")),
    }))
}

/// The starter channel every organization gets.
pub fn seed() -> Result<Value> {
    if db::find::<Record>("chat_channel").filter("kind", "channel").filter("name", "general").first()?.is_none() {
        db::create::<Record>(
            "chat_channel",
            &json!({ "name": "general", "kind": "channel", "is_private": false, "topic": "Everyone is here", "archived": false, "last_n": 0, "created_by": "system" }),
        )?;
    }
    Ok(json!({ "ok": true }))
}

#[allow(dead_code)]
fn unused(_: Filter) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_sort_ungrouped_direct_messages_first_then_teams_channels_and_groups() {
        let mut groups = vec!["Groups", "Channels", "Sales", "", "Ops"];
        groups.sort_by_key(|g| group_rank(g));
        assert_eq!(groups, ["", "Ops", "Sales", "Channels", "Groups"]);
    }

    #[test]
    fn the_other_person_of_a_direct_chat_comes_from_its_key() {
        let ch = json!({ "direct_key": "ann|bob" });
        assert_eq!(other_of_direct(&ch, "users:ann").as_deref(), Some("bob"));
        assert_eq!(other_of_direct(&ch, "bob").as_deref(), Some("ann"));
        assert_eq!(other_of_direct(&json!({}), "ann"), None);
    }

    #[test]
    fn only_a_public_channel_that_is_not_archived_is_open() {
        assert!(is_open(&json!({ "kind": "channel", "is_private": false })));
        assert!(!is_open(&json!({ "kind": "channel", "is_private": true })));
        assert!(!is_open(&json!({ "kind": "group", "is_private": false })));
        assert!(!is_open(&json!({ "kind": "channel", "archived": true })));
    }
}

/// The people of a conversation, for the members page: rows with `id` (the user) for row actions.
pub fn members_list(input: ChannelOnly) -> Result<Vec<Value>> {
    let ch = channel(&input.channel)?;
    require_read(&ch)?;
    let people = org_people()?;
    let mut rows: Vec<Value> = members_of(id_text(&ch))?
        .iter()
        .map(|m| {
            let user = text(m, "user").unwrap_or_default();
            json!({ "id": user, "name": name_of(&people, user), "role": text(m, "role"), "muted": flag(m, "muted") })
        })
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

/// What the heading of a conversation shows.
pub fn header(input: ChannelOnly) -> Result<Value> {
    let ch = channel(&input.channel)?;
    let user = require_read(&ch)?;
    let kind = text(&ch, "kind").unwrap_or("channel");
    let members = members_of(id_text(&ch))?.len();
    let subtitle = match kind {
        "direct" => "Direct message".to_string(),
        _ => {
            let topic = text(&ch, "topic").unwrap_or_default();
            let count = format!("{members} {}", if members == 1 { "person" } else { "people" });
            if topic.is_empty() { count } else { format!("{topic} · {count}") }
        }
    };
    let title = title_of(&ch, &user)?;
    Ok(json!({
        "title": if kind == "channel" { format!("# {title}") } else { title },
        "subtitle": subtitle,
        "kind": kind,
        "archived": flag(&ch, "archived"),
        "joined": member_of(id_text(&ch), &user)?.is_some(),
    }))
}

/// The caller's own settings in a conversation, for the notifications page.
pub fn my_membership(input: ChannelOnly) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    let m = member_of(id_text(&ch), &user)?.ok_or_else(|| Error::msg("you are not in this conversation"))?;
    Ok(json!({ "channel": key(id_text(&ch)), "muted": flag(&m, "muted") }))
}

/// Mute or unmute a conversation for the caller.
pub fn set_muted(input: MuteIn) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    let m = member_of(id_text(&ch), &user)?.ok_or_else(|| Error::msg("you are not in this conversation"))?;
    db::update::<Record>("chat_member", key(id_text(&m)), &json!({ "muted": input.muted }))?;
    Ok(json!({ "muted": input.muted }))
}

#[derive(Deserialize)]
pub struct MuteIn {
    channel: String,
    muted: bool,
}

// ---- for other plugins ------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct Ensure {
    name: String,
    #[serde(default)]
    team: Option<String>,
    #[serde(default)]
    topic: Option<String>,
    #[serde(default)]
    private: bool,
    #[serde(default)]
    members: Vec<String>,
}

fn ensure_team_named(name: &str) -> Result<String> {
    let name = clean("a team name", name, 80)?;
    if let Some(team) = db::find::<Record>("chat_team").filter("name", name.as_str()).first()? {
        return Ok(key(id_text(&team)).to_string());
    }
    let team: Record = db::create("chat_team", &json!({ "name": name, "created_by": actor_label() }))?;
    Ok(key(id_text(&team)).to_string())
}

/// Find or make a channel by name (inside a team, made by name too) and add people to it; nobody is removed. For
/// plugins that keep channels in step with something of their own (a department, a project).
pub fn ensure_channel(input: Ensure) -> Result<Value> {
    require_trusted()?;
    let name = clean("a name", &input.name, 80)?;
    let team = match input.team.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => Some(ensure_team_named(t)?),
        None => None,
    };
    let same: Vec<Record> = db::find("chat_channel").filter("kind", "channel").filter("name", name.as_str()).limit(50).all()?;
    let found = same.into_iter().find(|c| text(c, "team").map(str::to_string) == team);
    let (id, created) = match found {
        Some(c) => (id_text(&c).to_string(), false),
        None => {
            let c: Record = db::create(
                "chat_channel",
                &json!({
                    "name": name, "team": team, "kind": "channel", "is_private": input.private, "topic": input.topic,
                    "archived": false, "last_n": 0, "created_by": actor_label(),
                }),
            )?;
            (id_text(&c).to_string(), true)
        }
    };
    let users = check_users(&input.members)?;
    let before = members_of(&id)?.len();
    for u in &users {
        add_member(&id, u, "member")?;
    }
    Ok(json!({ "id": key(&id), "created": created, "added": members_of(&id)?.len() - before }))
}

#[derive(Deserialize)]
pub struct Sync {
    channel: String,
    users: Vec<String>,
    #[serde(default)]
    remove_others: bool,
}

/// Make a channel's people `users`: add the missing ones and, if asked, remove the others.
pub fn sync_members(input: Sync) -> Result<Value> {
    require_trusted()?;
    let ch = channel(&input.channel)?;
    let users = check_users(&input.users)?;
    let mut added = 0;
    for u in &users {
        if member_of(id_text(&ch), u)?.is_none() {
            add_member(id_text(&ch), u, "member")?;
            added += 1;
        }
    }
    let mut removed = 0;
    if input.remove_others {
        for m in members_of(id_text(&ch))? {
            let user = text(&m, "user").unwrap_or_default().to_string();
            if !users.iter().any(|u| key(u) == key(&user)) && text(&m, "role") != Some("owner") {
                remove_member_row(&ch, &user)?;
                removed += 1;
            }
        }
    }
    Ok(json!({ "added": added, "removed": removed }))
}

/// Take a person out of every conversation that is not a direct message or private (they left the organization).
pub fn remove_everywhere(input: UserOnly) -> Result<Value> {
    require_trusted()?;
    let mut removed = 0;
    for m in db::find::<Record>("chat_member").filter("user", input.user.as_str()).limit(2000).all()? {
        let Some(ch) = db::get::<Record>("chat_channel", key(text(&m, "channel").unwrap_or_default()))? else { continue };
        if text(&ch, "kind") == Some("direct") {
            continue;
        }
        remove_member_row(&ch, &input.user)?;
        removed += 1;
    }
    Ok(json!({ "removed": removed }))
}

#[derive(Deserialize)]
pub struct UserOnly {
    user: String,
}
