//! Atrium booking: people offer time ("30 minutes with me, weekdays 9 to 17"), colleagues pick a free slot, and the
//! meeting lands in both calendars. Free time is read from atrium_calendar, which never says what fills the rest.
//! See docs/design.md.

mod common;
mod services;
mod slots;

use aether_sdk::prelude::*;

handler! {
    fn my_services(input: Empty) -> Vec<Value> { services::my_services(input) }
    fn get_service(input: services::ServiceOnly) -> Value { services::get_service(input) }
    fn create_service(input: services::NewService) -> Value { services::create_service(input) }
    fn update_service(input: services::UpdateService) -> Value { services::update_service(input) }
    fn delete_service(input: services::ServiceOnly) -> Value { services::delete_service(input) }
    fn find_services(input: services::Search) -> Vec<Value> { services::find_services(input) }
    fn service_header(input: services::ServiceOnly) -> Value { services::service_header(input) }
    fn pick_weekdays(_input: Empty) -> Vec<Value> { services::pick_weekdays() }
    fn free_slots(input: services::ServiceOnly) -> Vec<Value> { slots::free_slots(input) }
    fn book(input: slots::Book) -> Value { slots::book(input) }
    fn public_service(input: slots::PublicRef) -> Value { slots::public_service(input) }
    fn public_slots(input: slots::PublicRef) -> Vec<Value> { slots::public_slots(input) }
    fn public_book(input: slots::PublicBook) -> Value { slots::public_book(input) }
    fn public_booking(input: slots::Token) -> Value { slots::public_booking(input) }
    fn public_cancel(input: slots::Token) -> Value { slots::public_cancel(input) }
    fn my_bookings(input: Empty) -> Vec<Value> { slots::my_bookings(input) }
    fn cancel_booking(input: slots::CancelIn) -> Value { slots::cancel_booking(input) }
}
