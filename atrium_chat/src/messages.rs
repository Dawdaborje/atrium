//! Posting, editing, deleting and reading messages, and threads.

use aether_sdk::{db::Filter, prelude::*};

use crate::channels::{add_member, can_read, channel, is_open, member_of, members_of, require_read};
use crate::common::*;
use crate::people::{find_mentions, name_of, org_people, Person};

const MAX_BODY: usize = 8000;
const MAX_FILES: usize = 10;
const MAX_FANOUT: usize = 200;

#[derive(Deserialize)]
pub struct Post {
    channel: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    files: Option<Vec<Value>>,
    #[serde(default)]
    parent: Option<String>,
    /// The page's query values; `thread` is the message being replied to.
    #[serde(default)]
    query: Option<Value>,
}

/// Attached files must be what the media store returned: a key under `media/`, a name, a size.
fn check_files(files: &[Value]) -> Result<Vec<Value>> {
    if files.len() > MAX_FILES {
        return Err(Error::msg(format!("a message has at most {MAX_FILES} files")));
    }
    files
        .iter()
        .map(|f| {
            let key = f.get("key").and_then(Value::as_str).unwrap_or_default();
            let name = f.get("name").and_then(Value::as_str).unwrap_or_default();
            if !key.starts_with("media/") || key.contains("..") || name.is_empty() {
                return Err(Error::msg("an attached file was not uploaded through the media store"));
            }
            Ok(json!({
                "key": key, "name": name.chars().take(200).collect::<String>(),
                "mime": f.get("mime").and_then(Value::as_str).unwrap_or_default(),
                "size": f.get("size").and_then(Value::as_u64).unwrap_or(0),
            }))
        })
        .collect()
}

fn publish(topic: &str, event: &str, payload: &Value) -> Result<()> {
    realtime::publish(topic, event, payload)?;
    Ok(())
}

/// Tell each person in the channel, on their own inbox topic, that something happened (their rail refreshes).
fn nudge_members(channel: &str, author: &str, n: i64) -> Result<()> {
    for m in members_of(channel)?.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) != key(author))).take(MAX_FANOUT) {
        if flag(m, "muted") {
            continue;
        }
        if let Some(user) = text(m, "user") {
            publish(&inbox_topic(user), "activity", &json!({ "channel": key(channel), "n": n }))?;
        }
    }
    Ok(())
}

pub fn post(input: Post) -> Result<Value> {
    let user = me()?;
    let ch = channel(&input.channel)?;
    if flag(&ch, "archived") {
        return Err(Error::msg("this channel is archived"));
    }
    if member_of(id_text(&ch), &user)?.is_none() {
        if is_open(&ch) {
            add_member(id_text(&ch), &user, "member")?;
        } else {
            return Err(Error::msg("you are not in this conversation"));
        }
    }
    let body = input.body.as_deref().map(str::trim).unwrap_or_default().to_string();
    if body.chars().count() > MAX_BODY {
        return Err(Error::msg(format!("a message is at most {MAX_BODY} characters")));
    }
    let files = check_files(&input.files.unwrap_or_default())?;
    if body.is_empty() && files.is_empty() {
        return Err(Error::msg("write something or attach a file"));
    }
    let parent_key = input
        .parent
        .or_else(|| input.query.as_ref().and_then(|q| q.get("thread")).and_then(Value::as_str).map(str::to_string))
        .map(|p| key(&p).to_string())
        .filter(|p| !p.is_empty());
    let parent = match &parent_key {
        Some(p) => {
            let msg = records::require("chat_message", p, "message")?;
            if text(&msg, "channel").map(key) != Some(key(id_text(&ch))) || text(&msg, "parent").is_some_and(|x| !x.is_empty()) || flag(&msg, "deleted") {
                return Err(Error::msg("you can only reply to a message of this channel"));
            }
            Some(msg)
        }
        None => None,
    };
    let updated: Record = db::increment("chat_channel", key(id_text(&ch)), "last_n", 1.0)?.ok_or_else(|| Error::msg("the channel is gone"))?;
    let n = int(&updated, "last_n");
    let people = org_people()?;
    // Who is mentioned: the people written as @Name, or everyone in the channel for @channel; only those who can read it.
    let (named, everyone) = find_mentions(&body, &people);
    let mut targets: Vec<Person> = named.into_iter().cloned().collect();
    if everyone {
        for m in members_of(id_text(&ch))?.iter().take(MAX_FANOUT) {
            if let Some(u) = text(m, "user") {
                if let Some(p) = people.iter().find(|p| key(&p.id) == key(u)) {
                    if !targets.iter().any(|t| t.id == p.id) {
                        targets.push(p.clone());
                    }
                }
            }
        }
    }
    let mut mentioned: Vec<Person> = Vec::new();
    for person in targets.into_iter().filter(|p| key(&p.id) != key(&user)) {
        if can_read(&ch, &person.id)? {
            mentioned.push(person);
        }
    }
    let row: Record = db::create(
        "chat_message",
        &json!({
            "channel": channel_id(id_text(&ch)), "author": user, "author_name": name_of(&people, &user), "body": body, "n": n,
            "parent": parent_key, "reply_count": 0, "at": now()?, "files": if files.is_empty() { Value::Null } else { json!(files) }, "deleted": false,
            "mentions": mentioned.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            "mention_names": mentioned.iter().map(|p| p.name.clone()).collect::<Vec<_>>(),
        }),
    )?;
    let channel_key = key(id_text(&ch)).to_string();
    match (&parent, &parent_key) {
        (Some(_), Some(p)) => {
            publish(&thread_topic(p), "message", &json!({ "row": row }))?;
            if let Some(updated_parent) = db::increment::<Record>("chat_message", p, "reply_count", 1.0)? {
                publish(&channel_topic(&channel_key), "message", &json!({ "row": updated_parent }))?;
            }
        }
        _ => publish(&channel_topic(&channel_key), "message", &json!({ "row": row }))?,
    }
    nudge_members(&channel_key, &user, n)?;
    notify_mentions(&ch, &row, &user, &name_of(&people, &user), &mentioned, n)?;
    if let Some(mine) = member_of(&channel_key, &user)? {
        db::update::<Record>("chat_member", key(id_text(&mine)), &json!({ "last_read_n": n }))?;
    }
    events::emit(
        "message_posted",
        &json!({ "channel": channel_key, "message": key(id_text(&row)), "author": user, "reply": parent_key.is_some() }),
    )?;
    Ok(json!({ "row": row, "id": key(id_text(&row)), "n": n }))
}

/// A mentioned person is told even when they muted the channel: a mark on their rail, and a notification.
fn notify_mentions(ch: &Record, row: &Record, author: &str, author_name: &str, mentioned: &[Person], n: i64) -> Result<()> {
    let channel_key = key(id_text(ch)).to_string();
    let place = match text(ch, "kind") {
        Some("direct") => "a direct message".to_string(),
        Some("group") => "a group chat".to_string(),
        _ => format!("#{}", text(ch, "name").unwrap_or("channel")),
    };
    for person in mentioned.iter().filter(|p| key(&p.id) != key(author)) {
        db::create::<Record>(
            "chat_mention",
            &json!({ "channel": channel_key, "user": person.id, "message": key(id_text(row)), "n": n }),
        )?;
        publish(&inbox_topic(&person.id), "activity", &json!({ "channel": channel_key, "n": n, "mention": true }))?;
        let snippet: String = text(row, "body").unwrap_or_default().chars().take(140).collect();
        Notification::new(format!("{author_name} mentioned you in {place}"))
            .body(snippet)
            .link(format!("/chat/{channel_key}"))
            .to(Audience::Actors(vec![person.id.clone()]))
            .send()?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct EditIn {
    id: String,
    body: String,
}

fn load_message(id: &str) -> Result<(Record, Record)> {
    let msg = records::require("chat_message", key(id), "message")?;
    let ch = channel(text(&msg, "channel").unwrap_or_default())?;
    Ok((msg, ch))
}

/// Publish a changed message where it is shown: its thread, or the channel (and the channel too for a reply).
fn publish_row(msg: &Record, ch: &Record, event: &str, payload: &Value) -> Result<()> {
    match text(msg, "parent").filter(|p| !p.is_empty()) {
        Some(parent) => publish(&thread_topic(parent), event, payload),
        None => publish(&channel_topic(id_text(ch)), event, payload),
    }
}

pub fn edit(input: EditIn) -> Result<Value> {
    let user = me()?;
    let (msg, ch) = load_message(&input.id)?;
    if text(&msg, "author").is_none_or(|a| key(a) != key(&user)) {
        return Err(Error::msg("you can only edit your own messages"));
    }
    if flag(&msg, "deleted") {
        return Err(Error::msg("this message was deleted"));
    }
    let body = clean("a message", &input.body, MAX_BODY)?;
    let row: Record = db::update("chat_message", key(id_text(&msg)), &json!({ "body": body, "edited_at": now()? }))?.ok_or_else(|| Error::msg("the message is gone"))?;
    publish_row(&msg, &ch, "message", &json!({ "row": row }))?;
    Ok(json!({ "row": row }))
}

pub fn delete(input: crate::channels::IdOnly) -> Result<Value> {
    let user = me()?;
    let (msg, ch) = load_message(&input.id)?;
    let own = text(&msg, "author").is_some_and(|a| key(a) == key(&user));
    let owner = member_of(id_text(&ch), &user)?.is_some_and(|m| text(&m, "role") == Some("owner"));
    if !(own || owner || is_admin()?) {
        return Err(Error::msg("you can only delete your own messages"));
    }
    db::update::<Record>("chat_message", key(id_text(&msg)), &json!({ "deleted": true, "body": "", "files": Value::Null }))?;
    publish_row(&msg, &ch, "deleted", &json!({ "id": key(id_text(&msg)) }))?;
    if let Some(parent) = text(&msg, "parent").filter(|p| !p.is_empty()) {
        if let Some(updated) = db::increment::<Record>("chat_message", parent, "reply_count", -1.0)? {
            publish(&channel_topic(id_text(&ch)), "message", &json!({ "row": updated }))?;
        }
    }
    Ok(json!({ "deleted": true }))
}

#[derive(Deserialize)]
pub struct List {
    channel: String,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    before: Option<String>,
}

fn page_size(limit: Option<u32>) -> u32 {
    limit.unwrap_or(50).clamp(1, 200)
}

/// Rows newest first from the database, answered oldest first, with whether there may be older ones.
fn answer(mut rows: Vec<Record>, limit: u32) -> Value {
    let more = rows.len() as u32 == limit;
    rows.reverse();
    json!({ "rows": rows, "more": more })
}

pub fn list(input: List) -> Result<Value> {
    let ch = channel(&input.channel)?;
    let user = require_read(&ch)?;
    let limit = page_size(input.limit);
    let mut filter = Filter::eq("channel", channel_id(id_text(&ch))).and(Filter::eq("deleted", false)).and(Filter::is_empty("parent"));
    if let Some(before) = input.before.as_deref().filter(|b| !b.is_empty()) {
        let pivot = records::require("chat_message", key(before), "message")?;
        filter = filter.and(Filter::lt("n", int(&pivot, "n")));
    }
    let rows: Vec<Record> = db::find("chat_message").matching(filter).order_by("-n").limit(limit).all()?;
    // Opening the newest page is reading it.
    if input.before.as_deref().is_none_or(str::is_empty) {
        if let Some(m) = member_of(id_text(&ch), &user)? {
            db::update::<Record>("chat_member", key(id_text(&m)), &json!({ "last_read_n": int(&ch, "last_n") }))?;
            crate::channels::clear_mentions(id_text(&ch), &user)?;
        }
    }
    Ok(answer(rows, limit))
}

#[derive(Deserialize)]
pub struct ThreadIn {
    #[serde(default)]
    thread: Option<String>,
    /// The page's query values; `thread` is the message whose replies are shown.
    #[serde(default)]
    query: Option<Value>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    before: Option<String>,
}

/// A message and its replies, oldest first.
pub fn thread(input: ThreadIn) -> Result<Value> {
    let thread = input
        .thread
        .or_else(|| input.query.as_ref().and_then(|q| q.get("thread")).and_then(Value::as_str).map(str::to_string))
        .filter(|t| !t.is_empty())
        .ok_or_else(|| Error::msg("choose a thread"))?;
    let root = records::require("chat_message", key(&thread), "message")?;
    let ch = channel(text(&root, "channel").unwrap_or_default())?;
    require_read(&ch)?;
    let limit = page_size(input.limit);
    let mut filter = Filter::eq("parent", key(&thread)).and(Filter::eq("deleted", false));
    if let Some(before) = input.before.as_deref().filter(|b| !b.is_empty()) {
        let pivot = records::require("chat_message", key(before), "message")?;
        filter = filter.and(Filter::lt("n", int(&pivot, "n")));
    }
    let replies: Vec<Record> = db::find("chat_message").matching(filter).order_by("-n").limit(limit).all()?;
    let mut out = answer(replies, limit);
    if input.before.as_deref().is_none_or(str::is_empty) && !flag(&root, "deleted") {
        if let Some(rows) = out["rows"].as_array_mut() {
            rows.insert(0, root);
        }
    }
    Ok(out)
}

#[allow(dead_code)]
fn reachable(ch: &Record, user: &str) -> Result<bool> {
    can_read(ch, user)
}

#[derive(Deserialize)]
pub struct ReactIn {
    id: String,
    emoji: String,
}

const MAX_EMOJI: usize = 24;
const MAX_REACTORS: usize = 200;

/// Add the caller's reaction to a message, or take it back if it is there.
pub fn react(input: ReactIn) -> Result<Value> {
    let (msg, ch) = load_message(&input.id)?;
    let user = require_read(&ch)?;
    let emoji = input.emoji.trim().to_string();
    if emoji.is_empty() || emoji.chars().count() > 16 || emoji.chars().any(char::is_whitespace) {
        return Err(Error::msg("a reaction is one emoji"));
    }
    if flag(&msg, "deleted") {
        return Err(Error::msg("this message was deleted"));
    }
    let mut reactions = msg.get("reactions").and_then(Value::as_object).cloned().unwrap_or_default();
    let mine = reactions.get(&emoji).and_then(Value::as_array).is_some_and(|users| users.iter().any(|u| u.as_str().is_some_and(|u| key(u) == key(&user))));
    if mine {
        if let Some(users) = reactions.get_mut(&emoji).and_then(Value::as_array_mut) {
            users.retain(|u| u.as_str().is_none_or(|u| key(u) != key(&user)));
        }
        if reactions.get(&emoji).and_then(Value::as_array).is_some_and(Vec::is_empty) {
            reactions.remove(&emoji);
        }
    } else {
        if !reactions.contains_key(&emoji) && reactions.len() >= MAX_EMOJI {
            return Err(Error::msg(format!("a message takes at most {MAX_EMOJI} different reactions")));
        }
        let users = reactions.entry(emoji).or_insert_with(|| json!([]));
        if let Some(list) = users.as_array_mut() {
            if list.len() >= MAX_REACTORS {
                return Err(Error::msg("too many people have reacted this way"));
            }
            list.push(json!(user));
        }
    }
    let row: Record = db::update("chat_message", key(id_text(&msg)), &json!({ "reactions": reactions }))?.ok_or_else(|| Error::msg("the message is gone"))?;
    publish_row(&msg, &ch, "message", &json!({ "row": row }))?;
    Ok(json!({ "row": row }))
}

#[derive(Deserialize, Default)]
pub struct SearchIn {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
}

/// Messages containing `q`, in the conversations the caller can read, newest first.
pub fn search(input: SearchIn) -> Result<Value> {
    let user = me()?;
    let needle = input.q.unwrap_or_default().trim().to_string();
    if needle.chars().count() < 2 {
        return Ok(json!({ "rows": [], "total": 0 }));
    }
    let mut names: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for m in db::find::<Record>("chat_member").filter("user", user.as_str()).limit(1000).all()? {
        if let Some(c) = text(&m, "channel") {
            if let Some(ch) = db::get::<Record>("chat_channel", key(c))? {
                names.insert(key(c).to_string(), crate::channels::title_of(&ch, &user)?);
            }
        }
    }
    for ch in db::find::<Record>("chat_channel").filter("kind", "channel").filter("is_private", false).filter("archived", false).limit(500).all()? {
        names.entry(key(id_text(&ch)).to_string()).or_insert_with(|| text(&ch, "name").unwrap_or("channel").to_string());
    }
    let keys: Vec<String> = names.keys().cloned().collect();
    if keys.is_empty() {
        return Ok(json!({ "rows": [], "total": 0 }));
    }
    let filter = Filter::one_of("channel", keys).and(Filter::like("body", &needle)).and(Filter::eq("deleted", false));
    let total = db::find::<Record>("chat_message").matching(filter.clone()).count()?;
    let limit = page_size(input.limit);
    let rows: Vec<Record> = db::find("chat_message").matching(filter).order_by("-at").limit(limit).offset(input.offset.unwrap_or(0)).all()?;
    let rows: Vec<Value> = rows
        .iter()
        .map(|r| {
            let channel = text(r, "channel").map(key).unwrap_or_default().to_string();
            json!({
                "id": key(id_text(r)), "channel": channel, "channel_name": names.get(&channel),
                "author_name": text(r, "author_name"), "body": text(r, "body").unwrap_or_default().chars().take(240).collect::<String>(),
                "at": r.get("at"), "thread": text(r, "parent"),
            })
        })
        .collect();
    Ok(json!({ "rows": rows, "total": total }))
}

#[derive(Deserialize)]
pub struct PinIn {
    id: String,
}

/// Pin a message of the conversation, or take the pin off. Members pin; so does an administrator.
pub fn pin(input: PinIn) -> Result<Value> {
    let user = me()?;
    let (msg, ch) = load_message(&input.id)?;
    if member_of(id_text(&ch), &user)?.is_none() && !is_admin()? {
        return Err(Error::msg("only people in the conversation pin messages"));
    }
    if flag(&msg, "deleted") {
        return Err(Error::msg("this message was deleted"));
    }
    if flag(&ch, "archived") {
        return Err(Error::msg("this conversation is archived"));
    }
    let pinned = !flag(&msg, "pinned");
    let row: Record = db::update("chat_message", key(id_text(&msg)), &json!({ "pinned": pinned }))?.ok_or_else(|| Error::msg("the message is gone"))?;
    publish_row(&msg, &ch, "message", &json!({ "row": row }))?;
    Ok(json!({ "row": row, "pinned": pinned }))
}

#[derive(Deserialize)]
pub struct PinnedIn {
    channel: String,
}

/// The pinned messages of a conversation, newest first, for the Pinned page.
pub fn pinned(input: PinnedIn) -> Result<Vec<Value>> {
    let ch = channel(&input.channel)?;
    require_read(&ch)?;
    let filter = Filter::eq("channel", channel_id(id_text(&ch))).and(Filter::eq("pinned", true)).and(Filter::eq("deleted", false));
    let rows: Vec<Record> = db::find("chat_message").matching(filter).order_by("-n").limit(100).all()?;
    Ok(rows
        .iter()
        .map(|r| {
            json!({
                "id": key(id_text(r)), "channel": input.channel, "author_name": text(r, "author_name"),
                "body": text(r, "body").unwrap_or_default().chars().take(400).collect::<String>(), "at": r.get("at"),
            })
        })
        .collect())
}
