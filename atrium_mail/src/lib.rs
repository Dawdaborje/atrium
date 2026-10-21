//! Atrium shared inbox: mail to a company address becomes a conversation a team answers together. See docs/design.md.

mod boxes;
mod common;
mod inbound;
mod threads;

use aether_sdk::prelude::*;

handler! {
    // ---- mail arriving (the route in plugin.toml)
    fn on_mail(input: inbound::Incoming) -> Value { inbound::on_mail(input) }
    // ---- boxes and their people
    fn my_boxes(_input: Empty) -> Vec<Value> { boxes::my_boxes() }
    fn box_options(_input: Empty) -> Vec<Value> { boxes::box_options() }
    fn mail_access(_input: Empty) -> Value { boxes::mail_access() }
    fn create_box(input: boxes::NewBox) -> Value { boxes::create_box(input) }
    fn delete_box(input: boxes::BoxId) -> Value { boxes::delete_box(input) }
    fn box_members(input: boxes::BoxScope) -> Vec<Value> { boxes::box_members(input) }
    fn add_box_members(input: boxes::AddMembers) -> Value { boxes::add_box_members(input) }
    fn remove_box_member(input: boxes::BoxId) -> Value { boxes::remove_box_member(input) }
    fn pick_people(input: boxes::Pick) -> Vec<Value> { boxes::pick_people(input) }
    fn rule_var_my_boxes(_input: Empty) -> Vec<String> { boxes::rule_my_boxes() }
    // ---- conversations
    fn inbox(input: threads::Inbox) -> Vec<Value> { threads::inbox(input) }
    fn get_thread(input: threads::ThreadId) -> Value { threads::get_thread(input) }
    fn reply_thread(input: threads::Reply) -> Value { threads::reply_thread(input) }
    fn take_thread(input: threads::ThreadId) -> Value { threads::take_thread(input) }
    fn assign_thread(input: threads::Assign) -> Value { threads::assign_thread(input) }
    fn release_thread(input: threads::ThreadId) -> Value { threads::release_thread(input) }
    fn close_thread(input: threads::ThreadId) -> Value { threads::close_thread(input) }
    fn reopen_thread(input: threads::ThreadId) -> Value { threads::reopen_thread(input) }
}
