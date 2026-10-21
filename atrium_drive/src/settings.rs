//! The organization's limits for the drive, set by a drive administrator.

use aether_sdk::prelude::*;

use crate::common::*;

/// Who sets the limits: the organization's administrators. (A role of the plugin's own would close the drive's pages to
/// everyone who does not hold it.)
pub const ADMIN_ROLE: &str = "org_admin";

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// MB each person may store; 0 for no limit.
    pub quota_mb: i64,
    pub links_enabled: bool,
    /// Days a new link works; 0 for never.
    pub link_days: i64,
    /// The longest a link may work; 0 for no limit.
    pub link_days_max: i64,
    pub search_contents: bool,
    pub comments_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { quota_mb: 0, links_enabled: true, link_days: 30, link_days_max: 0, search_contents: true, comments_enabled: true }
    }
}

fn read(row: &Record) -> Settings {
    let d = Settings::default();
    let number = |name: &str, fallback: i64| row.get(name).and_then(Value::as_i64).unwrap_or(fallback);
    let on = |name: &str, fallback: bool| row.get(name).and_then(Value::as_bool).unwrap_or(fallback);
    Settings {
        quota_mb: number("quota_mb", d.quota_mb),
        links_enabled: on("links_enabled", d.links_enabled),
        link_days: number("link_days", d.link_days),
        link_days_max: number("link_days_max", d.link_days_max),
        search_contents: on("search_contents", d.search_contents),
        comments_enabled: on("comments_enabled", d.comments_enabled),
    }
}

/// The settings now in force (the defaults until an administrator saves some).
pub fn current() -> Result<Settings> {
    Ok(db::find::<Record>("drive_setting").first()?.map(|row| read(&row)).unwrap_or_default())
}

pub fn quota_bytes(settings: &Settings) -> i64 {
    settings.quota_mb.max(0).saturating_mul(1024 * 1024)
}

/// The form's values.
pub fn get_settings() -> Result<Value> {
    context::current()?.require_role(ADMIN_ROLE)?;
    let s = current()?;
    Ok(json!({
        "quota_mb": s.quota_mb, "links_enabled": s.links_enabled, "link_days": s.link_days, "link_days_max": s.link_days_max,
        "search_contents": s.search_contents, "comments_enabled": s.comments_enabled,
    }))
}

#[derive(Deserialize, Default)]
pub struct Change {
    #[serde(default)]
    quota_mb: Option<i64>,
    #[serde(default)]
    links_enabled: Option<bool>,
    #[serde(default)]
    link_days: Option<i64>,
    #[serde(default)]
    link_days_max: Option<i64>,
    #[serde(default)]
    search_contents: Option<bool>,
    #[serde(default)]
    comments_enabled: Option<bool>,
}

fn check_days(name: &str, value: i64) -> Result<()> {
    if !(0..=3650).contains(&value) {
        return Err(Error::msg(format!("{name} is between 0 and 3650 days")));
    }
    Ok(())
}

pub fn save_settings(input: Change) -> Result<Value> {
    context::current()?.require_role(ADMIN_ROLE)?;
    let mut data = serde_json::Map::new();
    if let Some(q) = input.quota_mb {
        if !(0..=100_000_000).contains(&q) {
            return Err(Error::msg("the quota is a number of MB, 0 or more"));
        }
        data.insert("quota_mb".into(), json!(q));
    }
    if let Some(d) = input.link_days {
        check_days("the default link life", d)?;
        data.insert("link_days".into(), json!(d));
    }
    if let Some(d) = input.link_days_max {
        check_days("the longest link life", d)?;
        data.insert("link_days_max".into(), json!(d));
    }
    for (name, value) in [("links_enabled", input.links_enabled), ("search_contents", input.search_contents), ("comments_enabled", input.comments_enabled)] {
        if let Some(v) = value {
            data.insert(name.into(), json!(v));
        }
    }
    let before = current()?;
    let (days, max) = (data.get("link_days").and_then(Value::as_i64).unwrap_or(before.link_days), data.get("link_days_max").and_then(Value::as_i64).unwrap_or(before.link_days_max));
    if max > 0 && (days == 0 || days > max) {
        return Err(Error::msg("the default link life cannot be longer than the longest one"));
    }
    match db::find::<Record>("drive_setting").first()? {
        Some(existing) => {
            db::update::<Record>("drive_setting", key(id_text(&existing)), &Value::Object(data))?;
        }
        None => {
            db::create::<Record>("drive_setting", &Value::Object(data))?;
        }
    }
    get_settings()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_saved_gives_the_defaults() {
        let s = read(&json!({}));
        assert_eq!(s, Settings::default());
        assert_eq!(quota_bytes(&s), 0);
    }

    #[test]
    fn saved_values_are_read_back() {
        let s = read(&json!({ "quota_mb": 5, "links_enabled": false, "link_days": 7, "link_days_max": 30, "search_contents": false, "comments_enabled": false }));
        assert_eq!((s.quota_mb, s.links_enabled, s.link_days, s.link_days_max, s.search_contents, s.comments_enabled), (5, false, 7, 30, false, false));
        assert_eq!(quota_bytes(&s), 5 * 1024 * 1024);
    }
}
