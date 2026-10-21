//! What the pages show: the plans, a board, a person's tasks, a task for the form.

use aether_sdk::{db::Filter, prelude::*};

use crate::access::{members_of, require, role_of, task_for, Role};
use crate::common::*;
use crate::plans::{buckets_of, labels_of, PlanOnly};
use crate::tasks::TaskOnly;

/// "Due Oct 31", "Overdue since Oct 28", or nothing; a finished task is never late.
pub fn due_text(due: Option<&str>, today: &str, completed: bool) -> (String, bool) {
    let Some(due) = due.filter(|d| !d.is_empty()) else { return (String::new(), false) };
    let short = aether_sdk::dates::parse_date(due).map(|d| d.format("%b %-d").to_string()).unwrap_or_else(|_| due.to_string());
    if completed {
        return (format!("Due {short}"), false);
    }
    if due < today {
        (format!("Overdue since {short}"), true)
    } else if due == today {
        ("Due today".to_string(), false)
    } else {
        (format!("Due {short}"), false)
    }
}

fn check_text(task: &Record) -> String {
    match int(task, "check_total") {
        0 => String::new(),
        total => format!("{}/{total}", int(task, "check_done")),
    }
}

fn task_row(task: &Record, today: &str) -> Value {
    let completed = text(task, "progress") == Some("completed");
    let (due_label, late) = due_text(text(task, "due"), today, completed);
    json!({
        "id": key(id_text(task)), "plan": text(task, "plan"), "title": text(task, "title"), "bucket": text(task, "bucket"),
        "priority": text(task, "priority"), "progress": text(task, "progress"), "due": text(task, "due"), "due_text": due_label, "late": late,
        "assignee_names": text(task, "assignee_names"), "check_text": check_text(task), "labels_text": text(task, "labels_text"),
        "position": int(task, "position"), "recurrence": text(task, "recurrence"),
    })
}

/// The plans the caller is in, with how much is open in each.
pub fn my_plans() -> Result<Vec<Value>> {
    let me = me()?;
    let today = today()?;
    let memberships: Vec<Record> = db::find("planner_member").limit(2000).all()?;
    let mut rows = Vec::new();
    for m in memberships.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(&me))) {
        let plan_key = text(m, "plan").unwrap_or_default().to_string();
        let Some(plan) = db::get::<Record>("planner_plan", &plan_key)? else { continue };
        if flag(&plan, "archived") {
            continue;
        }
        let open = db::count("planner_task", Filter::eq("plan", plan_key.as_str()).and(Filter::ne("progress", "completed")))?;
        let done = db::count("planner_task", Filter::eq("plan", plan_key.as_str()).and(Filter::eq("progress", "completed")))?;
        let late = db::count("planner_task", Filter::eq("plan", plan_key.as_str()).and(Filter::ne("progress", "completed")).and(Filter::lt("due", today.as_str())))?;
        rows.push(json!({
            "id": plan_key, "name": text(&plan, "name"), "description": text(&plan, "description"), "color": text(&plan, "color"),
            "role": text(m, "role"), "open": open, "done": done, "overdue": late, "members": members_of(&plan_key)?.len(),
        }));
    }
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

pub fn plan_header(input: PlanOnly) -> Result<Value> {
    let (plan, role, _) = require(&input.plan, Role::Member)?;
    let open = db::count("planner_task", Filter::eq("plan", key(id_text(&plan))).and(Filter::ne("progress", "completed")))?;
    Ok(json!({
        "title": text(&plan, "name"),
        "subtitle": format!("{open} open · {} people{}", members_of(id_text(&plan))?.len(), if role == Role::Owner { " · you own it" } else { "" }),
    }))
}

/// The plan as the settings form edits it.
pub fn plan_info(input: PlanOnly) -> Result<Value> {
    let (plan, role, _) = require(&input.plan, Role::Member)?;
    Ok(json!({
        "id": key(id_text(&plan)), "name": text(&plan, "name"), "description": text(&plan, "description"), "color": text(&plan, "color"),
        "archived": flag(&plan, "archived"), "role": if role == Role::Owner { "owner" } else { "member" },
    }))
}

pub fn board_buckets(input: PlanOnly) -> Result<Vec<Value>> {
    let (plan, _, _) = require(&input.plan, Role::Member)?;
    Ok(buckets_of(id_text(&plan))?.iter().map(|b| json!({ "id": key(id_text(b)), "label": text(b, "name") })).collect())
}

pub fn board_tasks(input: PlanOnly) -> Result<Vec<Value>> {
    let (plan, _, _) = require(&input.plan, Role::Member)?;
    let today = today()?;
    let tasks: Vec<Record> = db::find("planner_task").filter("plan", key(id_text(&plan))).order_by("position").limit(5000).all()?;
    // A task is blocked while one of the tasks it waits for is not completed (all of them are in the plan).
    let open: std::collections::HashSet<String> = tasks.iter().filter(|t| text(t, "progress") != Some("completed")).map(|t| key(id_text(t)).to_string()).collect();
    Ok(tasks
        .iter()
        .map(|t| {
            let mut row = task_row(t, &today);
            let blocked = strings(t.get("blocked_by")).iter().any(|b| open.contains(key(b))) && text(t, "progress") != Some("completed");
            row["blocked_text"] = json!(if blocked { "Blocked" } else { "" });
            row
        })
        .collect())
}

/// A task as the form edits it.
pub fn get_task(input: TaskOnly) -> Result<Value> {
    let (task, plan, _, _) = task_for(&input.id)?;
    Ok(json!({
        "id": key(id_text(&task)), "plan": key(id_text(&plan)), "plan_name": text(&plan, "name"), "title": text(&task, "title"),
        "description": text(&task, "description"), "bucket": text(&task, "bucket"), "assignees": task.get("assignees"),
        "due": text(&task, "due"), "start": text(&task, "start"), "priority": text(&task, "priority"), "progress": text(&task, "progress"),
        "labels": task.get("labels"), "recurrence": text(&task, "recurrence"), "created_by": text(&task, "created_by"),
        "blocked_by": task.get("blocked_by"),
    }))
}

#[derive(Deserialize, Default)]
pub struct Pick {
    /// For the pickers on a task page: the task.
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    plan: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

fn plan_of_pick(input: &Pick) -> Result<Record> {
    if let Some(task) = input.id.as_deref().filter(|t| !t.is_empty()) {
        return Ok(task_for(task)?.1);
    }
    Ok(require(input.plan.as_deref().unwrap_or_default(), Role::Member)?.0)
}

fn matches(needle: &str, name: &str) -> bool {
    needle.is_empty() || name.to_lowercase().contains(needle)
}

pub fn pick_buckets(input: Pick) -> Result<Vec<Value>> {
    let plan = plan_of_pick(&input)?;
    let needle = input.q.clone().unwrap_or_default().trim().to_lowercase();
    Ok(buckets_of(id_text(&plan))?
        .iter()
        .filter(|b| matches(&needle, text(b, "name").unwrap_or_default()))
        .map(|b| json!({ "id": key(id_text(b)), "name": text(b, "name") }))
        .collect())
}

/// The tasks of the same plan this one may wait for.
pub fn pick_tasks(input: Pick) -> Result<Vec<Value>> {
    let plan = plan_of_pick(&input)?;
    let needle = input.q.clone().unwrap_or_default().trim().to_lowercase();
    let own = input.id.as_deref().map(key).unwrap_or_default().to_string();
    let tasks: Vec<Record> = db::find("planner_task").filter("plan", key(id_text(&plan))).order_by("position").limit(2000).all()?;
    Ok(tasks
        .iter()
        .filter(|t| key(id_text(t)) != own && matches(&needle, text(t, "title").unwrap_or_default()))
        .take(200)
        .map(|t| json!({ "id": key(id_text(t)), "name": text(t, "title") }))
        .collect())
}

pub fn pick_people(input: Pick) -> Result<Vec<Value>> {
    let plan = plan_of_pick(&input)?;
    let needle = input.q.clone().unwrap_or_default().trim().to_lowercase();
    let people = org_people()?;
    Ok(members_of(id_text(&plan))?
        .iter()
        .filter_map(|m| text(m, "user"))
        .map(|u| json!({ "id": u, "name": name_of(&people, u) }))
        .filter(|p| matches(&needle, p["name"].as_str().unwrap_or_default()))
        .collect())
}

pub fn pick_labels(input: Pick) -> Result<Vec<Value>> {
    let plan = plan_of_pick(&input)?;
    let needle = input.q.clone().unwrap_or_default().trim().to_lowercase();
    Ok(labels_of(id_text(&plan))?
        .iter()
        .filter(|l| matches(&needle, text(l, "name").unwrap_or_default()))
        .map(|l| json!({ "id": key(id_text(l)), "name": text(l, "name") }))
        .collect())
}

/// Everyone who could be added to a plan: all members of the organization but the caller.
pub fn pick_org_people(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?
        .into_iter()
        .filter(|p| key(&p.id) != key(&me) && matches(&needle, &p.name.to_lowercase()))
        .take(200)
        .map(|p| json!({ "id": p.id, "name": p.name }))
        .collect())
}

#[derive(Deserialize, Default)]
pub struct Mine {
    /// `assigned` (the default) or `created`.
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    query: Option<Value>,
    /// Include finished tasks.
    #[serde(default)]
    done: bool,
}

/// The tasks assigned to the caller (or made by them) across every plan they are in, soonest due first.
pub fn my_tasks(input: Mine) -> Result<Vec<Value>> {
    let me = me()?;
    let today = today()?;
    let scope = input.scope.as_deref().or_else(|| input.query.as_ref().and_then(|q| q.get("scope")).and_then(Value::as_str)).unwrap_or("assigned");
    let base = if scope == "created" { Filter::eq("created_by", me.as_str()) } else { Filter::like("assignee_keys", &format!(",{},", key(&me))) };
    let filter = if input.done { base } else { base.and(Filter::ne("progress", "completed")) };
    let tasks: Vec<Record> = db::find("planner_task").matching(filter).limit(2000).all()?;
    let plans: Vec<Record> = db::find("planner_plan").limit(2000).all()?;
    let mut rows: Vec<Value> = Vec::new();
    for t in &tasks {
        // Only tasks of plans the caller is still in.
        if role_of(text(t, "plan").unwrap_or_default(), &me)?.is_none() {
            continue;
        }
        let mut row = task_row(t, &today);
        row["plan_name"] = json!(plans.iter().find(|p| key(id_text(p)) == text(t, "plan").unwrap_or_default()).and_then(|p| text(p, "name")));
        rows.push(row);
    }
    rows.sort_by(|a, b| {
        let (da, db_) = (a["due"].as_str().unwrap_or("9999-99-99"), b["due"].as_str().unwrap_or("9999-99-99"));
        da.cmp(db_).then_with(|| a["title"].as_str().unwrap_or_default().to_lowercase().cmp(&b["title"].as_str().unwrap_or_default().to_lowercase()))
    });
    Ok(rows)
}

/// For record rules: the keys of the plans the caller is in, so members may read the tasks of their plans.
pub fn rule_my_plans() -> Result<Vec<String>> {
    let me = match context::current()?.actor.id {
        Some(me) => me,
        None => return Ok(Vec::new()),
    };
    let rows: Vec<Record> = db::find("planner_member").limit(5000).all()?;
    Ok(rows.iter().filter(|m| text(m, "user").is_some_and(|u| key(u) == key(&me))).filter_map(|m| text(m, "plan").map(str::to_string)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_dates_read_as_due_today_overdue_or_upcoming() {
        assert_eq!(due_text(Some("2026-10-31"), "2026-10-18", false), ("Due Oct 31".to_string(), false));
        assert_eq!(due_text(Some("2026-10-18"), "2026-10-18", false), ("Due today".to_string(), false));
        assert_eq!(due_text(Some("2026-10-05"), "2026-10-18", false), ("Overdue since Oct 5".to_string(), true));
        assert_eq!(due_text(Some("2026-10-05"), "2026-10-18", true), ("Due Oct 5".to_string(), false));
        assert_eq!(due_text(None, "2026-10-18", false), (String::new(), false));
    }

    #[test]
    fn a_checklist_shows_done_over_total() {
        assert_eq!(check_text(&json!({ "check_total": 5, "check_done": 2 })), "2/5");
        assert_eq!(check_text(&json!({ "check_total": 0 })), "");
    }

    #[test]
    fn pickers_filter_by_a_lowercase_needle() {
        assert!(matches("", "Anything") && matches("ann", "Ann Lee") && !matches("bob", "Ann Lee"));
    }
}
