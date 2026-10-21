//! How much each person stores, and the organization's limit.

use std::collections::HashSet;

use aether_sdk::prelude::*;

use crate::common::*;
use crate::settings;

/// Bytes `user` has stored: every stored object they uploaded, counted once however many versions point at it.
pub fn used_bytes(user: &str) -> Result<i64> {
    let blobs: Vec<Record> = db::find("drive_blob").filter("uploaded_by", user).limit(20000).all()?;
    let mut seen = HashSet::new();
    Ok(blobs.iter().filter(|b| seen.insert(text(b, "blob_key").unwrap_or_default().to_string())).map(|b| int(b, "size")).sum())
}

/// Refuse an upload that would take `user` over the quota.
pub fn check(user: &str, adding: i64) -> Result<()> {
    let limit = settings::quota_bytes(&settings::current()?);
    if limit <= 0 {
        return Ok(());
    }
    let used = used_bytes(user)?;
    if used.saturating_add(adding) > limit {
        return Err(Error::msg(format!(
            "your drive is full: {} of {} used, and this file needs {}",
            human(used),
            human(limit),
            human(adding)
        )));
    }
    Ok(())
}

pub fn human(bytes: i64) -> String {
    let b = bytes.max(0) as f64;
    if b < 1024.0 {
        format!("{bytes} B")
    } else if b < 1024.0 * 1024.0 {
        format!("{:.1} KB", b / 1024.0)
    } else if b < 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} MB", b / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", b / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_written_for_people() {
        assert_eq!(human(10), "10 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(human(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human(3 * 1024 * 1024 * 1024), "3.0 GB");
    }
}
