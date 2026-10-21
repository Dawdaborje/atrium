//! Who may see and change a calendar: its members. Owners manage it, editors change events, viewers only look.

use aether_sdk::prelude::*;

use crate::common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Editor,
    Owner,
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Viewer => "viewer",
        Role::Editor => "editor",
        Role::Owner => "owner",
    }
}

pub fn parse_role(text: &str) -> Result<Role> {
    match text {
        "owner" => Ok(Role::Owner),
        "editor" => Ok(Role::Editor),
        "viewer" => Ok(Role::Viewer),
        _ => Err(Error::msg("a role is owner, editor or viewer")),
    }
}

pub fn members_of(calendar: &str) -> Result<Vec<Record>> {
    db::find("cal_member").filter("calendar", key(calendar)).limit(500).all()
}

pub fn member_row(calendar: &str, user: &str) -> Result<Option<Record>> {
    Ok(members_of(calendar)?.into_iter().find(|m| text(m, "user").is_some_and(|u| key(u) == key(user))))
}

pub fn role_of(calendar: &str, user: &str) -> Result<Option<Role>> {
    Ok(member_row(calendar, user)?.map(|m| parse_role(text(&m, "role").unwrap_or("viewer")).unwrap_or(Role::Viewer)))
}

/// The calendar and the caller's role in it, at least `min`. A calendar the caller is not in does not exist for them.
pub fn require(calendar: &str, min: Role) -> Result<(Record, Role, String)> {
    let me = me()?;
    let record = db::get::<Record>("cal_calendar", key(calendar))?.ok_or_else(|| Error::msg("there is no such calendar"))?;
    match role_of(key(id_text(&record)), &me)? {
        Some(role) if role >= min => Ok((record, role, me)),
        Some(_) if min == Role::Owner => Err(Error::msg("only the owner of the calendar can do this")),
        Some(_) => Err(Error::msg("you can only look at this calendar")),
        None => Err(Error::msg("there is no such calendar")),
    }
}

/// The keys of the calendars the caller is in.
pub fn my_calendar_keys(me: &str) -> Result<Vec<String>> {
    let rows: Vec<Record> = db::find("cal_member").limit(5000).all()?;
    Ok(rows.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(me))).filter_map(|m| text(m, "calendar").map(str::to_string)).collect())
}

/// The caller's own calendar, made the first time it is needed.
pub fn ensure_personal(me: &str) -> Result<()> {
    for calendar in my_calendar_keys(me)? {
        if let Some(c) = db::get::<Record>("cal_calendar", &calendar)? {
            if flag(&c, "personal") && text(&c, "owner").is_some_and(|o| key(o) == key(me)) {
                return Ok(());
            }
        }
    }
    let calendar: Record = db::create("cal_calendar", &json!({ "name": "My calendar", "color": "#0f766e", "owner": me, "personal": true }))?;
    db::create::<Record>("cal_member", &json!({ "calendar": key(id_text(&calendar)), "user": me, "role": "owner" }))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_rank_owner_over_editor_over_viewer() {
        assert!(Role::Owner > Role::Editor && Role::Editor > Role::Viewer);
        assert_eq!(parse_role("editor").unwrap(), Role::Editor);
        assert!(parse_role("boss").is_err());
    }
}
