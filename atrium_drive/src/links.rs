//! Sharing by link: anyone in the organization who has the link can open a file or folder (for viewing), until the link
//! expires or its owner takes it back. The link's token is the key of its record, which the kernel makes unguessable.

use aether_sdk::{
    dates::{format_date, parse_date, Duration},
    db::Filter,
    prelude::*,
};

use crate::access::{in_trash, parent_of, require, Kind, Level};
use crate::browse::{file_row, folder_row};
use crate::common::*;
use crate::settings;

const MAX_DEPTH: usize = 40;

/// The last day a link made today with `days` works; empty for a link that does not expire.
pub fn expiry(today: &str, days: i64) -> Result<String> {
    if days <= 0 {
        return Ok(String::new());
    }
    Ok(format_date(parse_date(today)? + Duration::days(days)))
}

/// Whether a link with this last day has stopped working on `today`.
pub fn expired(expires_at: &str, today: &str) -> bool {
    !expires_at.is_empty() && expires_at < today
}

#[derive(Deserialize)]
pub struct NewLink {
    kind: String,
    id: String,
    #[serde(default)]
    days: Option<i64>,
}

pub fn create_link(input: NewLink) -> Result<Value> {
    let me = me()?;
    let s = settings::current()?;
    if !s.links_enabled {
        return Err(Error::msg("sharing by link is switched off for this organization"));
    }
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let days = input.days.unwrap_or(s.link_days);
    if !(0..=3650).contains(&days) {
        return Err(Error::msg("a link works for 0 to 3650 days (0: it does not expire)"));
    }
    if s.link_days_max > 0 && (days == 0 || days > s.link_days_max) {
        return Err(Error::msg(format!("a link works for at most {} days here", s.link_days_max)));
    }
    let ctx = context::current()?;
    let expires_at = expiry(ctx.today(), days)?;
    let link: Record = db::create(
        "drive_link",
        &json!({ "item": key(id_text(&item)), "kind": kind.name(), "created_by": me, "expires_at": expires_at, "revoked": false, "at": now()? }),
    )?;
    Ok(json!({ "id": key(id_text(&link)), "path": format!("/drive/link/{}", key(id_text(&link))), "expires_at": expires_at }))
}

#[derive(Deserialize)]
pub struct ItemOnly {
    kind: String,
    id: String,
}

pub fn list_links(input: ItemOnly) -> Result<Vec<Value>> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let today = context::current()?.today().to_string();
    let links: Vec<Record> = db::find("drive_link").filter("item", key(id_text(&item))).order_by("-at").limit(200).all()?;
    Ok(links
        .iter()
        .map(|l| {
            let until = text(l, "expires_at").unwrap_or_default();
            json!({
                "id": key(id_text(l)), "path": format!("/drive/link/{}", key(id_text(l))),
                "expires_at": until, "expired": expired(until, &today),
                "text": if until.is_empty() { "Does not expire".to_string() } else { format!("Works until {until}") },
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct LinkOnly {
    id: String,
}

pub fn revoke_link(input: LinkOnly) -> Result<Value> {
    let link = db::get::<Record>("drive_link", key(&input.id))?.ok_or_else(|| Error::msg("there is no such link"))?;
    let kind = Kind::parse(text(&link, "kind").unwrap_or_default())?;
    require(kind, text(&link, "item").unwrap_or_default(), Level::Owner)?;
    db::delete::<Record>("drive_link", key(id_text(&link)))?;
    Ok(json!({ "revoked": true }))
}

/// A link that works now, with the item it opens: the link has not been taken back or run out, links are switched on,
/// and the item exists and is not in the trash.
pub fn active(token: &str) -> Result<Option<(Record, Kind, Record)>> {
    if !settings::current()?.links_enabled {
        return Ok(None);
    }
    let Some(link) = db::get::<Record>("drive_link", key(token))? else { return Ok(None) };
    let today = context::current()?.today().to_string();
    if flag(&link, "revoked") || expired(text(&link, "expires_at").unwrap_or_default(), &today) {
        return Ok(None);
    }
    let kind = Kind::parse(text(&link, "kind").unwrap_or_default())?;
    let Some(item) = db::get::<Record>(kind.model(), text(&link, "item").map(key).unwrap_or_default())? else { return Ok(None) };
    if in_trash(kind, &item)? {
        return Ok(None);
    }
    Ok(Some((link, kind, item)))
}

/// Whether a link that works now covers this file: a link on the file, or on a folder above it.
pub fn covers_file(file: &Record) -> Result<bool> {
    if !settings::current()?.links_enabled {
        return Ok(false);
    }
    let mut ids = vec![key(id_text(file)).to_string()];
    let mut parent = parent_of(Kind::File, file);
    for _ in 0..MAX_DEPTH {
        if parent.is_empty() {
            break;
        }
        ids.push(parent.clone());
        parent = match db::get::<Record>("drive_folder", &parent)? {
            Some(f) => parent_of(Kind::Folder, &f),
            None => break,
        };
    }
    let today = context::current()?.today().to_string();
    Ok(db::find::<Record>("drive_link")
        .matching(Filter::one_of("item", ids))
        .limit(200)
        .all()?
        .iter()
        .any(|l| !flag(l, "revoked") && !expired(text(l, "expires_at").unwrap_or_default(), &today)))
}

#[derive(Deserialize)]
pub struct Open {
    token: String,
    #[serde(default)]
    folder: Option<String>,
}

/// What a link opens, shaped like a folder listing so the same browser draws it. Everything is view-only.
pub fn open_link(input: Open) -> Result<Value> {
    me()?;
    let gone = || Error::msg("this link does not work any more");
    let (_, kind, item) = active(&input.token)?.ok_or_else(gone)?;
    let people = org_people()?;
    let token = key(&input.token).to_string();
    if kind == Kind::File {
        return Ok(json!({
            "view": "link", "token": token, "folder": null, "path": [], "can_write": false, "folders": [],
            "files": [file_row(&item, Level::View, &people, false)],
        }));
    }
    let root = key(id_text(&item)).to_string();
    let wanted = input.folder.as_deref().map(key).filter(|f| !f.is_empty()).unwrap_or(&root).to_string();
    // The folder asked for must be the link's own or inside it.
    let mut chain: Vec<Record> = Vec::new();
    let mut at = wanted.clone();
    let mut inside = false;
    for _ in 0..MAX_DEPTH {
        let Some(folder) = db::get::<Record>("drive_folder", &at)? else { break };
        let next = parent_of(Kind::Folder, &folder);
        let is_root = key(id_text(&folder)) == root;
        chain.push(folder);
        if is_root {
            inside = true;
            break;
        }
        if next.is_empty() {
            break;
        }
        at = next;
    }
    if !inside || chain.iter().any(|f| flag(f, "trashed")) {
        return Err(gone());
    }
    let here = chain[0].clone();
    chain.reverse();
    let path: Vec<Value> = chain.iter().map(|f| json!({ "id": key(id_text(f)), "name": text(f, "name") })).collect();
    let subfolders: Vec<Record> = db::find("drive_folder").filter("parent", wanted.as_str()).filter("trashed", false).limit(1000).all()?;
    let files: Vec<Record> = db::find("drive_file").filter("folder", wanted.as_str()).filter("trashed", false).limit(1000).all()?;
    let mut folders: Vec<Value> = subfolders.iter().map(|f| folder_row(f, Level::View, &people, false)).collect();
    let mut file_rows: Vec<Value> = files.iter().map(|f| file_row(f, Level::View, &people, false)).collect();
    folders.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    file_rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(json!({
        "view": "link", "token": token, "folder": { "id": wanted, "name": text(&here, "name"), "level": "view" },
        "path": path, "can_write": false, "folders": folders, "files": file_rows,
    }))
}

/// Take away every link on an item that is being deleted for good.
pub fn drop_for(item: &str) -> Result<()> {
    for link in db::find::<Record>("drive_link").filter("item", item).limit(500).all()? {
        db::delete::<Record>("drive_link", key(id_text(&link)))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_runs_out_the_day_after_its_last_day() {
        assert_eq!(expiry("2026-10-18", 30).unwrap(), "2026-11-17");
        assert_eq!(expiry("2026-10-18", 0).unwrap(), "");
        assert!(!expired("2026-11-17", "2026-11-17"));
        assert!(expired("2026-11-17", "2026-11-18"));
        assert!(!expired("", "2099-01-01"));
    }
}
