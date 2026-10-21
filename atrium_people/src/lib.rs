//! Atrium people: a directory of everyone who can sign in, with profiles and an org chart.
//!
//! The login accounts of the organization are the people; a profile adds what they say about themselves (title,
//! department, who they report to, how to reach them). The reporting lines are the org chart. The optional HR link
//! keeps the fields it manages in step with the HR records. See docs/design.md.

mod common;
mod profiles;

use aether_sdk::prelude::*;

handler! {
    // ---- looking
    fn directory(input: profiles::Directory) -> Vec<Value> { profiles::directory(input) }
    fn department_options(_input: Empty) -> Vec<Value> { profiles::department_options() }
    fn person(input: profiles::PersonIn) -> Value { profiles::person(input) }
    fn my_profile(_input: Empty) -> Value { profiles::my_profile() }
    fn person_reports(input: profiles::PersonIn) -> Vec<Value> { profiles::reports(input) }
    fn person_chain(input: profiles::PersonIn) -> Vec<Value> { profiles::chain(input) }
    fn org_chart(input: profiles::Chart) -> Vec<Value> { profiles::org_chart(input) }
    fn upcoming(input: profiles::Upcoming) -> Vec<Value> { profiles::upcoming(input) }
    fn pick_people(input: profiles::Pick) -> Vec<Value> { profiles::pick_people(input) }
    // ---- changing
    fn save_profile(input: profiles::Save) -> Value { profiles::save_profile(input) }
    // ---- for the HR link
    fn apply_hr(input: profiles::ApplyHr) -> Value { profiles::apply_hr(input) }
    fn set_away(input: profiles::SetAway) -> Value { profiles::set_away(input) }
    fn release_hr(input: profiles::PersonIn) -> Value { profiles::release_hr(input) }
}
