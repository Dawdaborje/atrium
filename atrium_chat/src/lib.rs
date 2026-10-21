//! Atrium chat: teams, channels, group and direct chats, threads, read tracking and live delivery.
//!
//! A message is stored first (`chat_message`, audited, searchable); the WebSocket then carries a signal to the
//! people on the channel (`realtime::publish`). Who may join a topic is decided here, by the `can_join_*`
//! functions the kernel calls as the person who asks. See docs/design.md.

mod access;
mod channels;
mod common;
mod messages;
mod people;

use aether_sdk::prelude::*;

handler! {
    // ---- the rail and starting conversations
    fn my_channels(_input: Empty) -> Vec<Value> { channels::my_rail() }
    fn browse_channels(input: channels::Browse) -> Vec<Value> { channels::browse_channels(input) }
    fn list_teams(_input: Empty) -> Vec<Value> { channels::list_teams() }
    fn list_people(input: people::Search) -> Vec<Value> { people::list_people(input) }
    fn people(input: people::Search) -> Vec<Value> { people::names(input) }
    fn create_team(input: channels::NewTeam) -> Value { channels::create_team(input) }
    fn create_channel(input: channels::NewChannel) -> Value { channels::create_channel(input) }
    fn create_group(input: channels::NewGroup) -> Value { channels::create_group(input) }
    fn open_direct(input: channels::Direct) -> Value { channels::open_direct(input) }
    // ---- belonging
    fn join_channel(input: channels::ChannelOnly) -> Value { channels::join(input) }
    fn leave_channel(input: channels::ChannelOnly) -> Value { channels::leave(input) }
    fn add_members(input: channels::People) -> Value { channels::add_members(input) }
    fn remove_member(input: channels::One) -> Value { channels::remove_member(input) }
    fn edit_channel(input: channels::Edit) -> Value { channels::edit_channel(input) }
    fn channel_info(input: channels::ChannelOnly) -> Value { channels::info(input) }
    fn mark_read(input: channels::ChannelOnly) -> Value { channels::mark_read(input) }
    fn seed_chat(_input: Empty) -> Value { channels::seed() }
    fn my_membership(input: channels::ChannelOnly) -> Value { channels::my_membership(input) }
    fn set_muted(input: channels::MuteIn) -> Value { channels::set_muted(input) }
    fn channel_members(input: channels::ChannelOnly) -> Vec<Value> { channels::members_list(input) }
    fn channel_header(input: channels::ChannelOnly) -> Value { channels::header(input) }
    // ---- for other plugins (an administrator, or the kernel's own work)
    fn ensure_channel(input: channels::Ensure) -> Value { channels::ensure_channel(input) }
    fn sync_members(input: channels::Sync) -> Value { channels::sync_members(input) }
    fn remove_everywhere(input: channels::UserOnly) -> Value { channels::remove_everywhere(input) }
    // ---- messages
    fn react(input: messages::ReactIn) -> Value { messages::react(input) }
    fn pin_message(input: messages::PinIn) -> Value { messages::pin(input) }
    fn pinned_messages(input: messages::PinnedIn) -> Vec<Value> { messages::pinned(input) }
    fn search_messages(input: messages::SearchIn) -> Value { messages::search(input) }
    fn post_message(input: messages::Post) -> Value { messages::post(input) }
    fn edit_message(input: messages::EditIn) -> Value { messages::edit(input) }
    fn delete_message(input: channels::IdOnly) -> Value { messages::delete(input) }
    fn channel_messages(input: messages::List) -> Value { messages::list(input) }
    fn thread_messages(input: messages::ThreadIn) -> Value { messages::thread(input) }
    // ---- live topics
    fn can_join_channel(input: access::Join) -> Value { access::can_join_channel(input) }
    fn can_join_thread(input: access::Join) -> Value { access::can_join_thread(input) }
    fn can_join_inbox(input: access::Join) -> Value { access::can_join_inbox(input) }
    fn can_join_online(input: access::Join) -> Value { access::can_join_online(input) }
}
