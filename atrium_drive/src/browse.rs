//! Listing: a folder, everything shared with the caller, what they changed lately, the trash, search.

use std::collections::HashMap;

use aether_sdk::{db::Filter, prelude::*};

use crate::access::{in_trash, level_of, parent_of, path_of, shares_for, Kind, Level};
use crate::common::*;

const LIST_LIMIT: u32 = 1000;

#[derive(Deserialize, Default)]
pub struct Browse {
    #[serde(default)]
    folder: Option<String>,
    /// `mine` (the default), `shared`, `recent` or `trash`.
    #[serde(default)]
    view: Option<String>,
    /// The page's query values; `view` is read from here when it is not given directly.
    #[serde(default)]
    query: Option<Value>,
}

pub fn folder_row(f: &Record, level: Level, people: &[Person], shared: bool) -> Value {
    json!({
        "id": key(id_text(f)), "kind": "folder", "name": text(f, "name"), "owner": text(f, "owner"),
        "owner_name": name_of(people, text(f, "owner").unwrap_or_default()), "modified_at": text(f, "modified_at"),
        "level": level.name(), "shared": shared, "trashed": flag(f, "trashed"),
    })
}

pub fn file_row(f: &Record, level: Level, people: &[Person], shared: bool) -> Value {
    json!({
        "id": key(id_text(f)), "kind": "file", "name": text(f, "name"), "owner": text(f, "owner"),
        "owner_name": name_of(people, text(f, "owner").unwrap_or_default()), "mime": text(f, "mime"), "size": int(f, "size"),
        "version": int(f, "version"), "modified_at": text(f, "modified_at"), "modified_by": name_of(people, text(f, "modified_by").unwrap_or_default()),
        "media": f.get("media"), "level": level.name(), "shared": shared, "trashed": flag(f, "trashed"),
    })
}

fn by_name(rows: &mut [Value]) {
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
}

/// Which of these items are shared with anyone (so the list can show it).
fn shared_flags(ids: &[String]) -> Result<HashMap<String, bool>> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    for s in db::find::<Record>("drive_share").matching(Filter::one_of("item", ids.to_vec())).limit(2000).all()? {
        if let Some(item) = text(&s, "item") {
            out.insert(key(item).to_string(), true);
        }
    }
    Ok(out)
}

/// A listing with what the browser also needs: which rows the caller starred, and which features are on.
fn decorate(mut listing: Value, me: &str) -> Result<Value> {
    let stars = crate::social::favourite_ids(me)?;
    for list in ["folders", "files"] {
        if let Some(rows) = listing.get_mut(list).and_then(Value::as_array_mut) {
            for row in rows {
                let starred = row["id"].as_str().is_some_and(|id| stars.contains(id));
                row["favourite"] = json!(starred);
            }
        }
    }
    let s = crate::settings::current()?;
    listing["features"] = json!({ "links": s.links_enabled, "comments": s.comments_enabled, "settings": context::current()?.has_role(crate::settings::ADMIN_ROLE) });
    Ok(listing)
}

pub fn browse(input: Browse) -> Result<Value> {
    let me = me()?;
    decorate(browse_places(input)?, &me)
}

fn browse_places(input: Browse) -> Result<Value> {
    let me = me()?;
    let people = org_people()?;
    let from_query = input.query.as_ref().and_then(|q| q.get("view")).and_then(Value::as_str);
    let view = input.view.as_deref().or(from_query).unwrap_or("mine");
    let folder_key = input.folder.as_deref().map(str::trim).filter(|f| !f.is_empty());
    if let Some(folder_key) = folder_key {
        let (folder, level) = crate::access::require(Kind::Folder, folder_key, Level::View)?;
        let fk = key(id_text(&folder)).to_string();
        let subfolders: Vec<Record> = db::find("drive_folder").filter("parent", fk.as_str()).filter("trashed", false).limit(LIST_LIMIT).all()?;
        let files: Vec<Record> = db::find("drive_file").filter("folder", fk.as_str()).filter("trashed", false).limit(LIST_LIMIT).all()?;
        let ids: Vec<String> = subfolders.iter().chain(files.iter()).map(|r| key(id_text(r)).to_string()).collect();
        let flags = shared_flags(&ids)?;
        let mut folders: Vec<Value> = subfolders.iter().map(|f| folder_row(f, level.max(Level::View), &people, flags.contains_key(key(id_text(f))))).collect();
        let mut file_rows: Vec<Value> = files.iter().map(|f| file_row(f, level, &people, flags.contains_key(key(id_text(f))))).collect();
        by_name(&mut folders);
        by_name(&mut file_rows);
        let path: Vec<Value> = path_of(&folder, &me)?.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect();
        // Items inside a folder the caller can read take the folder's level, or better.
        return Ok(json!({
            "view": "folder",
            "folder": { "id": fk, "name": text(&folder, "name"), "level": level.name(), "owner_name": name_of(&people, text(&folder, "owner").unwrap_or_default()) },
            "path": path, "can_write": level.can_write(), "folders": folders, "files": file_rows,
        }));
    }
    match view {
        "shared" => shared_with_me(&me, &people),
        "recent" => recent(&me, &people),
        "favourites" => favourites(&me, &people),
        "trash" => trash_view(&me, &people),
        _ => {
            let subfolders: Vec<Record> = db::find("drive_folder").filter("owner", me.as_str()).filter("parent", "").filter("trashed", false).limit(LIST_LIMIT).all()?;
            let files: Vec<Record> = db::find("drive_file").filter("owner", me.as_str()).filter("folder", "").filter("trashed", false).limit(LIST_LIMIT).all()?;
            let ids: Vec<String> = subfolders.iter().chain(files.iter()).map(|r| key(id_text(r)).to_string()).collect();
            let flags = shared_flags(&ids)?;
            let mut folders: Vec<Value> = subfolders.iter().map(|f| folder_row(f, Level::Owner, &people, flags.contains_key(key(id_text(f))))).collect();
            let mut file_rows: Vec<Value> = files.iter().map(|f| file_row(f, Level::Owner, &people, flags.contains_key(key(id_text(f))))).collect();
            by_name(&mut folders);
            by_name(&mut file_rows);
            Ok(json!({ "view": "mine", "folder": null, "path": [], "can_write": true, "folders": folders, "files": file_rows }))
        }
    }
}

fn shared_with_me(me: &str, people: &[Person]) -> Result<Value> {
    let shares: Vec<Record> = db::find::<Record>("drive_share").limit(2000).all()?;
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for share in shares.iter().filter(|s| text(s, "user").is_some_and(|u| u == "org" || key(u) == key(me))) {
        let kind = Kind::parse(text(share, "kind").unwrap_or_default())?;
        let Some(item) = db::get::<Record>(kind.model(), text(share, "item").map(key).unwrap_or_default())? else { continue };
        if text(&item, "owner").is_some_and(|o| key(o) == key(me)) || in_trash(kind, &item)? {
            continue;
        }
        let level = level_of(kind, &item, me)?.unwrap_or(Level::View);
        match kind {
            Kind::Folder => folders.push(folder_row(&item, level, people, true)),
            Kind::File => files.push(file_row(&item, level, people, true)),
        }
    }
    by_name(&mut folders);
    by_name(&mut files);
    Ok(json!({ "view": "shared", "folder": null, "path": [], "can_write": false, "folders": folders, "files": files }))
}

fn favourites(me: &str, people: &[Person]) -> Result<Value> {
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for star in db::find::<Record>("drive_favourite").filter("user", me).limit(2000).all()? {
        let Ok(kind) = Kind::parse(text(&star, "kind").unwrap_or_default()) else { continue };
        let Some(item) = db::get::<Record>(kind.model(), text(&star, "item").map(key).unwrap_or_default())? else { continue };
        let Some(level) = level_of(kind, &item, me)? else { continue };
        if in_trash(kind, &item)? {
            continue;
        }
        match kind {
            Kind::Folder => folders.push(folder_row(&item, level, people, false)),
            Kind::File => files.push(file_row(&item, level, people, false)),
        }
    }
    by_name(&mut folders);
    by_name(&mut files);
    Ok(json!({ "view": "favourites", "folder": null, "path": [], "can_write": false, "folders": folders, "files": files }))
}

fn recent(me: &str, people: &[Person]) -> Result<Value> {
    let files: Vec<Record> = db::find("drive_file").filter("owner", me).filter("trashed", false).order_by("-modified_at").limit(30).all()?;
    let rows: Vec<Value> = files.iter().map(|f| file_row(f, Level::Owner, people, false)).collect();
    Ok(json!({ "view": "recent", "folder": null, "path": [], "can_write": false, "folders": [], "files": rows }))
}

fn trash_view(me: &str, people: &[Person]) -> Result<Value> {
    let folders: Vec<Record> = db::find("drive_folder").filter("owner", me).filter("trashed", true).limit(LIST_LIMIT).all()?;
    let files: Vec<Record> = db::find("drive_file").filter("owner", me).filter("trashed", true).limit(LIST_LIMIT).all()?;
    let mut folder_rows: Vec<Value> = folders.iter().map(|f| folder_row(f, Level::Owner, people, false)).collect();
    let mut file_rows: Vec<Value> = files.iter().map(|f| file_row(f, Level::Owner, people, false)).collect();
    by_name(&mut folder_rows);
    by_name(&mut file_rows);
    Ok(json!({ "view": "trash", "folder": null, "path": [], "can_write": false, "folders": folder_rows, "files": file_rows }))
}

#[derive(Deserialize, Default)]
pub struct Search {
    #[serde(default)]
    q: Option<String>,
}

/// Files and folders whose name contains the text, and files whose text does, among those the caller may see.
pub fn search(input: Search) -> Result<Value> {
    let me = me()?;
    decorate(search_places(input)?, &me)
}

fn search_places(input: Search) -> Result<Value> {
    let me = me()?;
    let people = org_people()?;
    let needle = input.q.unwrap_or_default().trim().to_string();
    if needle.chars().count() < 2 {
        return Ok(json!({ "view": "search", "folder": null, "path": [], "can_write": false, "folders": [], "files": [] }));
    }
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for f in db::find::<Record>("drive_folder").matching(Filter::like("name", &needle).and(Filter::eq("trashed", false))).limit(300).all()? {
        if let Some(level) = level_of(Kind::Folder, &f, &me)? {
            if !in_trash(Kind::Folder, &f)? {
                folders.push(folder_row(&f, level, &people, false));
            }
        }
    }
    let mut matcher = Filter::like("name", &needle);
    if crate::settings::current()?.search_contents {
        matcher = matcher.or(Filter::like("content", &needle));
    }
    for f in db::find::<Record>("drive_file").matching(matcher.and(Filter::eq("trashed", false))).limit(300).all()? {
        if let Some(level) = level_of(Kind::File, &f, &me)? {
            if !in_trash(Kind::File, &f)? {
                let mut row = file_row(&f, level, &people, false);
                let in_name = text(&f, "name").is_some_and(|n| n.to_lowercase().contains(&needle.to_lowercase()));
                row["found_in"] = json!(if in_name { "name" } else { "text" });
                files.push(row);
            }
        }
    }
    by_name(&mut folders);
    by_name(&mut files);
    Ok(json!({ "view": "search", "folder": null, "path": [], "can_write": false, "folders": folders, "files": files }))
}

/// Folders the caller may write to, with their full path, for choosing where to move something.
pub fn folder_tree() -> Result<Vec<Value>> {
    let me = me()?;
    let all: Vec<Record> = db::find("drive_folder").filter("trashed", false).limit(2000).all()?;
    let by_id: HashMap<String, &Record> = all.iter().map(|f| (key(id_text(f)).to_string(), f)).collect();
    let mut rows = Vec::new();
    for f in &all {
        let Some(level) = level_of(Kind::Folder, f, &me)? else { continue };
        if !level.can_write() || in_trash(Kind::Folder, f)? {
            continue;
        }
        let mut names = vec![text(f, "name").unwrap_or_default().to_string()];
        let mut parent = parent_of(Kind::Folder, f);
        for _ in 0..40 {
            let Some(p) = by_id.get(&parent) else { break };
            names.push(text(p, "name").unwrap_or_default().to_string());
            parent = parent_of(Kind::Folder, p);
        }
        names.reverse();
        rows.push(json!({ "id": key(id_text(f)), "name": names.join(" / ") }));
    }
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

/// How much the caller has stored, in bytes (every stored object once), and the organization's limit (0: none).
pub fn storage_used() -> Result<Value> {
    let me = me()?;
    let blobs = db::find::<Record>("drive_blob").filter("uploaded_by", me.as_str()).count()?;
    let quota = crate::settings::quota_bytes(&crate::settings::current()?);
    Ok(json!({ "bytes": crate::quota::used_bytes(&me)?, "versions": blobs, "quota_bytes": quota }))
}

#[derive(Deserialize)]
pub struct FileInfo {
    file: String,
}

pub fn file_info(input: FileInfo) -> Result<Value> {
    let me = me()?;
    let (file, level) = crate::access::require(Kind::File, &input.file, Level::View)?;
    let people = org_people()?;
    let mut path = Vec::new();
    let folder = parent_of(Kind::File, &file);
    if !folder.is_empty() {
        if let Some(f) = db::get::<Record>("drive_folder", &folder)? {
            path = path_of(&f, &me)?.into_iter().map(|(id, name)| json!({ "id": id, "name": name })).collect();
        }
    }
    let versions = db::find::<Record>("drive_blob").filter("file", key(id_text(&file))).count()?;
    let shared = if level == Level::Owner { shares_for(id_text(&file))?.len() } else { 0 };
    let mut row = file_row(&file, level, &people, shared > 0);
    row["path"] = json!(path);
    row["versions"] = json!(versions);
    row["shares"] = json!(shared);
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_sort_by_name_ignoring_case() {
        let mut rows = vec![json!({ "name": "banana" }), json!({ "name": "Apple" }), json!({ "name": "cherry" })];
        by_name(&mut rows);
        assert_eq!(rows.iter().map(|r| r["name"].as_str().unwrap_or_default()).collect::<Vec<_>>(), ["Apple", "banana", "cherry"]);
    }
}

/// The places in the drive, for the list on the left.
pub fn nav() -> Result<Vec<Value>> {
    Ok(vec![
        json!({ "id": "mine", "name": "My files" }),
        json!({ "id": "favourites", "name": "Favourites" }),
        json!({ "id": "shared", "name": "Shared with me" }),
        json!({ "id": "recent", "name": "Recent" }),
        json!({ "id": "trash", "name": "Trash" }),
    ])
}

/// People to share with: everyone but the caller.
pub fn list_people() -> Result<Vec<Value>> {
    let me = me()?;
    Ok(org_people()?.into_iter().filter(|p| key(&p.id) != key(&me)).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}
