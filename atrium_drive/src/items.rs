//! Making, renaming, moving, trashing and versioning files and folders.

use aether_sdk::{db::Filter, prelude::*};

use crate::access::{in_trash, level_of, parent_of, require, Kind, Level};
use crate::common::*;

const MAX_PURGE: usize = 5000;

#[derive(Deserialize, Clone)]
pub struct MediaIn {
    key: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    mime: Option<String>,
    #[serde(default)]
    size: Option<u64>,
}

/// What the media store returned for an upload: a key under `media/`, with a name and a size.
fn check_media(media: &MediaIn) -> Result<Value> {
    if !media.key.starts_with("media/") || media.key.contains("..") || media.key.len() > 200 {
        return Err(Error::msg("the file was not uploaded through the media store"));
    }
    Ok(json!({
        "key": media.key,
        "name": media.name.clone().unwrap_or_default().chars().take(200).collect::<String>(),
        "mime": media.mime.clone().unwrap_or_default(),
        "size": media.size.unwrap_or(0),
    }))
}

/// The names in use in a folder (or at the top of the caller's own files): the files and folders not in the trash.
fn names_in(parent: &str, owner: &str) -> Result<Vec<String>> {
    let mut files = db::find::<Record>("drive_file").filter("folder", parent).filter("trashed", false);
    let mut folders = db::find::<Record>("drive_folder").filter("parent", parent).filter("trashed", false);
    if parent.is_empty() {
        files = files.filter("owner", owner);
        folders = folders.filter("owner", owner);
    }
    let mut names: Vec<String> = files.limit(1000).all()?.iter().filter_map(|f| text(f, "name").map(str::to_string)).collect();
    names.extend(folders.limit(1000).all()?.iter().filter_map(|f| text(f, "name").map(str::to_string)));
    Ok(names)
}

/// The folder a new item goes in: the top (`""`) or a folder the caller may write to.
fn target_folder(folder: Option<&str>) -> Result<String> {
    match folder.map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) => {
            let (rec, _) = require(Kind::Folder, f, Level::Edit)?;
            if flag(&rec, "trashed") {
                return Err(Error::msg("that folder is in the trash"));
            }
            Ok(key(id_text(&rec)).to_string())
        }
        None => Ok(String::new()),
    }
}

#[derive(Deserialize)]
pub struct NewFolder {
    name: String,
    #[serde(default)]
    parent: Option<String>,
}

pub fn create_folder(input: NewFolder) -> Result<Value> {
    let me = me()?;
    let parent = target_folder(input.parent.as_deref())?;
    let name = clean_name("a name", &input.name)?;
    if names_in(&parent, &me)?.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
        return Err(Error::msg(format!("there is already something named {name} here")));
    }
    let folder: Record = db::create("drive_folder", &json!({ "name": name, "parent": parent, "owner": me, "trashed": false, "modified_at": now()? }))?;
    Ok(json!({ "id": key(id_text(&folder)), "name": name }))
}

#[derive(Deserialize)]
pub struct Register {
    #[serde(default)]
    folder: Option<String>,
    media: MediaIn,
    #[serde(default)]
    name: Option<String>,
}

/// A file was uploaded to the media store; make it a file of the drive and put it under the drive's guard.
pub fn register_file(input: Register) -> Result<Value> {
    let me = me()?;
    let folder = target_folder(input.folder.as_deref())?;
    let media = check_media(&input.media)?;
    let wanted = clean_name("a name", input.name.as_deref().or(input.media.name.as_deref()).unwrap_or("file"))?;
    let name = free_name(&wanted, &names_in(&folder, &me)?);
    crate::quota::check(&me, input.media.size.unwrap_or(0) as i64)?;
    aether_sdk::media::protect(&input.media.key)?;
    let at = now()?;
    let file: Record = db::create(
        "drive_file",
        &json!({
            "name": name, "folder": folder, "owner": me, "media": media, "mime": media["mime"], "size": media["size"], "version": 1,
            "trashed": false, "modified_at": at, "modified_by": me,
        }),
    )?;
    db::create::<Record>(
        "drive_blob",
        &json!({
            "file": key(id_text(&file)), "blob_key": input.media.key, "version": 1, "media": media, "name": name,
            "mime": media["mime"], "size": media["size"], "uploaded_by": me, "at": at,
        }),
    )?;
    crate::index::start(key(id_text(&file)), 1, &input.media.key, &name);
    events::emit("file_added", &json!({ "file": key(id_text(&file)), "folder": folder, "owner": me, "name": name }))?;
    Ok(json!({ "id": key(id_text(&file)), "name": name, "version": 1 }))
}

#[derive(Deserialize)]
pub struct NewVersion {
    file: String,
    media: MediaIn,
}

/// A new version of a file: the old one stays, readable and restorable.
pub fn new_version(input: NewVersion) -> Result<Value> {
    let me = me()?;
    let (file, _) = require(Kind::File, &input.file, Level::Edit)?;
    if flag(&file, "trashed") {
        return Err(Error::msg("this file is in the trash"));
    }
    let media = check_media(&input.media)?;
    crate::quota::check(&me, input.media.size.unwrap_or(0) as i64)?;
    aether_sdk::media::protect(&input.media.key)?;
    let version = int(&file, "version") + 1;
    let at = now()?;
    db::create::<Record>(
        "drive_blob",
        &json!({
            "file": key(id_text(&file)), "blob_key": input.media.key, "version": version, "media": media,
            "name": text(&file, "name"), "mime": media["mime"], "size": media["size"], "uploaded_by": me, "at": at,
        }),
    )?;
    db::update::<Record>(
        "drive_file",
        key(id_text(&file)),
        &json!({ "media": media, "mime": media["mime"], "size": media["size"], "version": version, "modified_at": at, "modified_by": me }),
    )?;
    crate::index::start(key(id_text(&file)), version, &input.media.key, text(&file, "name").unwrap_or_default());
    Ok(json!({ "id": key(id_text(&file)), "version": version }))
}

#[derive(Deserialize)]
pub struct Rename {
    kind: String,
    id: String,
    name: String,
}

pub fn rename(input: Rename) -> Result<Value> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Edit)?;
    let name = clean_name("a name", &input.name)?;
    let parent = parent_of(kind, &item);
    let owner = text(&item, "owner").unwrap_or_default().to_string();
    let same_as_now = text(&item, "name").is_some_and(|n| n == name);
    if !same_as_now && names_in(&parent, &owner)?.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
        return Err(Error::msg(format!("there is already something named {name} here")));
    }
    db::update::<Record>(kind.model(), key(id_text(&item)), &json!({ "name": name, "modified_at": now()? }))?;
    Ok(json!({ "name": name }))
}

#[derive(Deserialize)]
pub struct Move {
    kind: String,
    id: String,
    #[serde(default)]
    to: Option<String>,
}

pub fn move_item(input: Move) -> Result<Value> {
    let me = me()?;
    let kind = Kind::parse(&input.kind)?;
    let (item, level) = require(kind, &input.id, Level::Edit)?;
    let target = input.to.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if target.is_none() && level != Level::Owner {
        return Err(Error::msg("only the owner can move this to the top of their files"));
    }
    let dest = target_folder(target)?;
    if kind == Kind::Folder {
        // A folder cannot go into itself or anything inside it.
        let mut cursor = dest.clone();
        for _ in 0..40 {
            if cursor.is_empty() {
                break;
            }
            if cursor == key(id_text(&item)) {
                return Err(Error::msg("a folder cannot be moved into itself"));
            }
            cursor = match db::get::<Record>("drive_folder", &cursor)? {
                Some(f) => parent_of(Kind::Folder, &f),
                None => break,
            };
        }
    }
    let owner = text(&item, "owner").unwrap_or(&me).to_string();
    let current = text(&item, "name").unwrap_or_default().to_string();
    let name = free_name(&current, &names_in(&dest, &owner)?.into_iter().filter(|n| !n.eq_ignore_ascii_case(&current) || parent_of(kind, &item) != dest).collect::<Vec<_>>());
    db::update::<Record>(kind.model(), key(id_text(&item)), &json!({ kind.parent_field(): dest, "name": name, "modified_at": now()? }))?;
    Ok(json!({ "name": name, "folder": dest }))
}

#[derive(Deserialize)]
pub struct Target {
    kind: String,
    id: String,
}

pub fn trash(input: Target) -> Result<Value> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Edit)?;
    db::update::<Record>(kind.model(), key(id_text(&item)), &json!({ "trashed": true, "trashed_at": now()?, "modified_at": now()? }))?;
    Ok(json!({ "trashed": true }))
}

pub fn restore(input: Target) -> Result<Value> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    // Back where it was, unless that folder is gone or in the trash too: then at the top.
    let parent = parent_of(kind, &item);
    let mut data = json!({ "trashed": false, "modified_at": now()? });
    if !parent.is_empty() {
        let usable = db::get::<Record>("drive_folder", &parent)?.is_some_and(|f| !in_trash(Kind::Folder, &f).unwrap_or(true));
        if !usable {
            data[kind.parent_field()] = json!("");
        }
    }
    let me = me()?;
    let dest = data.get(kind.parent_field()).and_then(Value::as_str).map(str::to_string).unwrap_or(parent);
    let taken = names_in(&dest, &me)?;
    let name = free_name(text(&item, "name").unwrap_or_default(), &taken);
    data["name"] = json!(name);
    db::update::<Record>(kind.model(), key(id_text(&item)), &data)?;
    Ok(json!({ "restored": true, "name": name }))
}

/// Delete a file for good: its versions and its shares. The bytes stay guarded, so nobody can read them, until the
/// kernel's cleanup removes the stored objects.
fn purge_file(file: &Record) -> Result<()> {
    let id = key(id_text(file));
    for blob in db::find::<Record>("drive_blob").filter("file", id).limit(1000).all()? {
        db::delete::<Record>("drive_blob", key(id_text(&blob)))?;
    }
    for share in db::find::<Record>("drive_share").filter("item", id).limit(500).all()? {
        db::delete::<Record>("drive_share", key(id_text(&share)))?;
    }
    crate::social::drop_comments(id)?;
    crate::social::drop_favourites(id)?;
    crate::links::drop_for(id)?;
    db::delete::<Record>("drive_file", id)?;
    Ok(())
}

pub fn purge(input: Target) -> Result<Value> {
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::Owner)?;
    let mut removed = 0usize;
    match kind {
        Kind::File => {
            purge_file(&item)?;
            removed += 1;
        }
        Kind::Folder => {
            let mut folders = vec![item.clone()];
            let mut at = 0;
            while at < folders.len() && folders.len() < MAX_PURGE {
                let id = key(id_text(&folders[at])).to_string();
                let children: Vec<Record> = db::find("drive_folder").filter("parent", id.as_str()).limit(1000).all()?;
                folders.extend(children);
                at += 1;
            }
            for folder in &folders {
                let id = key(id_text(folder));
                for file in db::find::<Record>("drive_file").filter("folder", id).limit(1000).all()? {
                    purge_file(&file)?;
                    removed += 1;
                }
                for share in db::find::<Record>("drive_share").filter("item", id).limit(500).all()? {
                    db::delete::<Record>("drive_share", key(id_text(&share)))?;
                }
                crate::social::drop_favourites(id)?;
                crate::links::drop_for(id)?;
                db::delete::<Record>("drive_folder", id)?;
                removed += 1;
            }
        }
    }
    Ok(json!({ "removed": removed }))
}

/// Delete everything the caller has in the trash.
pub fn empty_trash() -> Result<Value> {
    let me = me()?;
    let mut removed = 0;
    for file in db::find::<Record>("drive_file").filter("owner", me.as_str()).filter("trashed", true).limit(1000).all()? {
        removed += purge(Target { kind: "file".into(), id: id_text(&file).into() })?["removed"].as_u64().unwrap_or(0);
    }
    for folder in db::find::<Record>("drive_folder").filter("owner", me.as_str()).filter("trashed", true).limit(1000).all()? {
        // A folder purged with its parent is already gone.
        if db::get::<Record>("drive_folder", key(id_text(&folder)))?.is_some() {
            removed += purge(Target { kind: "folder".into(), id: id_text(&folder).into() })?["removed"].as_u64().unwrap_or(0);
        }
    }
    Ok(json!({ "removed": removed }))
}

// ---- versions -------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct FileOnly {
    pub file: String,
}

pub fn versions(input: FileOnly) -> Result<Vec<Value>> {
    let (file, _) = require(Kind::File, &input.file, Level::View)?;
    let people = org_people()?;
    let blobs: Vec<Record> = db::find("drive_blob").filter("file", key(id_text(&file))).order_by("-version").limit(200).all()?;
    Ok(blobs
        .iter()
        .map(|b| {
            json!({
                "id": key(id_text(b)), "version": int(b, "version"), "size": int(b, "size"), "mime": text(b, "mime"), "name": text(b, "name"),
                "uploaded_by": text(b, "uploaded_by").map(|u| name_of(&people, u)), "at": text(b, "at"), "media": b.get("media"),
                "current": text(b, "blob_key") == file.get("media").and_then(|m| m.get("key")).and_then(Value::as_str),
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct RestoreVersion {
    file: String,
    version: i64,
}

/// Make an earlier version the current one again. It becomes a new version, so nothing is lost either way.
pub fn restore_version(input: RestoreVersion) -> Result<Value> {
    let me = me()?;
    let (file, _) = require(Kind::File, &input.file, Level::Edit)?;
    let blob: Record = db::find("drive_blob")
        .filter("file", key(id_text(&file)))
        .filter("version", input.version)
        .first()?
        .ok_or_else(|| Error::msg("there is no such version"))?;
    let version = int(&file, "version") + 1;
    let at = now()?;
    let media = blob.get("media").cloned().unwrap_or(Value::Null);
    db::create::<Record>(
        "drive_blob",
        &json!({
            "file": key(id_text(&file)), "blob_key": text(&blob, "blob_key"), "version": version, "media": media,
            "name": text(&file, "name"), "mime": text(&blob, "mime"), "size": int(&blob, "size"), "uploaded_by": me, "at": at,
        }),
    )?;
    db::update::<Record>(
        "drive_file",
        key(id_text(&file)),
        &json!({ "media": media, "mime": text(&blob, "mime"), "size": int(&blob, "size"), "version": version, "modified_at": at, "modified_by": me }),
    )?;
    if let Some(blob_key) = text(&blob, "blob_key") {
        crate::index::start(key(id_text(&file)), version, blob_key, text(&file, "name").unwrap_or_default());
    }
    Ok(json!({ "version": version }))
}

#[derive(Deserialize)]
pub struct KeyIn {
    key: String,
}

/// The kernel asks this, as the person who wants to read a stored file: may they?
pub fn can_read_media(input: KeyIn) -> Result<Value> {
    let me = me()?;
    let blobs: Vec<Record> = db::find("drive_blob").filter("blob_key", input.key.as_str()).limit(10).all()?;
    for blob in blobs {
        let Some(file) = db::get::<Record>("drive_file", text(&blob, "file").map(key).unwrap_or_default())? else { continue };
        let Some(level) = level_of(Kind::File, &file, &me)? else {
            // No share of its own: a link that works (the key reached the person through it) opens it for viewing.
            if !in_trash(Kind::File, &file)? && crate::links::covers_file(&file)? {
                return Ok(json!({ "allow": true }));
            }
            continue;
        };
        // A file in the trash can still be read by its owner (to look before deleting for good), not by others.
        if !in_trash(Kind::File, &file)? || level == Level::Owner {
            return Ok(json!({ "allow": true }));
        }
    }
    Ok(json!({ "allow": false }))
}

#[allow(dead_code)]
fn unused(_: Filter) {}
