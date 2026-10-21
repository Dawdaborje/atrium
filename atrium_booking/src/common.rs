//! Helpers shared by the parts of the plugin.

pub use aether_sdk::records::{text, Record};

use aether_sdk::{prelude::*, transport};

pub fn key(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, key)| key)
}

pub fn me() -> Result<String> {
    context::current()?.actor.id.ok_or_else(|| Error::msg("sign in to use booking"))
}

pub fn now() -> Result<String> {
    let ctx = context::current()?;
    if ctx.now.is_empty() {
        return Err(Error::msg("the kernel did not say what time it is"));
    }
    Ok(ctx.now)
}

pub fn id_text(record: &Record) -> &str {
    text(record, "id").unwrap_or_default()
}

pub fn flag(record: &Record, field: &str) -> bool {
    record.get(field).and_then(Value::as_bool).unwrap_or(false)
}

pub fn clean(label: &str, value: &str, max: usize) -> Result<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(Error::msg(format!("{label} is needed")));
    }
    if v.chars().count() > max {
        return Err(Error::msg(format!("{label} is at most {max} characters")));
    }
    Ok(v.to_string())
}

/// The zone named `name`, if it is one (`Europe/Oslo`); case does not matter. Empty means UTC.
pub fn zone_of(name: &str) -> Option<chrono_tz::Tz> {
    let wanted = name.trim();
    if wanted.is_empty() {
        return Some(chrono_tz::UTC);
    }
    wanted.parse::<chrono_tz::Tz>().ok().or_else(|| chrono_tz::TZ_VARIANTS.iter().copied().find(|z| z.name().eq_ignore_ascii_case(wanted)))
}

/// The UTC moment of a wall-clock time in `zone`. A time that does not exist (the clocks jumped over it) is read an
/// hour later; one that exists twice is the first.
pub fn local_to_utc(local: aether_sdk::dates::NaiveDateTime, zone: chrono_tz::Tz) -> aether_sdk::dates::NaiveDateTime {
    use chrono::{LocalResult, TimeZone};
    match zone.from_local_datetime(&local) {
        LocalResult::Single(t) => t.naive_utc(),
        LocalResult::Ambiguous(first, _) => first.naive_utc(),
        LocalResult::None => match zone.from_local_datetime(&(local + aether_sdk::dates::Duration::hours(1))) {
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => t.naive_utc(),
            LocalResult::None => local,
        },
    }
}

pub fn utc_to_local(utc: aether_sdk::dates::NaiveDateTime, zone: chrono_tz::Tz) -> aether_sdk::dates::NaiveDateTime {
    use chrono::TimeZone;
    zone.from_utc_datetime(&utc).naive_local()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: String,
    pub name: String,
}

/// Everyone who can sign in to this organization.
pub fn org_people() -> Result<Vec<Person>> {
    let reply = transport::call("org::members", &json!({}))?;
    Ok(reply
        .get("data")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let id = row.get("id")?.as_str()?.to_string();
                    let name = row.get("display_name").and_then(Value::as_str).unwrap_or(&id).to_string();
                    Some(Person { id, name })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn name_of(people: &[Person], user: &str) -> String {
    people
        .iter()
        .find(|p| key(&p.id) == key(user))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| key(user).chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_cleaned_and_limited() {
        assert_eq!(clean("a name", "  Plan  ", 10).unwrap(), "Plan");
        assert!(clean("a name", " ", 10).is_err());
        assert!(clean("a name", "toolongname", 5).is_err());
    }
}
