//! Who may do what with a file or folder: the owner, anyone it is shared with, and, through a folder, everything
//! inside it.

use aether_sdk::prelude::*;

use crate::common::*;

const MAX_DEPTH: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    View,
    Edit,
    Owner,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::View => "view",
            Level::Edit => "edit",
            Level::Owner => "owner",
        }
    }
    pub fn parse(text: &str) -> Option<Level> {
        match text {
            "view" => Some(Level::View),
            "edit" => Some(Level::Edit),
            _ => None,
        }
    }
    pub fn can_write(self) -> bool {
        self >= Level::Edit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    File,
}

impl Kind {
    pub fn parse(text: &str) -> Result<Kind> {
        match text {
            "folder" => Ok(Kind::Folder),
            "file" => Ok(Kind::File),
            _ => Err(Error::msg("kind is `folder` or `file`")),
        }
    }
    pub fn model(self) -> &'static str {
        match self {
            Kind::Folder => "drive_folder",
            Kind::File => "drive_file",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Kind::Folder => "folder",
            Kind::File => "file",
        }
    }
    /// The field holding the key of the folder this is in.
    pub fn parent_field(self) -> &'static str {
        match self {
            Kind::Folder => "parent",
            Kind::File => "folder",
        }
    }
}

pub fn parent_of(kind: Kind, item: &Record) -> String {
    text(item, kind.parent_field()).map(|p| key(p).to_string()).unwrap_or_default()
}

pub fn shares_for(item: &str) -> Result<Vec<Record>> {
    db::find("drive_share").filter("item", key(item)).limit(500).all()
}

fn matches_person(share: &Record, me: &str) -> bool {
    text(share, "user").is_some_and(|u| u == "org" || key(u) == key(me))
}

/// The best level `me` has on `item`: owner (of it, or of any folder above it: what is in your folder is yours to
/// manage), or the highest share on the item or any folder above it.
pub fn level_of(kind: Kind, item: &Record, me: &str) -> Result<Option<Level>> {
    if text(item, "owner").is_some_and(|o| key(o) == key(me)) {
        return Ok(Some(Level::Owner));
    }
    let mut best: Option<Level> = None;
    let mut current = item.clone();
    let mut here = kind;
    for _ in 0..MAX_DEPTH {
        if text(&current, "owner").is_some_and(|o| key(o) == key(me)) {
            return Ok(Some(Level::Owner));
        }
        for share in shares_for(id_text(&current))? {
            if matches_person(&share, me) {
                best = best.max(text(&share, "level").and_then(Level::parse));
            }
        }
        let parent = parent_of(here, &current);
        if parent.is_empty() {
            break;
        }
        match db::get::<Record>("drive_folder", &parent)? {
            Some(folder) => {
                current = folder;
                here = Kind::Folder;
            }
            None => break,
        }
    }
    Ok(best)
}

/// The record and the caller's level on it, at least `min`. A missing item and one the caller may not see look the
/// same ("not found"), so existence is not given away.
pub fn require(kind: Kind, id: &str, min: Level) -> Result<(Record, Level)> {
    let me = me()?;
    let item = db::get::<Record>(kind.model(), key(id))?.ok_or_else(|| Error::msg(format!("there is no such {}", kind.name())))?;
    match level_of(kind, &item, &me)? {
        Some(level) if level >= min => Ok((item, level)),
        Some(_) => Err(Error::msg(format!("you only have view access to this {}", kind.name()))),
        None => Err(Error::msg(format!("there is no such {}", kind.name()))),
    }
}

/// Whether the item or any folder above it is in the trash.
pub fn in_trash(kind: Kind, item: &Record) -> Result<bool> {
    let mut current = item.clone();
    let mut here = kind;
    for _ in 0..MAX_DEPTH {
        if flag(&current, "trashed") {
            return Ok(true);
        }
        let parent = parent_of(here, &current);
        if parent.is_empty() {
            return Ok(false);
        }
        match db::get::<Record>("drive_folder", &parent)? {
            Some(folder) => {
                current = folder;
                here = Kind::Folder;
            }
            None => return Ok(false),
        }
    }
    Ok(false)
}

/// The folders from the top down to (and including) a folder, as `(key, name)`, stopping above any folder `me` may
/// not see: someone given a deep folder sees it as the start of their path.
pub fn path_of(folder: &Record, me: &str) -> Result<Vec<(String, String)>> {
    let mut chain: Vec<Record> = vec![folder.clone()];
    for _ in 0..MAX_DEPTH {
        let parent = parent_of(Kind::Folder, chain.last().unwrap_or(folder));
        if parent.is_empty() {
            break;
        }
        match db::get::<Record>("drive_folder", &parent)? {
            Some(f) => chain.push(f),
            None => break,
        }
    }
    chain.reverse();
    let mut path = Vec::new();
    for f in &chain {
        if level_of(Kind::Folder, f, me)?.is_some() {
            path.push((key(id_text(f)).to_string(), text(f, "name").unwrap_or_default().to_string()));
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_view_below_edit_below_owner() {
        assert!(Level::View < Level::Edit && Level::Edit < Level::Owner);
        assert!(!Level::View.can_write() && Level::Edit.can_write() && Level::Owner.can_write());
        assert_eq!(Level::parse("edit"), Some(Level::Edit));
        assert_eq!(Level::parse("owner"), None, "ownership is never granted by a share");
    }

    #[test]
    fn a_share_for_everyone_or_the_person_matches() {
        let org = json!({ "user": "org" });
        let ann = json!({ "user": "users:ann" });
        assert!(matches_person(&org, "users:bob"));
        assert!(matches_person(&ann, "ann"));
        assert!(!matches_person(&ann, "users:bob"));
    }

    #[test]
    fn a_file_is_in_a_folder_and_a_folder_in_a_parent() {
        assert_eq!(parent_of(Kind::File, &json!({ "folder": "mdl_x:abc" })), "abc");
        assert_eq!(parent_of(Kind::Folder, &json!({ "parent": "" })), "");
        assert!(Kind::parse("file").is_ok() && Kind::parse("share").is_err());
    }
}
