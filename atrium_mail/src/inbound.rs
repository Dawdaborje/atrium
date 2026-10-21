//! Mail that arrives at inbox@ or inbox+tag@: find the box, find the conversation it belongs to (or open a new one),
//! keep the message, and tell the people.

use aether_sdk::prelude::*;

use crate::boxes;
use crate::common::*;

const MAX_TEXT: usize = 50_000;

#[derive(Deserialize, Default)]
pub struct Incoming {
    #[serde(default)]
    from: Value,
    #[serde(default)]
    to: Value,
    #[serde(default)]
    cc: Value,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    in_reply_to: Option<String>,
    #[serde(default)]
    received_at: Option<String>,
}

/// `{name, address}`, `"Name <a@b>"` or `"a@b"`: the name and the address.
pub fn person(value: &Value) -> (String, String) {
    let (name, raw) = match value {
        Value::Object(o) => (o.get("name").and_then(Value::as_str).unwrap_or_default().to_string(), o.get("address").and_then(Value::as_str).unwrap_or_default().to_string()),
        Value::String(t) => match (t.find('<'), t.rfind('>')) {
            (Some(a), Some(b)) if a < b => (t[..a].trim().trim_matches('"').to_string(), t[a + 1..b].to_string()),
            _ => (String::new(), t.clone()),
        },
        _ => (String::new(), String::new()),
    };
    (name, raw.trim().to_lowercase())
}

fn addresses(value: &Value) -> Vec<String> {
    match value {
        Value::Array(items) => items.iter().map(|v| person(v).1).collect(),
        Value::Null => Vec::new(),
        other => vec![person(other).1],
    }
}

/// The tag of the box an address points at: `inbox+sales@x` is `sales`, `inbox@x` is empty. Other addresses: none.
pub fn tag_in(address: &str) -> Option<String> {
    let local = address.split('@').next()?;
    if local == "inbox" {
        Some(String::new())
    } else {
        local.strip_prefix("inbox+").map(str::to_string)
    }
}

/// The conversation key written into a subject as `[#key]`.
pub fn key_in(subject: &str) -> Option<String> {
    let start = subject.rfind("[#")? + 2;
    let end = subject[start..].find(']')? + start;
    let k = &subject[start..end];
    (!k.is_empty() && k.len() <= 64 && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')).then(|| k.to_string())
}

/// A subject without `Re:`/`Fwd:` prefixes and without our `[#key]`.
pub fn plain_subject(subject: &str) -> String {
    let mut t = subject.trim().to_string();
    if let Some(i) = t.rfind("[#") {
        if t[i..].contains(']') {
            t = format!("{}{}", &t[..i], t[i..].split_once(']').map(|x| x.1).unwrap_or_default());
        }
    }
    loop {
        let low = t.trim_start().to_lowercase();
        let cut = ["re:", "fw:", "fwd:", "aw:"].iter().find(|p| low.starts_with(**p)).map(|p| p.len());
        match cut {
            Some(n) => t = t.trim_start()[n..].to_string(),
            None => break,
        }
    }
    t.trim().chars().take(300).collect()
}

pub fn on_mail(input: Incoming) -> Result<Value> {
    let recipients: Vec<String> = addresses(&input.to).into_iter().chain(addresses(&input.cc)).collect();
    let tags: Vec<String> = recipients.iter().filter_map(|a| tag_in(a)).collect();
    let boxes = boxes::all_boxes()?;
    let target = tags
        .iter()
        .find_map(|t| boxes.iter().find(|b| text(b, "tag").unwrap_or_default() == t))
        .or_else(|| boxes.iter().find(|b| text(b, "tag").is_none()));
    let Some(target) = target else {
        return Ok(json!({ "kept": false, "why": "no box for this address" }));
    };
    let box_key = key(id_text(target)).to_string();
    let message_id = input.message_id.clone().unwrap_or_default();
    if !message_id.is_empty() && !db::find::<Record>("mail_message").filter("message_id", message_id.as_str()).limit(1).all()?.is_empty() {
        return Ok(json!({ "kept": false, "why": "seen already" }));
    }
    let subject_raw = input.subject.clone().unwrap_or_default();
    let (name, address) = person(&input.from);
    if address.is_empty() {
        return Ok(json!({ "kept": false, "why": "no sender" }));
    }
    let at = now().ok().or(input.received_at.clone()).unwrap_or_default();
    let body: String = input.text.clone().unwrap_or_default().chars().take(MAX_TEXT).collect();

    // which conversation: our [#key] in the subject, or the message this one answers
    let mut thread: Option<Record> = None;
    if let Some(k) = key_in(&subject_raw) {
        thread = db::get::<Record>("mail_thread", &k)?.filter(|t| text(t, "box").is_some_and(|b| key(b) == box_key));
    }
    if thread.is_none() {
        if let Some(r) = input.in_reply_to.as_deref().filter(|r| !r.is_empty()) {
            if let Some(m) = db::find::<Record>("mail_message").filter("message_id", r).limit(1).all()?.into_iter().next() {
                thread = text(&m, "thread").map(|k| db::get::<Record>("mail_thread", key(k))).transpose()?.flatten();
            }
        }
    }
    let fresh = thread.is_none();
    let thread = match thread {
        Some(t) => t,
        None => db::create(
            "mail_thread",
            &json!({ "box": box_key, "subject": plain_subject(&subject_raw), "from_name": name, "from_address": address, "status": "open", "last_at": at, "count": 0 }),
        )?,
    };
    let tid = key(id_text(&thread)).to_string();
    db::create::<Record>(
        "mail_message",
        &json!({ "box": box_key, "thread": tid, "direction": "in", "subject": subject_raw.chars().take(300).collect::<String>(), "sender": address, "by": name, "text": body, "at": at, "message_id": message_id }),
    )?;
    let count = thread.get("count").and_then(Value::as_i64).unwrap_or(0) + 1;
    db::update::<Record>("mail_thread", &tid, &json!({ "count": count, "last_at": at, "status": "open" }))?;

    // tell the assignee, or the whole box when nobody has it yet
    let assignee = text(&thread, "assignee").map(str::to_string);
    let who: Vec<String> = match assignee {
        Some(a) => vec![a],
        None => boxes::members_of(&box_key)?.iter().filter_map(|m| text(m, "user").map(str::to_string)).collect(),
    };
    if !who.is_empty() {
        let label = if name.is_empty() { address.clone() } else { name.clone() };
        let title = if fresh { format!("New mail from {label}: {}", plain_subject(&subject_raw)) } else { format!("{label} answered: {}", plain_subject(&subject_raw)) };
        Notification::new(title).link(format!("/mail/t/{tid}")).to(Audience::Actors(who)).send()?;
    }
    Ok(json!({ "kept": true, "thread": tid, "new": fresh }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_name_their_box() {
        assert_eq!(tag_in("inbox@mail.x.test"), Some(String::new()));
        assert_eq!(tag_in("inbox+sales@mail.x.test"), Some("sales".into()));
        assert_eq!(tag_in("support@mail.x.test"), None);
    }

    #[test]
    fn the_conversation_key_is_read_from_the_subject() {
        assert_eq!(key_in("Re: Printer [#abc123]"), Some("abc123".into()));
        assert_eq!(key_in("Printer"), None);
        assert_eq!(key_in("Re: [#]"), None);
        assert_eq!(plain_subject("Re: RE: Printer [#abc123]"), "Printer");
    }

    #[test]
    fn people_are_read_from_either_shape() {
        assert_eq!(person(&json!({"name": "Ann", "address": "Ann@X.test"})), ("Ann".into(), "ann@x.test".into()));
        assert_eq!(person(&json!("Ann B <ann@x.test>")), ("Ann B".into(), "ann@x.test".into()));
    }
}
