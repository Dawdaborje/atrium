//! Who may read and write a site: its people. Owners manage, editors write pages, visitors read. An open site can be
//! read by everyone in the organization.

use aether_sdk::prelude::*;

use crate::common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Visitor,
    Editor,
    Owner,
}

pub fn role_name(role: Role) -> &'static str {
    match role {
        Role::Visitor => "visitor",
        Role::Editor => "editor",
        Role::Owner => "owner",
    }
}

pub fn parse_role(text: &str) -> Result<Role> {
    match text {
        "owner" => Ok(Role::Owner),
        "editor" => Ok(Role::Editor),
        "visitor" => Ok(Role::Visitor),
        _ => Err(Error::msg("a role is owner, editor or visitor")),
    }
}

pub fn members_of(site: &str) -> Result<Vec<Record>> {
    db::find("site_member").filter("site", key(site)).limit(500).all()
}

pub fn member_row(site: &str, user: &str) -> Result<Option<Record>> {
    Ok(members_of(site)?.into_iter().find(|m| text(m, "user").is_some_and(|u| key(u) == key(user))))
}

/// The role a member has; nothing for someone only passing through an open site.
pub fn role_of(site: &str, user: &str) -> Result<Option<Role>> {
    Ok(member_row(site, user)?.map(|m| parse_role(text(&m, "role").unwrap_or("visitor")).unwrap_or(Role::Visitor)))
}

/// The site and what the caller can do in it, at least `min`. A site the caller cannot read does not exist for them.
pub fn require(site: &str, min: Role) -> Result<(Record, Role, String)> {
    let me = me()?;
    let record = db::get::<Record>("site_site", key(site))?.ok_or_else(|| Error::msg("there is no such site"))?;
    let member = role_of(key(id_text(&record)), &me)?;
    let role = match member {
        Some(r) => r,
        None if flag(&record, "open") => Role::Visitor,
        None => return Err(Error::msg("there is no such site")),
    };
    if role < min {
        return Err(Error::msg(match min {
            Role::Owner => "only an owner of the site can do this",
            _ => "you can only read this site",
        }));
    }
    Ok((record, role, me))
}

/// Keys of the sites the caller reads: their own and the open ones.
pub fn my_site_keys(me: &str) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let rows: Vec<Record> = db::find("site_member").limit(10000).all()?;
    for m in rows.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(me))) {
        if let Some(s) = text(m, "site") {
            out.push(key(s).to_string());
        }
    }
    for s in db::find::<Record>("site_site").limit(2000).all()? {
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
    fn owners_outrank_editors_outrank_visitors() {
        assert!(Role::Owner > Role::Editor && Role::Editor > Role::Visitor);
        assert_eq!(parse_role("visitor").unwrap(), Role::Visitor);
        assert!(parse_role("boss").is_err());
    }
}
