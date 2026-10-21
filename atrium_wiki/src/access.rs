//! Who may read and write a space: its people. Owners manage, editors write pages, readers read. An open space can be
//! read by everyone in the organization.

use aether_sdk::prelude::*;

use crate::common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Reader,
    Editor,
    Owner,
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Reader => "reader",
        Role::Editor => "editor",
        Role::Owner => "owner",
    }
}

pub fn parse_role(text: &str) -> Result<Role> {
    match text {
        "owner" => Ok(Role::Owner),
        "editor" => Ok(Role::Editor),
        "reader" => Ok(Role::Reader),
        _ => Err(Error::msg("a role is owner, editor or reader")),
    }
}

pub fn members_of(space: &str) -> Result<Vec<Record>> {
    db::find("wiki_member").filter("space", key(space)).limit(500).all()
}

pub fn member_row(space: &str, user: &str) -> Result<Option<Record>> {
    Ok(members_of(space)?.into_iter().find(|m| text(m, "user").is_some_and(|u| key(u) == key(user))))
}

/// The role a member has; nothing for someone only passing through an open space.
pub fn role_of(space: &str, user: &str) -> Result<Option<Role>> {
    Ok(member_row(space, user)?.map(|m| parse_role(text(&m, "role").unwrap_or("reader")).unwrap_or(Role::Reader)))
}

/// The space and what the caller can do in it, at least `min`. A space the caller cannot read does not exist for them.
pub fn require(space: &str, min: Role) -> Result<(Record, Role, String)> {
    let me = me()?;
    let record = db::get::<Record>("wiki_space", key(space))?.ok_or_else(|| Error::msg("there is no such space"))?;
    let member = role_of(key(id_text(&record)), &me)?;
    let role = match member {
        Some(r) => r,
        None if flag(&record, "open") => Role::Reader,
        None => return Err(Error::msg("there is no such space")),
    };
    if role < min {
        return Err(Error::msg(match min {
            Role::Owner => "only an owner of the space can do this",
            _ => "you can only read this space",
        }));
    }
    Ok((record, role, me))
}

/// Keys of the spaces the caller reads: their own and the open ones.
pub fn my_space_keys(me: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let rows: Vec<Record> = db::find("wiki_member").limit(10000).all()?;
    for m in rows.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(me))) {
        if let Some(s) = text(m, "space") {
            out.push(key(s).to_string());
        }
    }
    for s in db::find::<Record>("wiki_space").limit(2000).all()? {
        if flag(&s, "open") && !out.iter().any(|k| k == key(id_text(&s))) {
            out.push(key(id_text(&s)).to_string());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_outrank_editors_outrank_readers() {
        assert!(Role::Owner > Role::Editor && Role::Editor > Role::Reader);
        assert_eq!(parse_role("reader").unwrap(), Role::Reader);
        assert!(parse_role("boss").is_err());
    }
}
