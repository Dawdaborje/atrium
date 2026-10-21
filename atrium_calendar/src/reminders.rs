//! Nudges: the day's agenda each morning, and a note a quarter of an hour before an event.

use aether_sdk::dates::Duration;
use aether_sdk::prelude::*;

use crate::calendars::notify;
use crate::common::*;
use crate::schedule::{answers_of, events_for, expand, is_attendee, parse, Scope};
use crate::zones::{day_bounds, Viewer};

/// Everyone with something on today hears what, once, in the morning.
pub fn morning_agenda() -> Result<Value> {
    let now = parse(&now()?)?;
    let mut sent = 0;
    for person in org_people()? {
        // Today is the person's own day, and they are told times as they read them.
        let viewer = Viewer::of(&person.id)?;
        let (today, tomorrow) = day_bounds(&viewer, now);
        let items = events_for(&person.id, today, tomorrow - Duration::seconds(1), None, Scope::Busy)?;
        if items.is_empty() {
            continue;
        }
        let first = items.iter().find(|i| !flag(&i.event, "all_day"));
        let title = match first {
            Some(i) => format!("{} on your calendar today, the first at {}: {}", items.len(), viewer.clock(i.start).format("%H:%M"), text(&i.event, "title").unwrap_or("an event")),
            None => format!("{} on your calendar today: {}", items.len(), text(&items[0].event, "title").unwrap_or("an event")),
        };
        notify(&person.id, &title, "/calendar/agenda")?;
        sent += 1;
    }
    Ok(json!({ "sent": sent }))
}

/// Run every five minutes: tell the people invited to an event that starts in ten to fifteen minutes.
pub fn starting_soon() -> Result<Value> {
    let now = parse(&now()?)?;
    let (from, to) = (now + Duration::minutes(10), now + Duration::minutes(15));
    let events: Vec<Record> = db::find("cal_event").limit(20000).all()?;
    let skips: Vec<Record> = db::find("cal_skip").limit(20000).all()?;
    let mut sent = 0;
    for event in events.iter().filter(|e| !flag(e, "all_day")) {
        let skipped: Vec<_> = skips.iter().filter(|s| text(s, "event").is_some_and(|k| key(k) == key(id_text(event)))).filter_map(|s| text(s, "date").and_then(|d| aether_sdk::dates::parse_date(d).ok())).collect();
        for item in expand(event, &skipped, from, to).into_iter().filter(|i| i.start >= from && i.start < to) {
            for user in strings(event.get("attendees")).iter().filter(|u| is_attendee(event, u)) {
                if answers_of(user)?.get(key(id_text(event))).is_some_and(|a| a == "declined") {
                    continue;
                }
                notify(user, &format!("“{}” starts at {}", text(event, "title").unwrap_or("An event"), Viewer::of(user)?.clock(item.start).format("%H:%M")), &format!("/calendar/event/{}", key(id_text(event))))?;
                sent += 1;
            }
        }
    }
    Ok(json!({ "sent": sent }))
}
