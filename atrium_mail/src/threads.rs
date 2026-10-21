//! Conversations: who has them, what state they are in, answering the customer.

use aether_sdk::{communication, mail};
use aether_sdk::prelude::*;

use crate::boxes;
use crate::common::*;
use crate::inbound::plain_subject;

const MAX_REPLY: usize = 20_000;

fn s<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

/// The conversation, if the caller works its box.
fn thread_for(id: &str) -> Result<(Record, String)> {
    let t = db::get::<Record>("mail_thread", key(id))?.ok_or_else(|| Error::msg("there is no such conversation"))?;
    let (_, me) = boxes::require_member(s(&t, "box")).map_err(|_| Error::msg("there is no such conversation"))?;
    Ok((t, me))
}

fn day(t: &str) -> String {
    t.chars().take(16).collect::<String>().replace('T', " ")
}

fn row(t: &Record, people: &[Person], names: &[(String, String)]) -> Value {
    let who = if s(t, "from_name").is_empty() { s(t, "from_address") } else { s(t, "from_name") };
    json!({
        "id": key(id_text(t)), "subject": s(t, "subject"), "from": who, "status": s(t, "status"), "count": t.get("count").and_then(Value::as_i64).unwrap_or(0),
        "assignee": if s(t, "assignee").is_empty() { "—".to_string() } else { name_of(people, s(t, "assignee")) }, "when": day(s(t, "last_at")),
        "box": names.iter().find(|(k, _)| k == key(s(t, "box"))).map(|(_, n)| n.clone()).unwrap_or_default(),
    })
}

#[derive(Deserialize, Default)]
pub struct Inbox {
    #[serde(default)]
    q: Option<String>,
    #[serde(default, rename = "box")]
    box_: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    mine: Option<bool>,
}

pub fn inbox(input: Inbox) -> Result<Vec<Value>> {
    let me = me()?;
    let mine = boxes::my_box_keys(&me)?;
    let people = org_people()?;
    let names: Vec<(String, String)> = boxes::all_boxes()?.iter().map(|b| (key(id_text(b)).to_string(), s(b, "name").to_string())).collect();
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let wanted_box = input.box_.unwrap_or_default();
    let status = input.status.unwrap_or_default();
    let mut rows: Vec<Record> = db::find::<Record>("mail_thread").limit(5000).all()?;
    rows.retain(|t| mine.iter().any(|b| b == key(s(t, "box"))));
    rows.retain(|t| wanted_box.is_empty() || key(s(t, "box")) == key(&wanted_box));
    rows.retain(|t| if status.is_empty() { s(t, "status") != "closed" } else { s(t, "status") == status });
    rows.retain(|t| !input.mine.unwrap_or(false) || key(s(t, "assignee")) == key(&me));
    rows.retain(|t| needle.is_empty() || [s(t, "subject"), s(t, "from_name"), s(t, "from_address")].iter().any(|f| f.to_lowercase().contains(&needle)));
    rows.sort_by(|a, b| s(b, "last_at").cmp(s(a, "last_at")));
    Ok(rows.iter().map(|t| row(t, &people, &names)).collect())
}

#[derive(Deserialize)]
pub struct ThreadId {
    pub id: String,
}

pub fn get_thread(input: ThreadId) -> Result<Value> {
    let (t, me) = thread_for(&input.id)?;
    let id = key(id_text(&t)).to_string();
    let people = org_people()?;
    let mut msgs: Vec<Record> = db::find::<Record>("mail_message").filter("thread", id.as_str()).limit(500).all()?;
    msgs.sort_by(|a, b| s(a, "at").cmp(s(b, "at")));
    let mut out = String::new();
    for m in &msgs {
        let by = if s(m, "direction") == "out" { format!("**{}** (reply)", name_of(&people, s(m, "by"))) } else { format!("**{}** <{}>", if s(m, "by").is_empty() { s(m, "sender") } else { s(m, "by") }, s(m, "sender")) };
        out.push_str(&format!("{by} · {}\n\n", day(s(m, "at"))));
        for line in s(m, "text").lines() {
            out.push_str(&format!("> {line}\n"));
        }
        out.push_str("\n---\n\n");
    }
    let who = if s(&t, "from_name").is_empty() { s(&t, "from_address").to_string() } else { format!("{} <{}>", s(&t, "from_name"), s(&t, "from_address")) };
    Ok(json!({
        "id": id, "subject": s(&t, "subject"), "from": who, "status": s(&t, "status"),
        "assignee": if s(&t, "assignee").is_empty() { "Nobody yet".to_string() } else { name_of(&people, s(&t, "assignee")) },
        "mine": key(s(&t, "assignee")) == key(&me), "conversation": out, "messages": msgs.len(),
    }))
}

fn set(id: &str, fields: Value) -> Result<()> {
    db::update::<Record>("mail_thread", key(id), &fields)?;
    Ok(())
}

pub fn take_thread(input: ThreadId) -> Result<Value> {
    let (t, me) = thread_for(&input.id)?;
    set(id_text(&t), json!({ "assignee": me }))?;
    Ok(json!({ "ok": true }))
}

pub fn release_thread(input: ThreadId) -> Result<Value> {
    let (t, _) = thread_for(&input.id)?;
    set(id_text(&t), json!({ "assignee": "" }))?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct Assign {
    id: String,
    user: String,
}

pub fn assign_thread(input: Assign) -> Result<Value> {
    let (t, me) = thread_for(&input.id)?;
    if !boxes::is_member(s(&t, "box"), &input.user)? {
        return Err(Error::msg("only a person of the box can have the conversation"));
    }
    set(id_text(&t), json!({ "assignee": input.user }))?;
    if key(&input.user) != key(&me) {
        Notification::new(format!("Conversation for you: {}", s(&t, "subject"))).link(format!("/mail/t/{}", key(id_text(&t)))).to(Audience::Actors(vec![input.user])).send()?;
    }
    Ok(json!({ "ok": true }))
}

pub fn close_thread(input: ThreadId) -> Result<Value> {
    let (t, _) = thread_for(&input.id)?;
    set(id_text(&t), json!({ "status": "closed" }))?;
    Ok(json!({ "ok": true }))
}

pub fn reopen_thread(input: ThreadId) -> Result<Value> {
    let (t, _) = thread_for(&input.id)?;
    set(id_text(&t), json!({ "status": "open" }))?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct Reply {
    id: String,
    text: String,
}

/// The address the customer's answer should come to: the box's own, from the domain the kernel reports.
fn box_address(tid: &str, tag: &str) -> Option<String> {
    let out = mail::thread(&json!({ "model": "mail_thread", "record": tid, "alias": "inbox" })).ok()?;
    let domain = out.get("reply_to")?.as_str()?.rsplit_once('@')?.1.to_string();
    Some(boxes::address(tag, &domain))
}

pub fn reply_thread(input: Reply) -> Result<Value> {
    let (t, me) = thread_for(&input.id)?;
    let body = clean("a reply", &input.text, MAX_REPLY)?;
    let tid = key(id_text(&t)).to_string();
    let (_, tag) = match db::get::<Record>("mail_box", key(s(&t, "box")))? {
        Some(b) => (b.clone(), s(&b, "tag").to_string()),
        None => return Err(Error::msg("the box is gone")),
    };
    let subject = format!("Re: {} [#{tid}]", plain_subject(s(&t, "subject")));
    let mut mail_out = communication::email([s(&t, "from_address")]).subject(&subject).text(&body);
    if let Some(addr) = box_address(id_text(&t), &tag) {
        mail_out = mail_out.reply_to(&addr);
    }
    mail_out.send().map_err(|e| Error::msg(format!("the reply could not be sent: {e}")))?;
    let at = now()?;
    db::create::<Record>("mail_message", &json!({ "box": key(s(&t, "box")), "thread": tid, "direction": "out", "subject": subject, "by": me, "text": body, "at": at }))?;
    let count = t.get("count").and_then(Value::as_i64).unwrap_or(0) + 1;
    let assignee = if s(&t, "assignee").is_empty() { me.clone() } else { s(&t, "assignee").to_string() };
    set(&tid, json!({ "count": count, "last_at": at, "status": "pending", "assignee": assignee }))?;
    Ok(json!({ "id": tid }))
}
