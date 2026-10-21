//! Tasks: making, changing, moving between buckets, completing (and repeating), checklists.

use aether_sdk::{dates, prelude::*};

use crate::access::{require, task_for, Role};
use crate::common::*;
use crate::plans::{buckets_of, labels_of, notify};

const PRIORITIES: [&str; 4] = ["low", "medium", "high", "urgent"];
const PROGRESS: [&str; 3] = ["not_started", "in_progress", "completed"];
const RECURRENCE: [&str; 5] = ["none", "daily", "weekly", "monthly", "yearly"];
const MAX_CHECKS: i64 = 100;
const MAX_BLOCKERS: usize = 20;

/// The position after the last task of a bucket.
pub fn next_position(bucket: &str) -> Result<i64> {
    let last: Option<Record> = db::find("planner_task").filter("bucket", key(bucket)).order_by("-position").first()?;
    Ok(last.map(|t| int(&t, "position")).unwrap_or(0) + 1)
}

/// When a repeating task is due next: a day, a week, a month or a year after `due`.
pub fn next_due(due: &str, rule: &str) -> Result<String> {
    let date = dates::parse_date(due)?;
    let next = match rule {
        "daily" => date + dates::Duration::days(1),
        "weekly" => date + dates::Duration::days(7),
        "monthly" => dates::add_months(date, 1),
        "yearly" => dates::add_months(date, 12),
        _ => return Err(Error::msg("that task does not repeat")),
    };
    Ok(dates::format_date(next))
}

fn check_choice(label: &str, value: &str, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(Error::msg(format!("{label} is one of: {}", allowed.join(", "))))
    }
}

fn check_date(label: &str, value: &str) -> Result<Option<String>> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(None);
    }
    dates::parse_date(v).map_err(|_| Error::msg(format!("{label} is a date like 2026-10-31")))?;
    Ok(Some(v.to_string()))
}

/// The people (by full id) who may be assigned: only members of the plan.
fn resolve_assignees(plan: &Record, users: &[String]) -> Result<(Vec<String>, String, String)> {
    let people = org_people()?;
    let members = crate::access::members_of(id_text(plan))?;
    let mut ids: Vec<String> = Vec::new();
    for user in users {
        let member = members
            .iter()
            .filter_map(|m| text(m, "user"))
            .find(|m| key(m) == key(user))
            .ok_or_else(|| Error::msg("a task can only be assigned to people in the plan: add them to the plan first"))?;
        if !ids.iter().any(|i| i == member) {
            ids.push(member.to_string());
        }
    }
    if ids.len() > 20 {
        return Err(Error::msg("a task has at most 20 assignees"));
    }
    let names = ids.iter().map(|u| name_of(&people, u)).collect::<Vec<_>>().join(", ");
    let keys = keys_text(&ids.iter().map(|u| key(u).to_string()).collect::<Vec<_>>());
    Ok((ids, names, keys))
}

fn resolve_labels(plan: &Record, wanted: &[String]) -> Result<(Vec<String>, String, String)> {
    let labels = labels_of(id_text(plan))?;
    let mut keys: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for w in wanted {
        let label = labels.iter().find(|l| key(id_text(l)) == key(w)).ok_or_else(|| Error::msg("that label is not in the plan"))?;
        if !keys.iter().any(|k| k == key(id_text(label))) {
            keys.push(key(id_text(label)).to_string());
            names.push(text(label, "name").unwrap_or_default().to_string());
        }
    }
    Ok((keys.clone(), names.join(", "), keys_text(&keys)))
}

/// The tasks that must be completed first, checked: the same plan, not the task itself, and no circle (a task cannot wait,
/// however indirectly, for a task that waits for it).
fn resolve_blockers(task: &Record, plan: &Record, wanted: &[String]) -> Result<Vec<String>> {
    let own = key(id_text(task)).to_string();
    let mut keys: Vec<String> = Vec::new();
    for w in wanted.iter().map(|w| key(w).to_string()).filter(|w| !w.is_empty()) {
        if w == own {
            return Err(Error::msg("a task cannot wait for itself"));
        }
        if keys.contains(&w) {
            continue;
        }
        let other = db::get::<Record>("planner_task", &w)?.filter(|t| text(t, "plan").is_some_and(|p| key(p) == key(id_text(plan)))).ok_or_else(|| Error::msg("a task can only wait for tasks of its own plan"))?;
        if waits_for(&other, &own)? {
            return Err(Error::msg(format!("“{}” already waits for this task: they would wait for each other", text(&other, "title").unwrap_or("that task"))));
        }
        keys.push(w);
    }
    if keys.len() > MAX_BLOCKERS {
        return Err(Error::msg(format!("a task waits for at most {MAX_BLOCKERS} others")));
    }
    Ok(keys)
}

/// Does `start` wait, directly or through others, for the task `target`?
fn waits_for(start: &Record, target: &str) -> Result<bool> {
    let mut seen: Vec<String> = Vec::new();
    let mut queue: Vec<String> = strings(start.get("blocked_by"));
    while let Some(next) = queue.pop() {
        let next = key(&next).to_string();
        if next == target {
            return Ok(true);
        }
        if seen.contains(&next) || seen.len() > 500 {
            continue;
        }
        seen.push(next.clone());
        if let Some(t) = db::get::<Record>("planner_task", &next)? {
            queue.extend(strings(t.get("blocked_by")));
        }
    }
    Ok(false)
}

/// The titles of the tasks this one still waits for.
fn open_blockers(task: &Record) -> Result<Vec<String>> {
    let mut open = Vec::new();
    for k in strings(task.get("blocked_by")) {
        if let Some(t) = db::get::<Record>("planner_task", key(&k))? {
            if text(&t, "progress") != Some("completed") {
                open.push(text(&t, "title").unwrap_or("a task").to_string());
            }
        }
    }
    Ok(open)
}

/// The open tasks that wait for `task`.
fn dependants(task: &Record) -> Result<Vec<Record>> {
    db::find("planner_task").filter("plan", text(task, "plan").map(key).unwrap_or_default()).matching(aether_sdk::db::Filter::like("blocked_keys", &format!(",{},", key(id_text(task))))).limit(500).all()
}

fn bucket_of_plan(plan: &Record, bucket: Option<&str>) -> Result<String> {
    let all = buckets_of(id_text(plan))?;
    match bucket.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => all.iter().find(|x| key(id_text(x)) == key(b)).map(|x| key(id_text(x)).to_string()).ok_or_else(|| Error::msg("that bucket is not in this plan")),
        None => all.first().map(|x| key(id_text(x)).to_string()).ok_or_else(|| Error::msg("the plan has no bucket")),
    }
}

fn announce_assignments(task: &Record, plan: &Record, assigned: &[String], me: &str) -> Result<()> {
    if assigned.is_empty() {
        return Ok(());
    }
    let people = org_people()?;
    let by = name_of(&people, me);
    for user in assigned.iter().filter(|u| key(u) != key(me)) {
        notify(user, &format!("{by} assigned you “{}”", text(task, "title").unwrap_or("a task")), &format!("/planner/task/{}", key(id_text(task))))?;
    }
    events::emit("task_assigned", &json!({ "task": key(id_text(task)), "plan": key(id_text(plan)), "assignees": assigned }))?;
    Ok(())
}

#[derive(Deserialize)]
pub struct NewTask {
    plan: String,
    title: String,
    #[serde(default)]
    bucket: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    assignees: Vec<String>,
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    recurrence: Option<String>,
}

pub fn create_task(input: NewTask) -> Result<Value> {
    let (plan, _, me) = require(&input.plan, Role::Member)?;
    if flag(&plan, "archived") {
        return Err(Error::msg("this plan is archived"));
    }
    let title = clean("a title", &input.title, 200)?;
    let bucket = bucket_of_plan(&plan, input.bucket.as_deref())?;
    let priority = input.priority.as_deref().unwrap_or("medium");
    check_choice("priority", priority, &PRIORITIES)?;
    let recurrence = input.recurrence.as_deref().unwrap_or("none");
    check_choice("repeat", recurrence, &RECURRENCE)?;
    let due = check_date("the due date", input.due.as_deref().unwrap_or(""))?;
    let start = check_date("the start date", input.start.as_deref().unwrap_or(""))?;
    if let (Some(s), Some(d)) = (&start, &due) {
        if s > d {
            return Err(Error::msg("a task cannot start after it is due"));
        }
    }
    let (assignees, names, akeys) = resolve_assignees(&plan, &input.assignees)?;
    let (labels, label_names, lkeys) = resolve_labels(&plan, &input.labels)?;
    let task: Record = db::create(
        "planner_task",
        &json!({
            "plan": key(id_text(&plan)), "bucket": bucket, "title": title, "description": input.description.as_deref().map(str::trim),
            "assignees": assignees, "assignee_names": names, "assignee_keys": akeys, "due": due, "start": start, "priority": priority,
            "progress": "not_started", "position": next_position(&bucket)?, "labels": labels, "labels_text": label_names, "label_keys": lkeys,
            "recurrence": recurrence, "check_total": 0, "check_done": 0, "created_by": me,
        }),
    )?;
    events::emit("task_created", &json!({ "task": key(id_text(&task)), "plan": key(id_text(&plan)), "title": title }))?;
    announce_assignments(&task, &plan, &assignees, &me)?;
    Ok(json!({ "id": key(id_text(&task)) }))
}

#[derive(Deserialize)]
pub struct QuickAdd {
    plan: String,
    title: String,
    /// The column it was added in.
    #[serde(default)]
    bucket: Option<String>,
}

pub fn quick_add(input: QuickAdd) -> Result<Value> {
    create_task(NewTask {
        plan: input.plan,
        title: input.title,
        bucket: input.bucket.filter(|b| b != "Unassigned"),
        description: None,
        assignees: Vec::new(),
        due: None,
        start: None,
        priority: None,
        labels: Vec::new(),
        recurrence: None,
    })
}

#[derive(Deserialize)]
pub struct UpdateTask {
    id: String,
    /// Whatever fields the form sent; only the known ones are used.
    #[serde(flatten)]
    fields: Record,
}

pub fn update_task(input: UpdateTask) -> Result<Value> {
    let (task, plan, _, me) = task_for(&input.id)?;
    let f = &input.fields;
    let mut data = json!({});
    if let Some(t) = f.get("title").and_then(Value::as_str) {
        data["title"] = json!(clean("a title", t, 200)?);
    }
    if let Some(d) = f.get("description") {
        data["description"] = json!(d.as_str().map(str::trim).unwrap_or_default());
    }
    if let Some(b) = f.get("bucket").and_then(Value::as_str) {
        let bucket = bucket_of_plan(&plan, Some(b))?;
        if bucket != text(&task, "bucket").map(key).unwrap_or_default() {
            data["position"] = json!(next_position(&bucket)?);
        }
        data["bucket"] = json!(bucket);
    }
    let mut newly: Vec<String> = Vec::new();
    if let Some(a) = f.get("assignees") {
        let (ids, names, keys) = resolve_assignees(&plan, &strings(Some(a)))?;
        let before = strings(task.get("assignees"));
        newly = ids.iter().filter(|u| !before.iter().any(|b| key(b) == key(u))).cloned().collect();
        data["assignees"] = json!(ids);
        data["assignee_names"] = json!(names);
        data["assignee_keys"] = json!(keys);
    }
    if let Some(l) = f.get("labels") {
        let (keys, names, ktext) = resolve_labels(&plan, &strings(Some(l)))?;
        data["labels"] = json!(keys);
        data["labels_text"] = json!(names);
        data["label_keys"] = json!(ktext);
    }
    let mut due = text(&task, "due").map(str::to_string);
    if let Some(d) = f.get("due") {
        due = check_date("the due date", d.as_str().unwrap_or(""))?;
        data["due"] = json!(due);
    }
    if let Some(s) = f.get("start") {
        let start = check_date("the start date", s.as_str().unwrap_or(""))?;
        if let (Some(st), Some(du)) = (&start, &due) {
            if st > du {
                return Err(Error::msg("a task cannot start after it is due"));
            }
        }
        data["start"] = json!(start);
    }
    if let Some(p) = f.get("priority").and_then(Value::as_str) {
        check_choice("priority", p, &PRIORITIES)?;
        data["priority"] = json!(p);
    }
    if let Some(r) = f.get("recurrence").and_then(Value::as_str) {
        check_choice("repeat", r, &RECURRENCE)?;
        data["recurrence"] = json!(r);
    }
    let mut task_now = task.clone();
    if let Some(b) = f.get("blocked_by") {
        let keys = resolve_blockers(&task, &plan, &strings(Some(b)))?;
        data["blocked_keys"] = json!(keys_text(&keys));
        data["blocked_by"] = json!(keys);
        task_now["blocked_by"] = data["blocked_by"].clone();
    }
    let mut completing = false;
    if let Some(p) = f.get("progress").and_then(Value::as_str) {
        check_choice("progress", p, &PROGRESS)?;
        let was = text(&task, "progress") == Some("completed");
        if p == "completed" && !was {
            let open = open_blockers(&task_now)?;
            if !open.is_empty() {
                return Err(Error::msg(format!("this task waits for: {}", open.join(", "))));
            }
        }
        data["progress"] = json!(p);
        if p == "completed" && !was {
            data["completed_at"] = json!(now()?);
            data["completed_by"] = json!(me);
            completing = true;
        } else if p != "completed" && was {
            data["completed_at"] = Value::Null;
            data["completed_by"] = Value::Null;
        }
    }
    let updated: Record = db::update("planner_task", key(id_text(&task)), &data)?.ok_or_else(|| Error::msg("the task is gone"))?;
    announce_assignments(&updated, &plan, &newly, &me)?;
    if completing {
        after_completion(&updated, &plan, &me)?;
    }
    Ok(json!({ "id": key(id_text(&updated)) }))
}

/// A task was completed: tell others, and make the next one if it repeats.
fn after_completion(task: &Record, plan: &Record, me: &str) -> Result<()> {
    events::emit("task_completed", &json!({ "task": key(id_text(task)), "plan": key(id_text(plan)), "by": me }))?;
    // What waited for this task and now waits for nothing is told to its people.
    for waiting in dependants(task)? {
        if text(&waiting, "progress") != Some("completed") && open_blockers(&waiting)?.is_empty() {
            for user in strings(waiting.get("assignees")).iter().filter(|u| key(u) != key(me)) {
                notify(user, &format!("“{}” can start: nothing blocks it now", text(&waiting, "title").unwrap_or("a task")), &format!("/planner/task/{}", key(id_text(&waiting))))?;
            }
        }
    }
    let rule = text(task, "recurrence").unwrap_or("none");
    if rule == "none" {
        return Ok(());
    }
    let base = text(task, "due").map(str::to_string).map(Ok).unwrap_or_else(today)?;
    let due = next_due(&base, rule)?;
    let start = match (text(task, "start"), text(task, "due")) {
        (Some(s), Some(d)) => {
            let gap = dates::parse_date(d)? - dates::parse_date(s)?;
            Some(dates::format_date(dates::parse_date(&due)? - gap))
        }
        _ => None,
    };
    let next: Record = db::create(
        "planner_task",
        &json!({
            "plan": key(id_text(plan)), "bucket": text(task, "bucket"), "title": text(task, "title"), "description": text(task, "description"),
            "assignees": task.get("assignees"), "assignee_names": text(task, "assignee_names"), "assignee_keys": text(task, "assignee_keys"),
            "due": due, "start": start, "priority": text(task, "priority"), "progress": "not_started",
            "position": next_position(text(task, "bucket").unwrap_or_default())?, "labels": task.get("labels"), "labels_text": text(task, "labels_text"),
            "label_keys": text(task, "label_keys"), "recurrence": rule, "check_total": int(task, "check_total"), "check_done": 0, "created_by": text(task, "created_by"),
        }),
    )?;
    for item in db::find::<Record>("planner_check").filter("task", key(id_text(task))).order_by("position").limit(200).all()? {
        db::create::<Record>("planner_check", &json!({ "task": key(id_text(&next)), "text": text(&item, "text"), "done": false, "position": int(&item, "position") }))?;
    }
    announce_assignments(&next, plan, &strings(next.get("assignees")), me)?;
    Ok(())
}

#[derive(Deserialize)]
pub struct TaskOnly {
    pub id: String,
}

pub fn complete_task(input: TaskOnly) -> Result<Value> {
    update_task(UpdateTask { id: input.id, fields: json!({ "progress": "completed" }) })
}

pub fn reopen_task(input: TaskOnly) -> Result<Value> {
    update_task(UpdateTask { id: input.id, fields: json!({ "progress": "in_progress" }) })
}

#[derive(Deserialize)]
pub struct MoveIn {
    id: String,
    /// The bucket it goes to.
    to: String,
    /// The task it goes in front of; none means the end.
    #[serde(default)]
    before: Option<String>,
}

pub fn move_task(input: MoveIn) -> Result<Value> {
    let (task, plan, _, _) = task_for(&input.id)?;
    let bucket = bucket_of_plan(&plan, Some(&input.to))?;
    let mut order: Vec<Record> = db::find("planner_task").filter("bucket", bucket.as_str()).order_by("position").limit(5000).all()?;
    order.retain(|t| key(id_text(t)) != key(id_text(&task)));
    let at = input.before.as_deref().and_then(|b| order.iter().position(|t| key(id_text(t)) == key(b))).unwrap_or(order.len());
    order.insert(at, task.clone());
    for (i, t) in order.iter().enumerate() {
        let position = i as i64 + 1;
        let moved = key(id_text(t)) == key(id_text(&task));
        if moved || int(t, "position") != position {
            db::update::<Record>("planner_task", key(id_text(t)), &json!({ "bucket": bucket, "position": position }))?;
        }
    }
    Ok(json!({ "ok": true }))
}

pub fn delete_task(input: TaskOnly) -> Result<Value> {
    let (task, _, _, _) = task_for(&input.id)?;
    purge_task(&task)?;
    Ok(json!({ "deleted": true }))
}

/// Remove a task and its checklist.
pub fn purge_task(task: &Record) -> Result<()> {
    // Whatever waited for it waits no more.
    for waiting in dependants(task)? {
        let rest: Vec<String> = strings(waiting.get("blocked_by")).into_iter().filter(|k| key(k) != key(id_text(task))).collect();
        db::update::<Record>("planner_task", key(id_text(&waiting)), &json!({ "blocked_keys": keys_text(&rest), "blocked_by": rest }))?;
    }
    for item in db::find::<Record>("planner_check").filter("task", key(id_text(task))).limit(500).all()? {
        db::delete::<Record>("planner_check", key(id_text(&item)))?;
    }
    db::delete::<Record>("planner_task", key(id_text(task)))?;
    Ok(())
}

/// A person left a plan: take them off its tasks.
pub fn unassign_everywhere(plan: &str, user: &str) -> Result<()> {
    let needle = format!(",{},", key(user));
    for task in db::find::<Record>("planner_task").filter("plan", key(plan)).limit(10000).all()? {
        if text(&task, "assignee_keys").is_some_and(|k| k.contains(&needle)) {
            let rest: Vec<String> = strings(task.get("assignees")).into_iter().filter(|u| key(u) != key(user)).collect();
            let people = org_people()?;
            db::update::<Record>(
                "planner_task",
                key(id_text(&task)),
                &json!({
                    "assignees": rest, "assignee_names": rest.iter().map(|u| name_of(&people, u)).collect::<Vec<_>>().join(", "),
                    "assignee_keys": keys_text(&rest.iter().map(|u| key(u).to_string()).collect::<Vec<_>>()),
                }),
            )?;
        }
    }
    Ok(())
}

/// A label was deleted: take it off the plan's tasks.
pub fn drop_label(plan: &str, label: &str) -> Result<()> {
    let needle = format!(",{label},");
    for task in db::find::<Record>("planner_task").filter("plan", key(plan)).limit(10000).all()? {
        if text(&task, "label_keys").is_some_and(|k| k.contains(&needle)) {
            let rest: Vec<String> = strings(task.get("labels")).into_iter().filter(|l| l != label).collect();
            let all = labels_of(plan)?;
            let names: Vec<String> = rest.iter().filter_map(|k| all.iter().find(|l| key(id_text(l)) == k)).filter_map(|l| text(l, "name").map(str::to_string)).collect();
            db::update::<Record>("planner_task", key(id_text(&task)), &json!({ "labels": rest, "labels_text": names.join(", "), "label_keys": keys_text(&rest) }))?;
        }
    }
    Ok(())
}

// ---- checklist ------------------------------------------------------------------------------------------

pub fn checklist(input: TaskOnly) -> Result<Vec<Value>> {
    let (task, _, _, _) = task_for(&input.id)?;
    let items: Vec<Record> = db::find("planner_check").filter("task", key(id_text(&task))).order_by("position").limit(200).all()?;
    Ok(items.iter().map(|i| json!({ "id": key(id_text(i)), "text": text(i, "text"), "done": flag(i, "done") })).collect())
}

fn recount(task: &str) -> Result<()> {
    let items: Vec<Record> = db::find("planner_check").filter("task", key(task)).limit(200).all()?;
    let done = items.iter().filter(|i| flag(i, "done")).count();
    db::update::<Record>("planner_task", key(task), &json!({ "check_total": items.len(), "check_done": done }))?;
    Ok(())
}

#[derive(Deserialize)]
pub struct AddCheck {
    id: String,
    text: String,
}

pub fn add_check(input: AddCheck) -> Result<Value> {
    let (task, _, _, _) = task_for(&input.id)?;
    if int(&task, "check_total") >= MAX_CHECKS {
        return Err(Error::msg(format!("a checklist has at most {MAX_CHECKS} items")));
    }
    let position = int(&task, "check_total") + 1;
    let item: Record = db::create("planner_check", &json!({ "task": key(id_text(&task)), "text": clean("an item", &input.text, 200)?, "done": false, "position": position }))?;
    recount(id_text(&task))?;
    Ok(json!({ "id": key(id_text(&item)) }))
}

fn item_with_task(id: &str) -> Result<(Record, Record)> {
    let item = db::get::<Record>("planner_check", key(id))?.ok_or_else(|| Error::msg("there is no such item"))?;
    let (task, _, _, _) = task_for(text(&item, "task").unwrap_or_default())?;
    Ok((item, task))
}

#[derive(Deserialize)]
pub struct Toggle {
    id: String,
    done: bool,
}

pub fn toggle_check(input: Toggle) -> Result<Value> {
    let (item, task) = item_with_task(&input.id)?;
    db::update::<Record>("planner_check", key(id_text(&item)), &json!({ "done": input.done }))?;
    recount(id_text(&task))?;
    Ok(json!({ "done": input.done }))
}

pub fn remove_check(input: TaskOnly) -> Result<Value> {
    let (item, task) = item_with_task(&input.id)?;
    db::delete::<Record>("planner_check", key(id_text(&item)))?;
    recount(id_text(&task))?;
    Ok(json!({ "removed": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_repeating_task_is_due_again_after_its_interval() {
        assert_eq!(next_due("2026-10-31", "daily").unwrap(), "2026-11-01");
        assert_eq!(next_due("2026-10-31", "weekly").unwrap(), "2026-11-07");
        assert_eq!(next_due("2026-01-31", "monthly").unwrap(), "2026-02-28");
        assert_eq!(next_due("2024-02-29", "yearly").unwrap(), "2025-02-28");
        assert!(next_due("2026-10-31", "none").is_err());
        assert!(next_due("soon", "daily").is_err());
    }

    #[test]
    fn dates_and_choices_are_checked() {
        assert_eq!(check_date("a date", "").unwrap(), None);
        assert_eq!(check_date("a date", "2026-10-31").unwrap().as_deref(), Some("2026-10-31"));
        assert!(check_date("a date", "31/10/2026").is_err());
        assert!(check_choice("priority", "high", &PRIORITIES).is_ok());
        assert!(check_choice("priority", "huge", &PRIORITIES).is_err());
    }
}
