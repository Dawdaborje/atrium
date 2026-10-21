//! Sharing a file or folder with a person, or with everyone in the organization.

use aether_sdk::prelude::*;

use crate::access::{require, shares_for, Kind, Level};
use crate::common::*;

#[derive(Deserialize)]
pub struct ShareIn {
    kind: String,
    id: String,
    /// A person's id, or `org` for everyone.
    user: String,
    #[serde(default)]
    level: Option<String>,
}

pub fn share(input: ShareIn) -> Result<Value> {
    let me = me()?;
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let level = Level::parse(input.level.as_deref().unwrap_or("view")).ok_or_else(|| Error::msg("the level is `view` or `edit`"))?;
    let people = org_people()?;
    let user = if input.user == "org" {
        "org".to_string()
    } else {
        people.iter().find(|p| key(&p.id) == key(&input.user)).map(|p| p.id.clone()).ok_or_else(|| Error::msg("that person is not a member of this organization"))?
    };
    if user != "org" && key(&user) == key(&me) {
        return Err(Error::msg("you already own this"));
    }
    let item_key = key(id_text(&item)).to_string();
    let existing = shares_for(&item_key)?.into_iter().find(|s| text(s, "user") == Some(user.as_str()));
    match existing {
        Some(share) => {
            db::update::<Record>("drive_share", key(id_text(&share)), &json!({ "level": level.name() }))?;
        }
        None => {
            db::create::<Record>(
                "drive_share",
                &json!({ "item": item_key, "kind": kind.name(), "user": user, "level": level.name(), "granted_by": me }),
            )?;
            if user != "org" {
                let name = text(&item, "name").unwrap_or("an item");
                let from = name_of(&people, &me);
                let folder = match kind {
                    Kind::Folder => item_key.clone(),
                    Kind::File => text(&item, "folder").map(|f| key(f).to_string()).unwrap_or_default(),
                };
                Notification::new(format!("{from} shared {name} with you"))
                    .link(if kind == Kind::Folder { format!("/drive/{folder}") } else { "/drive/shared".to_string() })
                    .to(Audience::Actors(vec![user.clone()]))
                    .send()?;
            }
        }
    }
    Ok(json!({ "shared": true, "user": user, "level": level.name() }))
}

#[derive(Deserialize)]
pub struct Unshare {
    kind: String,
    id: String,
    user: String,
}

pub fn unshare(input: Unshare) -> Result<Value> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let mut removed = 0;
    for share in shares_for(id_text(&item))? {
        if text(&share, "user").is_some_and(|u| u == input.user || (u != "org" && key(u) == key(&input.user))) {
            db::delete::<Record>("drive_share", key(id_text(&share)))?;
            removed += 1;
        }
    }
    Ok(json!({ "removed": removed }))
}

#[derive(Deserialize)]
pub struct ItemOnly {
    kind: String,
    id: String,
}

/// Who an item is shared with, and the owner. Only the owner sees this.
pub fn list_shares(input: ItemOnly) -> Result<Vec<Value>> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let people = org_people()?;
    Ok(shares_for(id_text(&item))?
        .iter()
        .map(|s| {
            let user = text(s, "user").unwrap_or_default();
            json!({
                "id": user, "user": user, "level": text(s, "level"),
                "name": if user == "org" { "Everyone in the organization".to_string() } else { name_of(&people, user) },
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_view_and_edit_can_be_granted() {
        assert!(Level::parse("owner").is_none());
        assert!(Level::parse("view").is_some() && Level::parse("edit").is_some());
    }
}
