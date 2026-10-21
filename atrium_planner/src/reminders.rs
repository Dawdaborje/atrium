//! A morning nudge for what is due.

use aether_sdk::{db::Filter, prelude::*};

use crate::common::*;
use crate::plans::notify;

/// Tasks due today, and tasks that fell due yesterday, tell the people they are assigned to.
pub fn due_reminders() -> Result<Value> {
    let today = today()?;
    let yesterday = aether_sdk::dates::format_date(aether_sdk::dates::parse_date(&today)? - aether_sdk::dates::Duration::days(1));
    let mut sent = 0;
    for (day, kind) in [(&today, "is due today"), (&yesterday, "is now overdue")] {
        let tasks: Vec<Record> = db::find("planner_task").matching(Filter::eq("due", day.as_str()).and(Filter::ne("progress", "completed"))).limit(5000).all()?;
        for task in &tasks {
            for user in strings(task.get("assignees")) {
                notify(&user, &format!("“{}” {kind}", text(task, "title").unwrap_or("A task")), &format!("/planner/task/{}", key(id_text(task))))?;
                sent += 1;
            }
        }
    }
    Ok(json!({ "sent": sent }))
}
