//! Atrium meetings: start or plan a video meeting, invite colleagues, join with one button. See docs/design.md.

mod common;
mod meetings;

use aether_sdk::prelude::*;

handler! {
    fn my_meetings(_input: Empty) -> Vec<Value> { meetings::my_meetings() }
    fn get_meeting(input: meetings::MeetingId) -> Value { meetings::get_meeting(input) }
    fn create_meeting(input: meetings::NewMeeting) -> Value { meetings::create_meeting(input) }
    fn start_now(input: meetings::Instant) -> Value { meetings::start_now(input) }
    fn join_meeting(input: meetings::MeetingId) -> Value { meetings::join_meeting(input) }
    fn cancel_meeting(input: meetings::MeetingId) -> Value { meetings::cancel_meeting(input) }
    fn pick_people(input: meetings::Pick) -> Vec<Value> { meetings::pick_people(input) }
}
