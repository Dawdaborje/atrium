//! Atrium mailbox: a person's own mail on the organization's JMAP server, read and written in Atrium. See docs/design.md.

mod messages;

use aether_sdk::prelude::*;

handler! {
    fn mailbox_state(_input: Empty) -> Value { messages::mailbox_state() }
    fn folders(_input: Empty) -> Vec<Value> { messages::folders() }
    fn inbox_messages(input: messages::Listing) -> Vec<Value> { messages::list(input) }
    fn get_message(input: messages::MessageId) -> Value { messages::get_message(input) }
    fn mark_unread(input: messages::MessageId) -> Value { messages::mark_unread(input) }
    fn trash_message(input: messages::MessageId) -> Value { messages::trash_message(input) }
    fn reply_message(input: messages::Reply) -> Value { messages::reply_message(input) }
    fn get_attachments(input: messages::MessageId) -> Value { messages::get_attachments(input) }
    fn can_read_media(_input: Value) -> Value { Ok(json!({ "allow": false })) }
    fn send_message(input: messages::Compose) -> Value { messages::send_message(input) }
}
