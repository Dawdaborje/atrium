//! Atrium calendar: shared calendars, events that repeat, invitations and answers, who is free, and iCalendar files.
//!
//! A calendar belongs to its members (owner, editor or viewer). Every person has a calendar of their own. An event's
//! conversation is the kernel's chatter. See docs/design.md.

mod access;
mod booking;
mod calendars;
mod common;
mod events;
mod ics;
mod links;
mod recur;
mod reminders;
mod schedule;
mod views;
mod zones;

use aether_sdk::prelude::*;

handler! {
    // ---- calendars and their people
    fn my_calendars(input: Empty) -> Vec<Value> { views::my_calendars(input) }
    fn create_calendar(input: calendars::NewCalendar) -> Value { calendars::create_calendar(input) }
    fn edit_calendar(input: calendars::EditCalendar) -> Value { calendars::edit_calendar(input) }
    fn delete_calendar(input: calendars::CalendarOnly) -> Value { calendars::delete_calendar(input) }
    fn calendar_header(input: calendars::CalendarOnly) -> Value { views::calendar_header(input) }
    fn calendar_info(input: calendars::CalendarOnly) -> Value { views::calendar_info(input) }
    fn calendar_members(input: calendars::CalendarOnly) -> Vec<Value> { calendars::calendar_members(input) }
    fn add_calendar_members(input: calendars::AddMembers) -> Value { calendars::add_calendar_members(input) }
    fn remove_calendar_member(input: calendars::RemoveMember) -> Value { calendars::remove_calendar_member(input) }
    fn set_calendar_role(input: calendars::SetRole) -> Value { calendars::set_calendar_role(input) }
    // ---- events
    fn events_in(input: views::Window) -> Vec<Value> { views::events_in(input) }
    fn agenda(input: Empty) -> Vec<Value> { views::agenda(input) }
    fn availability(input: Empty) -> Vec<Value> { views::availability(input) }
    fn get_event(input: events::EventOnly) -> Value { views::get_event(input) }
    fn create_event(input: events::NewEvent) -> Value { events::create_event(input) }
    fn update_event(input: events::UpdateEvent) -> Value { events::update_event(input) }
    fn delete_event(input: events::EventOnly) -> Value { events::delete_event(input) }
    fn delete_occurrence(input: events::SkipDay) -> Value { events::delete_occurrence(input) }
    fn respond(input: events::Answer) -> Value { events::respond(input) }
    // ---- time zone
    fn my_timezone(_input: Empty) -> Value { zones::my_timezone() }
    fn save_timezone(input: zones::SaveZone) -> Value { zones::save_timezone(input) }
    fn pick_timezones(input: zones::PickZone) -> Vec<Value> { zones::pick_timezones(input) }
    fn typed_to_utc(input: zones::Typed) -> Value { zones::typed_to_utc(input) }
    fn read_times(input: zones::Stored) -> Vec<Value> { zones::read_times(input) }
    // ---- booking links (the last three are open to people outside the organization, with a token)
    fn make_link(input: Empty) -> Value { links::make_link(input) }
    fn revoke_link(input: Empty) -> Value { links::revoke_link(input) }
    fn link_busy(input: links::BusyIn) -> Vec<Value> { links::link_busy(input) }
    fn link_book(input: links::GuestBook) -> Value { links::link_book(input) }
    fn link_cancel(input: links::GuestCancel) -> Value { links::link_cancel(input) }
    // ---- for the booking plugin
    fn busy_of(input: booking::BusyIn) -> Vec<Value> { booking::busy_of(input) }
    fn book_slot(input: booking::Book) -> Value { booking::book_slot(input) }
    fn cancel_booked(input: booking::Cancel) -> Value { booking::cancel_booked(input) }
    // ---- files
    fn export_ics(input: calendars::CalendarOnly) -> Value { ics::export_ics(input) }
    fn import_ics(input: ics::ImportIcs) -> Value { ics::import_ics(input) }
    // ---- pickers on the pages
    fn pick_calendars(input: views::Pick) -> Vec<Value> { views::pick_calendars(input) }
    fn pick_org_people(input: views::Pick) -> Vec<Value> { views::pick_org_people(input) }
    fn pick_other_people(input: views::Pick) -> Vec<Value> { views::pick_other_people(input) }
    // ---- scheduled runs, and the record rule
    fn morning_agenda(_input: Empty) -> Value { reminders::morning_agenda() }
    fn starting_soon(_input: Empty) -> Value { reminders::starting_soon() }
    fn rule_var_my_calendars(_input: Empty) -> Vec<String> { views::rule_my_calendars() }
}
