//! Helpers shared by the parts of the plugin.

pub use aether_sdk::records::{text, Record};

use aether_sdk::prelude::*;

/// The key of a record id: `chat_channel:abc` -> `abc`; a bare key stays as it is.
pub fn key(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, key)| key)
}

/// What a channel is called in the `channel` field of members and messages: its key.
pub fn channel_id(id: &str) -> String {
    key(id).to_string()
}

pub fn me() -> Result<String> {
    context::current()?.actor.id.ok_or_else(|| Error::msg("sign in to use chat"))
}

pub fn is_admin() -> Result<bool> {
    Ok(context::current()?.has_role("org_admin"))
}

pub fn now() -> Result<String> {
    let ctx = context::current()?;
    if ctx.now.is_empty() {
        return Err(Error::msg("the kernel did not say what time it is"));
    }
    Ok(ctx.now)
}

pub fn int(record: &Record, field: &str) -> i64 {
    record.get(field).and_then(Value::as_i64).unwrap_or(0)
}

pub fn flag(record: &Record, field: &str) -> bool {
    record.get(field).and_then(Value::as_bool).unwrap_or(false)
}

pub fn id_text(record: &Record) -> &str {
    text(record, "id").unwrap_or_default()
}

pub fn clean(label: &str, value: &str, max: usize) -> Result<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(Error::msg(format!("{label} is needed")));
    }
    if v.chars().count() > max {
        return Err(Error::msg(format!("{label} is at most {max} characters")));
    }
    Ok(v.to_string())
}

/// Topic names, without the plugin prefix.
pub fn channel_topic(channel: &str) -> String {
    format!("channel.{}", key(channel))
}
pub fn thread_topic(message: &str) -> String {
    format!("thread.{}", key(message))
}
pub fn inbox_topic(user: &str) -> String {
    format!("inbox.{}", key(user))
}

/// The kernel's own work (an event handler, a job) or an administrator: who may run the service functions other
/// plugins call to set up channels.
pub fn require_trusted() -> Result<()> {
    let ctx = context::current()?;
    if matches!(ctx.actor.kind, context::ActorKind::System) || ctx.has_role("org_admin") {
        Ok(())
    } else {
        Err(Error::msg("only an administrator can do this"))
    }
}

/// Who is acting, for `created_by`: the person, or `system` for the kernel's own work.
pub fn actor_label() -> String {
    context::current().ok().and_then(|c| c.actor.id).unwrap_or_else(|| "system".into())
}
