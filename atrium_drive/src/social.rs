//! Comments on files and favourites.

use std::collections::HashSet;

use aether_sdk::prelude::*;

use crate::access::{require, Kind, Level};
use crate::common::*;
use crate::settings;

const MAX_COMMENT: usize = 4000;

#[derive(Deserialize)]
pub struct FileOnly {
    file: String,
}

pub fn file_comments(input: FileOnly) -> Result<Vec<Value>> {
    let me = me()?;
    let (file, level) = require(Kind::File, &input.file, Level::View)?;
    let people = org_people()?;
    let comments: Vec<Record> = db::find("drive_comment").filter("file", key(id_text(&file))).order_by("at").limit(500).all()?;
    Ok(comments
        .iter()
        .map(|c| {
            let author = text(c, "author").unwrap_or_default();
            json!({
                "id": key(id_text(c)), "author": author, "author_name": name_of(&people, author), "body": text(c, "body"), "at": text(c, "at"),
                "can_delete": key(author) == key(&me) || level == Level::Owner,
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct NewComment {
    file: String,
    body: String,
}

pub fn add_comment(input: NewComment) -> Result<Value> {
    let me = me()?;
    if !settings::current()?.comments_enabled {
        return Err(Error::msg("comments are switched off for this organization"));
    }
    let (file, _) = require(Kind::File, &input.file, Level::View)?;
    let body = input.body.trim();
    if body.is_empty() {
        return Err(Error::msg("write something first"));
    }
    if body.chars().count() > MAX_COMMENT {
        return Err(Error::msg(format!("a comment is at most {MAX_COMMENT} characters")));
    }
    let comment: Record = db::create("drive_comment", &json!({ "file": key(id_text(&file)), "author": me, "body": body, "at": now()? }))?;
    let owner = text(&file, "owner").unwrap_or_default();
    if !owner.is_empty() && key(owner) != key(&me) {
        let people = org_people()?;
        Notification::new(format!("{} commented on {}", name_of(&people, &me), text(&file, "name").unwrap_or("a file")))
            .body(body.chars().take(140).collect::<String>())
            .link("/drive")
            .to(Audience::Actors(vec![owner.to_string()]))
            .send()?;
    }
    Ok(json!({ "id": key(id_text(&comment)) }))
}

#[derive(Deserialize)]
pub struct CommentOnly {
    id: String,
}

pub fn delete_comment(input: CommentOnly) -> Result<Value> {
    let me = me()?;
    let comment = db::get::<Record>("drive_comment", key(&input.id))?.ok_or_else(|| Error::msg("there is no such comment"))?;
    let (_, level) = require(Kind::File, text(&comment, "file").unwrap_or_default(), Level::View)?;
    if text(&comment, "author").is_some_and(|a| key(a) != key(&me)) && level != Level::Owner {
        return Err(Error::msg("only the author or the file's owner can delete a comment"));
    }
    db::delete::<Record>("drive_comment", key(id_text(&comment)))?;
    Ok(json!({ "deleted": true }))
}

/// Delete the comments of a file that is being deleted for good.
pub fn drop_comments(file: &str) -> Result<()> {
    for c in db::find::<Record>("drive_comment").filter("file", file).limit(1000).all()? {
        db::delete::<Record>("drive_comment", key(id_text(&c)))?;
    }
    Ok(())
}

// ---- favourites -----------------------------------------------------------------------------------------------

/// The keys of what the caller has starred.
pub fn favourite_ids(me: &str) -> Result<HashSet<String>> {
    Ok(db::find::<Record>("drive_favourite")
        .filter("user", me)
        .limit(5000)
        .all()?
        .iter()
        .filter_map(|f| text(f, "item").map(|i| key(i).to_string()))
        .collect())
}

#[derive(Deserialize)]
pub struct Star {
    kind: String,
    id: String,
    /// Star it (`true`), unstar it (`false`), or flip it when not given.
    #[serde(default)]
    on: Option<bool>,
}

pub fn toggle_favourite(input: Star) -> Result<Value> {
    let me = me()?;
    let kind = Kind::parse(&input.kind)?;
    let (item, _) = require(kind, &input.id, Level::View)?;
    let item_key = key(id_text(&item)).to_string();
    let existing: Vec<Record> = db::find("drive_favourite").filter("user", me.as_str()).filter("item", item_key.as_str()).limit(10).all()?;
    let want = input.on.unwrap_or(existing.is_empty());
    if want && existing.is_empty() {
        db::create::<Record>("drive_favourite", &json!({ "user": me, "item": item_key, "kind": kind.name() }))?;
    } else if !want {
        for f in &existing {
            db::delete::<Record>("drive_favourite", key(id_text(f)))?;
        }
    }
    Ok(json!({ "favourite": want }))
}

/// Remove everyone's stars on an item that is being deleted for good.
pub fn drop_favourites(item: &str) -> Result<()> {
    for f in db::find::<Record>("drive_favourite").filter("item", item).limit(2000).all()? {
        db::delete::<Record>("drive_favourite", key(id_text(&f)))?;
    }
    Ok(())
}
