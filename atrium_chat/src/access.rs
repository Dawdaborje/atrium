//! Who may join a live topic. The kernel calls these as the person who asks, before it lets them in.

use aether_sdk::prelude::*;

use crate::channels::{can_read, channel};
use crate::common::*;

#[derive(Deserialize)]
pub struct Join {
    #[serde(default)]
    params: Value,
    #[serde(default)]
    user: String,
}

fn param<'a>(join: &'a Join, name: &str) -> &'a str {
    join.params.get(name).and_then(Value::as_str).unwrap_or_default()
}

fn allow(yes: bool, reason: &str) -> Value {
    if yes {
        json!({ "allow": true })
    } else {
        json!({ "allow": false, "reason": reason })
    }
}

/// `channel.{channel}`: members, and anyone for a public channel.
pub fn can_join_channel(join: Join) -> Result<Value> {
    match channel(param(&join, "channel")) {
        Ok(ch) => Ok(allow(can_read(&ch, &join.user)?, "you are not in this conversation")),
        Err(_) => Ok(allow(false, "no such channel")),
    }
}

/// `thread.{thread}`: whoever may read the channel the message is in.
pub fn can_join_thread(join: Join) -> Result<Value> {
    let Ok(msg) = records::require("chat_message", key(param(&join, "thread")), "message") else {
        return Ok(allow(false, "no such message"));
    };
    match channel(text(&msg, "channel").unwrap_or_default()) {
        Ok(ch) => Ok(allow(can_read(&ch, &join.user)?, "you are not in this conversation")),
        Err(_) => Ok(allow(false, "no such channel")),
    }
}

/// `inbox.{user}`: only the person themselves.
pub fn can_join_inbox(join: Join) -> Result<Value> {
    Ok(allow(key(param(&join, "user")) == key(&join.user) && !join.user.is_empty(), "this is someone else's inbox"))
}

/// `online`: everyone signed in.
pub fn can_join_online(join: Join) -> Result<Value> {
    Ok(allow(!join.user.is_empty(), "sign in"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inbox_is_only_for_its_owner() {
        let own = Join { params: json!({ "user": "ann" }), user: "users:ann".into() };
        assert_eq!(can_join_inbox(own).unwrap_or_default()["allow"], true);
        let other = Join { params: json!({ "user": "bob" }), user: "users:ann".into() };
        assert_eq!(can_join_inbox(other).unwrap_or_default()["allow"], false);
        let nobody = Join { params: json!({ "user": "" }), user: String::new() };
        assert_eq!(can_join_inbox(nobody).unwrap_or_default()["allow"], false);
    }
}
