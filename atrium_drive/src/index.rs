//! Search through the text inside files: the kernel reads a stored file (a PDF, an Office file, a text file, a scan) and
//! the text is kept on the file's row, so a search finds a word that is only in the file.

use aether_sdk::prelude::*;

use crate::common::*;
use crate::settings;

/// The most text kept for one file.
const MAX_CHARS: usize = 60_000;

/// Ask the kernel to read the stored file; `index_done` is called when it has. Reading is best effort: a kind of file
/// it cannot read (a zip, a recording) is simply found by its name only.
pub fn start(file: &str, version: i64, media_key: &str, name: &str) {
    if !settings::current().map(|s| s.search_contents).unwrap_or(false) {
        return;
    }
    let _ = ocr::submit(&json!({ "media_key": media_key, "name": name, "on_done": "index_done", "ref": format!("{file}|{version}") }));
}

/// The text to keep: trimmed, at most [`MAX_CHARS`] characters.
pub fn clip(text: &str) -> String {
    text.trim().chars().take(MAX_CHARS).collect()
}

/// The kernel's call back when a file has been read: `{ event: "ocr.done", cache_key, status, ref }`.
pub fn index_done(event: Value) -> Result<Value> {
    let Some((file, version)) = event["ref"].as_str().and_then(|r| r.split_once('|')) else { return Ok(json!({ "ignored": true })) };
    let version: i64 = version.parse().unwrap_or(0);
    if event["status"] != "done" {
        return Ok(json!({ "indexed": false }));
    }
    let Some(row) = db::get::<Record>("drive_file", file)? else { return Ok(json!({ "indexed": false })) };
    if version < int(&row, "content_version") {
        return Ok(json!({ "indexed": false }));
    }
    let stored = ocr::result(&json!({ "cache_key": event["cache_key"] }))?;
    let body = clip(stored["result"]["text"].as_str().unwrap_or_default());
    db::update::<Record>("drive_file", file, &json!({ "content": body, "content_version": version }))?;
    Ok(json!({ "indexed": true, "characters": body.chars().count() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_trimmed_and_capped() {
        assert_eq!(clip("  hello \n"), "hello");
        assert_eq!(clip(&"x".repeat(MAX_CHARS + 10)).chars().count(), MAX_CHARS);
    }
}
