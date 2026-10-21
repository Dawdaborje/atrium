//! Time zones. Everything is stored in UTC; a person's own zone (their setting) is how they type and read times.
//!
//! A person with no setting is read as UTC, exactly as before zones existed: times leave with a `Z` and the browser
//! shows them in its own zone. A person who chose a zone types times in it (a time with an explicit `Z` or offset is
//! taken as it is) and reads them in it, as local times without a `Z`.

use aether_sdk::dates::{self, Duration, NaiveDateTime};
use aether_sdk::prelude::*;
use chrono::{LocalResult, TimeZone};
use chrono_tz::Tz;

use crate::common::*;

/// The zone named `name`, if it is one (`Europe/Oslo`, `UTC`); case does not matter.
pub fn parse_zone(name: &str) -> Option<Tz> {
    let wanted = name.trim();
    if wanted.is_empty() {
        return None;
    }
    wanted.parse::<Tz>().ok().or_else(|| chrono_tz::TZ_VARIANTS.iter().copied().find(|z| z.name().eq_ignore_ascii_case(wanted)))
}

/// The UTC moment of a local wall-clock time. A time that does not exist (the clocks jumped over it) is moved to the
/// first minute after the jump; one that exists twice is the first of the two.
pub fn local_to_utc(local: NaiveDateTime, zone: Tz) -> NaiveDateTime {
    match zone.from_local_datetime(&local) {
        LocalResult::Single(t) => t.naive_utc(),
        LocalResult::Ambiguous(first, _) => first.naive_utc(),
        LocalResult::None => match zone.from_local_datetime(&(local + Duration::hours(1))) {
            // Inside a gap: an hour on is a real time in every zone that jumps by an hour.
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.naive_utc(),
            LocalResult::None => local,
        },
    }
}

pub fn utc_to_local(utc: NaiveDateTime, zone: Tz) -> NaiveDateTime {
    zone.from_utc_datetime(&utc).naive_local()
}

/// How one person types and reads times.
#[derive(Debug, Clone, Copy, Default)]
pub struct Viewer {
    zone: Option<Tz>,
}

impl Viewer {
    /// Times as they are stored.
    pub fn utc() -> Viewer {
        Viewer { zone: None }
    }

    #[cfg(test)]
    pub fn in_zone(zone: Tz) -> Viewer {
        Viewer { zone: Some(zone) }
    }

    pub fn of(user: &str) -> Result<Viewer> {
        let row: Option<Record> = db::find("cal_setting").filter("user", user).first()?;
        Ok(Viewer { zone: row.as_ref().and_then(|r| text(r, "tz")).and_then(parse_zone) })
    }

    pub fn me() -> Result<Viewer> {
        Viewer::of(&me()?)
    }

    pub fn zone(&self) -> Option<Tz> {
        self.zone
    }

    /// The zone's name for the settings page and the events: `UTC` without a setting.
    pub fn name(&self) -> String {
        self.zone.map(|z| z.name().to_string()).unwrap_or_else(|| "UTC".to_string())
    }

    /// A typed time without a zone, as UTC.
    pub fn to_utc(&self, typed: NaiveDateTime) -> NaiveDateTime {
        self.zone.map(|z| local_to_utc(typed, z)).unwrap_or(typed)
    }

    /// A stored time as this person reads it (a wall-clock time).
    pub fn clock(&self, utc: NaiveDateTime) -> NaiveDateTime {
        self.zone.map(|z| utc_to_local(utc, z)).unwrap_or(utc)
    }

    /// A stored time as text for a page: a local time without a `Z` for people with a zone, the stored text otherwise.
    pub fn show(&self, utc: NaiveDateTime) -> String {
        match self.zone {
            Some(z) => dates::format_datetime(utc_to_local(utc, z)).trim_end_matches('Z').to_string(),
            None => dates::format_datetime(utc),
        }
    }
}

/// The local midnight that starts the day of `now` for this person, and the next one, as UTC.
pub fn day_bounds(viewer: &Viewer, now: NaiveDateTime) -> (NaiveDateTime, NaiveDateTime) {
    let start_local = viewer.clock(now).date().and_time(dates::NaiveTime::MIN);
    let end_local = start_local + Duration::days(1);
    (viewer.to_utc(start_local), viewer.to_utc(end_local))
}

/// The caller's zone for the settings page: empty when they have not chosen one.
pub fn my_timezone() -> Result<Value> {
    let me = me()?;
    let row: Option<Record> = db::find("cal_setting").filter("user", me.as_str()).first()?;
    Ok(json!({ "tz": row.as_ref().and_then(|r| text(r, "tz")).unwrap_or_default() }))
}

#[derive(Deserialize)]
pub struct SaveZone {
    #[serde(default)]
    tz: Option<String>,
}

/// Choose the zone the caller types and reads times in; an empty answer goes back to UTC as stored.
pub fn save_timezone(input: SaveZone) -> Result<Value> {
    let me = me()?;
    let wanted = input.tz.as_deref().map(str::trim).unwrap_or_default();
    let name = if wanted.is_empty() {
        String::new()
    } else {
        parse_zone(wanted).ok_or_else(|| Error::msg("that is not a time zone; use a name like Europe/Oslo or Africa/Lagos"))?.name().to_string()
    };
    let row: Option<Record> = db::find("cal_setting").filter("user", me.as_str()).first()?;
    match row {
        Some(r) => {
            db::update::<Record>("cal_setting", key(id_text(&r)), &json!({ "tz": name }))?;
        }
        None => {
            db::create::<Record>("cal_setting", &json!({ "user": me, "tz": name }))?;
        }
    }
    Ok(json!({ "tz": name }))
}

#[derive(Deserialize, Default)]
pub struct PickZone {
    #[serde(default)]
    q: Option<String>,
}

/// Zones whose name holds what was typed, for the settings page.
pub fn pick_timezones(input: PickZone) -> Result<Vec<Value>> {
    let needle = input.q.unwrap_or_default().trim().to_lowercase().replace(' ', "_");
    Ok(chrono_tz::TZ_VARIANTS.iter().filter(|z| needle.is_empty() || z.name().to_lowercase().contains(&needle)).take(100).map(|z| json!({ "id": z.name(), "name": z.name() })).collect())
}

#[derive(Deserialize)]
pub struct Typed {
    value: String,
}

/// For other plugins: a time typed by the caller (in their zone unless it says `Z`) as UTC, `2026-11-02T13:00:00Z`.
pub fn typed_to_utc(input: Typed) -> Result<Value> {
    let moment = crate::events::norm_datetime(&Viewer::me()?, "the time", &input.value)?;
    Ok(json!({ "utc": dates::format_datetime(moment) }))
}

#[derive(Deserialize)]
pub struct Stored {
    values: Vec<String>,
}

/// For other plugins: stored times as the caller reads them, `2026-11-02 14:00`, in the order given (a text that is not a time is given back).
pub fn read_times(input: Stored) -> Result<Vec<Value>> {
    let viewer = Viewer::me()?;
    Ok(input
        .values
        .iter()
        .take(500)
        .map(|v| match dates::parse_datetime(v) {
            Ok(utc) => json!(viewer.clock(utc).format("%Y-%m-%d %H:%M").to_string()),
            Err(_) => json!(v),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        dates::parse_datetime(text).unwrap()
    }

    #[test]
    fn zones_are_found_by_name() {
        assert!(parse_zone("Europe/Oslo").is_some());
        assert!(parse_zone("europe/oslo").is_some());
        assert!(parse_zone("Mars/Base").is_none());
        assert!(parse_zone("").is_none());
    }

    #[test]
    fn local_times_become_utc_across_the_clock_change() {
        let oslo = parse_zone("Europe/Oslo").unwrap();
        assert_eq!(local_to_utc(at("2026-07-01T09:00:00Z"), oslo), at("2026-07-01T07:00:00Z"));
        assert_eq!(local_to_utc(at("2026-12-01T09:00:00Z"), oslo), at("2026-12-01T08:00:00Z"));
        assert_eq!(utc_to_local(at("2026-12-01T08:00:00Z"), oslo), at("2026-12-01T09:00:00Z"));
    }

    #[test]
    fn a_time_that_does_not_exist_lands_after_the_jump() {
        let oslo = parse_zone("Europe/Oslo").unwrap();
        // 2026-03-29 02:30 never happens in Oslo; the clocks go from 02:00 to 03:00.
        assert_eq!(local_to_utc(at("2026-03-29T02:30:00Z"), oslo), at("2026-03-29T01:30:00Z"));
    }

    #[test]
    fn a_person_without_a_zone_sees_stored_times() {
        let v = Viewer::utc();
        assert_eq!(v.show(at("2026-10-20T09:00:00Z")), "2026-10-20T09:00:00Z");
        assert_eq!(v.to_utc(at("2026-10-20T09:00:00Z")), at("2026-10-20T09:00:00Z"));
        let tokyo = Viewer::in_zone(parse_zone("Asia/Tokyo").unwrap());
        assert_eq!(tokyo.show(at("2026-10-20T00:00:00Z")), "2026-10-20T09:00:00");
    }
}
