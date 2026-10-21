//! Editing an office file in the browser, in an editor server the administrator set up (the kernel's `collabora` bridge).
//!
//! The editor reads the file from the kernel and sends every save back to [`crate::items::new_version`], **as the person
//! editing**, so the drive's own rules apply to a save as to an upload: they must still be allowed to edit, the quota counts,
//! and each save is a version that can be restored.

use aether_sdk::bridge;
use aether_sdk::prelude::*;

use crate::access::{require, Kind, Level};
use crate::common::*;

const EDITABLE: &[&str] = &["doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "odp", "rtf", "csv"];

pub fn is_editable(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && EDITABLE.contains(&ext.to_lowercase().as_str()))
}

#[derive(Deserialize)]
pub struct Open {
    file: String,
}

/// The address of the editor for this file and this person. Someone who may only view it gets the editor without saving.
pub fn open_in_editor(input: Open) -> Result<Value> {
    let me = me()?;
    let (file, level) = require(Kind::File, &input.file, Level::View)?;
    if flag(&file, "trashed") {
        return Err(Error::msg("this file is in the trash"));
    }
    let name = text(&file, "name").unwrap_or_default();
    if !is_editable(name) {
        return Err(Error::msg("this kind of file cannot be edited online"));
    }
    let media = file.get("media").and_then(|m| m.get("key")).and_then(Value::as_str).ok_or_else(|| Error::msg("the file has no content"))?;
    let people = org_people()?;
    let link: Value = bridge::call(
        "collabora",
        "editor_url",
        &json!({
            "media": media, "name": name, "user": me, "user_name": name_of(&people, &me), "can_write": level.can_write(),
            "plugin": "atrium_drive", "save": "new_version", "ref": key(id_text(&file)),
        }),
    )?;
    Ok(json!({ "open_url": link["url"], "can_write": level.can_write() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_files_are_editable_and_nothing_else() {
        for yes in ["Plan.docx", "A.XLSX", "slides.odp", "data.csv", "my.report.doc"] {
            assert!(is_editable(yes), "{yes}");
        }
        for no in ["a.pdf", "x.docx.exe", ".docx", "docx", "pic.png", ""] {
            assert!(!is_editable(no), "{no}");
        }
    }
}
