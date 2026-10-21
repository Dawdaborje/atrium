//! Folders, messages, answering and writing: each a call to the kernel's `jmap` bridge as the person using the plugin.

use aether_sdk::bridge;
use aether_sdk::prelude::*;

/// The mail servers the organization may use, in the order tried: the first one the person has connected a login for answers.
const BRIDGES: [&str; 2] = ["jmap", "imap"];
const LIMIT: u64 = 50;

/// Whether a refusal only means "not this server": no login connected for it, or the administrator has not set it up.
fn not_this_server(why: &str) -> bool {
    why.contains("not connected your") || why.contains("not set") || why.contains("not enabled") || why.contains("may not call")
}

fn call(action: &str, params: Value) -> Result<Value> {
    // A refusal reaches the person as a message they can act on, not as a failed call.
    let mut first: Option<Error> = None;
    for name in BRIDGES {
        match bridge::call(name, action, &params) {
            Ok(v) => return Ok(v),
            Err(e) if not_this_server(&e.to_string()) => {
                first.get_or_insert_with(|| Error::msg(e.to_string()));
            }
            Err(e) => return Err(Error::msg(e.to_string())),
        }
    }
    Err(first.unwrap_or_else(|| Error::msg("you have not connected your mailbox yet")))
}

fn s<'a>(v: &'a Value, f: &str) -> &'a str {
    v.get(f).and_then(Value::as_str).unwrap_or_default()
}

/// Whether the person has connected their mailbox, and as whom.
pub fn mailbox_state() -> Result<Value> {
    match call("check", json!({})) {
        Ok(v) => Ok(json!({ "connected": true, "name": s(&v, "name") })),
        Err(e) => Ok(json!({ "connected": false, "why": e.to_string() })),
    }
}

pub fn folders() -> Result<Vec<Value>> {
    let rows = call("mailboxes", json!({}))?;
    Ok(rows.as_array().into_iter().flatten().map(|m| json!({ "id": m["id"], "name": format!("{} ({})", s(m, "name"), m["unread"].as_u64().unwrap_or(0)) })).collect())
}

#[derive(Deserialize, Default)]
pub struct Listing {
    #[serde(default)]
    q: Option<String>,
    #[serde(default, rename = "folder")]
    folder: Option<String>,
}

fn day(t: &str) -> String {
    t.chars().take(16).collect::<String>().replace('T', " ")
}

pub fn list(input: Listing) -> Result<Vec<Value>> {
    let mut params = json!({ "limit": LIMIT });
    if let Some(q) = input.q.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        params["search"] = json!(q);
    }
    if let Some(f) = input.folder.as_deref().filter(|f| !f.is_empty()) {
        params["mailbox"] = json!(f);
    }
    let out = call("list", params)?;
    Ok(out["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| {
            json!({
                "id": m["id"], "from": s(m, "from"), "subject": s(m, "subject"), "preview": s(m, "preview"), "when": day(s(m, "received_at")),
                "tag": if m["unread"].as_bool().unwrap_or(false) { "New" } else { "" }, "files": if m["attachments"].as_bool().unwrap_or(false) { "Yes" } else { "" },
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct MessageId {
    pub id: String,
}

fn people(list: &Value) -> String {
    list.as_array().into_iter().flatten().map(|p| if s(p, "name").is_empty() { s(p, "email").to_string() } else { format!("{} <{}>", s(p, "name"), s(p, "email")) }).collect::<Vec<_>>().join(", ")
}

/// Mail is plain text, not Markdown: every line is quoted so nothing in it is read as formatting.
fn quoted(text: &str) -> String {
    text.lines().map(|l| format!("> {}", l.replace('\r', ""))).collect::<Vec<_>>().join("\n")
}

/// Opening a message marks it read.
pub fn get_message(input: MessageId) -> Result<Value> {
    let m = call("get", json!({ "id": input.id }))?;
    if m["unread"].as_bool().unwrap_or(false) {
        let _ = call("mark", json!({ "id": input.id, "read": true }));
    }
    let files: Vec<String> = m["attachments"].as_array().into_iter().flatten().map(|a| format!("{} ({} bytes)", s(a, "name"), a["size"].as_u64().unwrap_or(0))).collect();
    Ok(json!({
        "id": m["id"], "subject": s(&m, "subject"), "from": people(&m["from"]), "to": people(&m["to"]), "cc": people(&m["cc"]), "when": day(s(&m, "received_at")),
        "files": if files.is_empty() { "None".to_string() } else { files.join(", ") }, "body": quoted(s(&m, "text")),
    }))
}

/// The files a person attached on the form, as the bridge wants them: read back from the media store, where only they could put them.
fn attachments(files: &Value) -> Result<Vec<Value>> {
    use aether_sdk::media;
    let list: Vec<&Value> = match files {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![files],
        _ => Vec::new(),
    };
    if list.len() > 10 {
        return Err(Error::msg("attach at most 10 files"));
    }
    let mut out = Vec::new();
    for f in list {
        let key = s(f, "key");
        let bytes = media::read(key).map_err(|e| Error::msg(format!("could not read {}: {e}", s(f, "name"))))?;
        let name = s(f, "name");
        out.push(json!({ "name": if name.is_empty() { "attachment" } else { name }, "type": s(f, "mime"), "base64": base64_of(&bytes) }));
    }
    Ok(out)
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

/// Once sent, the uploads are no longer needed under the plugin's guard (best effort).
fn release(files: &Value) {
    let list: Vec<&Value> = match files {
        Value::Array(a) => a.iter().collect(),
        Value::Object(_) => vec![files],
        _ => Vec::new(),
    };
    for f in list {
        let _ = aether_sdk::media::release(s(f, "key"));
    }
}

/// The attachments of a message, fetched from the mail server and stored for the person alone: `{ files, skipped }`.
/// Opened on request (the Attachments button), not with every message.
pub fn get_attachments(input: MessageId) -> Result<Value> {
    let m = call("get", json!({ "id": input.id }))?;
    let (mut files, mut skipped) = (Vec::new(), Vec::new());
    for (index, a) in m["attachments"].as_array().into_iter().flatten().enumerate() {
        let name = s(a, "name");
        let got = match call("attachment", json!({ "id": input.id, "index": index })) {
            Ok(got) => got,
            Err(e) => {
                skipped.push(format!("{name}: {e}"));
                continue;
            }
        };
        use base64::{Engine, engine::general_purpose::STANDARD};
        let bytes = STANDARD.decode(s(&got, "base64")).unwrap_or_default();
        match aether_sdk::media::store(if name.is_empty() { "attachment" } else { name }, &bytes) {
            Ok(f) => files.push(json!({ "key": f.key, "name": f.name, "mime": f.mime, "size": f.size })),
            Err(e) => skipped.push(format!("{name}: {e}")),
        }
    }
    Ok(json!({ "id": input.id, "subject": s(&m, "subject"), "files": files, "skipped": if skipped.is_empty() { "None".to_string() } else { skipped.join("; ") } }))
}

pub fn mark_unread(input: MessageId) -> Result<Value> {
    call("mark", json!({ "id": input.id, "read": false }))
}

pub fn trash_message(input: MessageId) -> Result<Value> {
    call("trash", json!({ "id": input.id }))
}

#[derive(Deserialize)]
pub struct Reply {
    id: String,
    text: String,
    #[serde(default)]
    files: Value,
    #[serde(default)]
    all: bool,
}

fn emails(list: &Value) -> Vec<String> {
    list.as_array().into_iter().flatten().map(|p| s(p, "email").to_string()).filter(|e| !e.is_empty()).collect()
}

pub fn reply_message(input: Reply) -> Result<Value> {
    if input.text.trim().is_empty() {
        return Err(Error::msg("write your answer first"));
    }
    let m = call("get", json!({ "id": input.id }))?;
    let to = emails(&m["from"]);
    if to.is_empty() {
        return Err(Error::msg("this message has no sender to answer"));
    }
    let me = call("check", json!({}))?;
    let cc: Vec<String> = if input.all { emails(&m["to"]).into_iter().chain(emails(&m["cc"])).filter(|e| !e.eq_ignore_ascii_case(s(&me, "name")) && !to.contains(e)).collect() } else { Vec::new() };
    let subject = s(&m, "subject");
    let subject = if subject.to_lowercase().starts_with("re:") { subject.to_string() } else { format!("Re: {subject}") };
    let mut refs: Vec<Value> = m["references"].as_array().cloned().unwrap_or_default();
    if let Some(id) = m["message_id"].as_str() {
        refs.push(json!(id));
    }
    let sent = call("send", json!({ "to": to, "cc": cc, "subject": subject, "text": input.text, "in_reply_to": m["message_id"], "references": refs, "attachments": attachments(&input.files)? }))?;
    release(&input.files);
    Ok(json!({ "id": sent["id"] }))
}

#[derive(Deserialize)]
pub struct Compose {
    to: String,
    #[serde(default)]
    cc: Option<String>,
    subject: String,
    text: String,
    #[serde(default)]
    files: Value,
}

fn split(list: &str) -> Vec<String> {
    list.split([',', ';', ' ', '\n']).map(str::trim).filter(|a| !a.is_empty()).map(str::to_string).collect()
}

pub fn send_message(input: Compose) -> Result<Value> {
    let to = split(&input.to);
    if to.is_empty() {
        return Err(Error::msg("who is it for? Add an address"));
    }
    let sent = call("send", json!({ "to": to, "cc": split(input.cc.as_deref().unwrap_or_default()), "subject": input.subject, "text": input.text, "attachments": attachments(&input.files)? }))?;
    release(&input.files);
    Ok(json!({ "id": sent["id"] }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_missing_login_or_setup_moves_on_to_the_next_server() {
        assert!(not_this_server("you have not connected your account yet: connect it first, then try again"));
        assert!(not_this_server("jmap: `session_url` is not set in the bridge settings"));
        assert!(!not_this_server("the mail server did not accept your login"));
    }

    #[test]
    fn address_lists_are_split_on_commas_and_spaces() {
        assert_eq!(split("a@x.test, b@x.test;c@x.test  d@x.test"), ["a@x.test", "b@x.test", "c@x.test", "d@x.test"]);
        assert!(split("  ").is_empty());
    }

    #[test]
    fn mail_text_is_quoted_so_it_is_never_formatting() {
        assert_eq!(quoted("# hi\r\n[x](javascript:1)"), "> # hi\n> [x](javascript:1)");
    }
}
