//! Plans, their people, buckets and labels.

use aether_sdk::{db::Filter, prelude::*};

use crate::access::{member_row, members_of, require, Role};
use crate::common::*;

const MAX_MEMBERS: usize = 200;
const MAX_BUCKETS: usize = 30;
const MAX_LABELS: usize = 50;
const DEFAULT_BUCKETS: [&str; 3] = ["To do", "In progress", "Done"];

fn check_users(users: &[String]) -> Result<Vec<String>> {
    let people = org_people()?;
    let mut out = Vec::new();
    for user in users {
        let full = people.iter().find(|p| key(&p.id) == key(user)).map(|p| p.id.clone()).ok_or_else(|| Error::msg("that person is not a member of this organization"))?;
        if !out.contains(&full) {
            out.push(full);
        }
    }
    Ok(out)
}

fn add_member(plan: &str, user: &str, role: &str) -> Result<()> {
    if member_row(plan, user)?.is_none() {
        db::create::<Record>("planner_member", &json!({ "plan": key(plan), "user": user, "role": role }))?;
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewPlan {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    members: Vec<String>,
}

pub fn create_plan(input: NewPlan) -> Result<Value> {
    let me = me()?;
    let name = clean("a name", &input.name, 120)?;
    let members = check_users(&input.members)?;
    let plan: Record = db::create(
        "planner_plan",
        &json!({ "name": name, "description": input.description.as_deref().map(str::trim), "color": input.color, "owner": me, "archived": false }),
    )?;
    let id = key(id_text(&plan)).to_string();
    add_member(&id, &me, "owner")?;
    for u in members.iter().filter(|u| key(u) != key(&me)) {
        add_member(&id, u, "member")?;
        notify(u, &format!("{} added you to the plan {name}", name_of(&org_people()?, &me)), &format!("/planner/{id}"))?;
    }
    for (i, bucket) in DEFAULT_BUCKETS.iter().enumerate() {
        db::create::<Record>("planner_bucket", &json!({ "plan": id, "name": bucket, "position": i as i64 + 1 }))?;
    }
    Ok(json!({ "id": id }))
}

pub fn notify(user: &str, title: &str, link: &str) -> Result<()> {
    Notification::new(title.to_string()).link(link.to_string()).to(Audience::Actors(vec![user.to_string()])).send()?;
    Ok(())
}

#[derive(Deserialize)]
pub struct PlanOnly {
    pub plan: String,
}

#[derive(Deserialize)]
pub struct EditPlan {
    plan: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    archived: Option<bool>,
}

pub fn edit_plan(input: EditPlan) -> Result<Value> {
    let (plan, _, _) = require(&input.plan, Role::Owner)?;
    let mut data = json!({});
    if let Some(n) = input.name {
        data["name"] = json!(clean("a name", &n, 120)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(c) = input.color {
        data["color"] = json!(c.trim());
    }
    if let Some(a) = input.archived {
        data["archived"] = json!(a);
    }
    db::update::<Record>("planner_plan", key(id_text(&plan)), &data)?;
    Ok(json!({ "ok": true }))
}

/// Delete a plan with its tasks, buckets, labels and people. Only the owner.
pub fn delete_plan(input: PlanOnly) -> Result<Value> {
    let (plan, _, _) = require(&input.plan, Role::Owner)?;
    let id = key(id_text(&plan)).to_string();
    for task in db::find::<Record>("planner_task").filter("plan", id.as_str()).limit(10000).all()? {
        crate::tasks::purge_task(&task)?;
    }
    for model in ["planner_bucket", "planner_label", "planner_member"] {
        for row in db::find::<Record>(model).filter("plan", id.as_str()).limit(1000).all()? {
            db::delete::<Record>(model, key(id_text(&row)))?;
        }
    }
    db::delete::<Record>("planner_plan", &id)?;
    Ok(json!({ "deleted": true }))
}

// ---- people ---------------------------------------------------------------------------------------------

pub fn plan_members(input: PlanOnly) -> Result<Vec<Value>> {
    let (plan, _, _) = require(&input.plan, Role::Member)?;
    let people = org_people()?;
    let mut rows: Vec<Value> = members_of(id_text(&plan))?
        .iter()
        .map(|m| {
            let user = text(m, "user").unwrap_or_default();
            json!({ "id": user, "name": name_of(&people, user), "role": text(m, "role") })
        })
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

#[derive(Deserialize)]
pub struct AddMembers {
    plan: String,
    users: Vec<String>,
}

pub fn add_plan_members(input: AddMembers) -> Result<Value> {
    let (plan, _, me) = require(&input.plan, Role::Owner)?;
    let users = check_users(&input.users)?;
    if members_of(id_text(&plan))?.len() + users.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a plan has at most {MAX_MEMBERS} people")));
    }
    let people = org_people()?;
    let mut added = 0;
    for u in &users {
        if member_row(id_text(&plan), u)?.is_none() {
            add_member(id_text(&plan), u, "member")?;
            added += 1;
            notify(u, &format!("{} added you to the plan {}", name_of(&people, &me), text(&plan, "name").unwrap_or("a plan")), &format!("/planner/{}", key(id_text(&plan))))?;
        }
    }
    Ok(json!({ "added": added }))
}

#[derive(Deserialize)]
pub struct RemoveMember {
    plan: String,
    /// The person; a list row's action sends it as `id`.
    #[serde(alias = "id")]
    user: String,
}

pub fn remove_plan_member(input: RemoveMember) -> Result<Value> {
    let (plan, role, me) = require(&input.plan, Role::Member)?;
    if key(&input.user) != key(&me) && role != Role::Owner {
        return Err(Error::msg("only the owner of the plan can remove people"));
    }
    let row = member_row(id_text(&plan), &input.user)?.ok_or_else(|| Error::msg("that person is not in the plan"))?;
    if text(&row, "role") == Some("owner") {
        let owners = members_of(id_text(&plan))?.iter().filter(|m| text(m, "role") == Some("owner")).count();
        if owners <= 1 {
            return Err(Error::msg("a plan needs an owner: make someone else the owner first"));
        }
    }
    db::delete::<Record>("planner_member", key(id_text(&row)))?;
    // Their tasks are left unassigned: they can no longer see them.
    crate::tasks::unassign_everywhere(id_text(&plan), &input.user)?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize)]
pub struct SetRole {
    plan: String,
    #[serde(alias = "id")]
    user: String,
    role: String,
}

/// Make a member an owner, or an owner an ordinary member (never the last owner).
pub fn set_plan_role(input: SetRole) -> Result<Value> {
    let (plan, _, _) = require(&input.plan, Role::Owner)?;
    if input.role != "owner" && input.role != "member" {
        return Err(Error::msg("the role is `owner` or `member`"));
    }
    let row = member_row(id_text(&plan), &input.user)?.ok_or_else(|| Error::msg("that person is not in the plan"))?;
    if input.role == "member" && text(&row, "role") == Some("owner") {
        let owners = members_of(id_text(&plan))?.iter().filter(|m| text(m, "role") == Some("owner")).count();
        if owners <= 1 {
            return Err(Error::msg("a plan needs an owner"));
        }
    }
    db::update::<Record>("planner_member", key(id_text(&row)), &json!({ "role": input.role }))?;
    Ok(json!({ "ok": true }))
}

// ---- buckets --------------------------------------------------------------------------------------------

pub fn buckets_of(plan: &str) -> Result<Vec<Record>> {
    db::find("planner_bucket").filter("plan", key(plan)).order_by("position").limit(100).all()
}

#[derive(Deserialize)]
pub struct NewBucket {
    plan: String,
    name: String,
}

pub fn add_bucket(input: NewBucket) -> Result<Value> {
    let (plan, _, _) = require(&input.plan, Role::Member)?;
    let name = clean("a name", &input.name, 80)?;
    let existing = buckets_of(id_text(&plan))?;
    if existing.len() >= MAX_BUCKETS {
        return Err(Error::msg(format!("a plan has at most {MAX_BUCKETS} buckets")));
    }
    if existing.iter().any(|b| text(b, "name").is_some_and(|n| n.eq_ignore_ascii_case(&name))) {
        return Err(Error::msg(format!("there is already a bucket named {name}")));
    }
    let position = existing.iter().map(|b| int(b, "position")).max().unwrap_or(0) + 1;
    let bucket: Record = db::create("planner_bucket", &json!({ "plan": key(id_text(&plan)), "name": name, "position": position }))?;
    Ok(json!({ "id": key(id_text(&bucket)) }))
}

fn bucket_in_plan(bucket: &str) -> Result<(Record, Record)> {
    let b = db::get::<Record>("planner_bucket", key(bucket))?.ok_or_else(|| Error::msg("there is no such bucket"))?;
    let (plan, _, _) = require(text(&b, "plan").unwrap_or_default(), Role::Member)?;
    Ok((b, plan))
}

#[derive(Deserialize)]
pub struct RenameBucket {
    #[serde(alias = "id")]
    bucket: String,
    name: String,
}

pub fn rename_bucket(input: RenameBucket) -> Result<Value> {
    let (b, _) = bucket_in_plan(&input.bucket)?;
    db::update::<Record>("planner_bucket", key(id_text(&b)), &json!({ "name": clean("a name", &input.name, 80)? }))?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct DeleteBucket {
    #[serde(alias = "id")]
    bucket: String,
}

/// Delete a bucket; its tasks move to the first bucket that is left. A plan keeps at least one.
pub fn delete_bucket(input: DeleteBucket) -> Result<Value> {
    let (b, plan) = bucket_in_plan(&input.bucket)?;
    let all = buckets_of(id_text(&plan))?;
    let Some(target) = all.iter().find(|x| key(id_text(x)) != key(id_text(&b))) else {
        return Err(Error::msg("a plan needs at least one bucket"));
    };
    let target_key = key(id_text(target)).to_string();
    let mut next = crate::tasks::next_position(&target_key)?;
    for task in db::find::<Record>("planner_task").filter("bucket", key(id_text(&b))).order_by("position").limit(5000).all()? {
        db::update::<Record>("planner_task", key(id_text(&task)), &json!({ "bucket": target_key, "position": next }))?;
        next += 1;
    }
    db::delete::<Record>("planner_bucket", key(id_text(&b)))?;
    Ok(json!({ "moved_to": target_key }))
}

#[derive(Deserialize)]
pub struct MoveBucket {
    #[serde(alias = "id")]
    bucket: String,
    /// The bucket it goes in front of; none puts it last.
    #[serde(default)]
    before: Option<String>,
}

pub fn move_bucket(input: MoveBucket) -> Result<Value> {
    let (b, plan) = bucket_in_plan(&input.bucket)?;
    let mut order: Vec<String> = buckets_of(id_text(&plan))?.iter().map(|x| key(id_text(x)).to_string()).filter(|k| k != key(id_text(&b))).collect();
    let at = input.before.as_deref().and_then(|bf| order.iter().position(|k| k == key(bf))).unwrap_or(order.len());
    order.insert(at, key(id_text(&b)).to_string());
    for (i, k) in order.iter().enumerate() {
        db::update::<Record>("planner_bucket", k, &json!({ "position": i as i64 + 1 }))?;
    }
    Ok(json!({ "ok": true }))
}

// ---- labels ---------------------------------------------------------------------------------------------

pub fn labels_of(plan: &str) -> Result<Vec<Record>> {
    db::find("planner_label").filter("plan", key(plan)).limit(200).all()
}

#[derive(Deserialize)]
pub struct NewLabel {
    plan: String,
    name: String,
    #[serde(default)]
    color: Option<String>,
}

pub fn add_label(input: NewLabel) -> Result<Value> {
    let (plan, _, _) = require(&input.plan, Role::Member)?;
    let name = clean("a name", &input.name, 40)?;
    let existing = labels_of(id_text(&plan))?;
    if existing.len() >= MAX_LABELS {
        return Err(Error::msg(format!("a plan has at most {MAX_LABELS} labels")));
    }
    if existing.iter().any(|l| text(l, "name").is_some_and(|n| n.eq_ignore_ascii_case(&name))) {
        return Err(Error::msg(format!("there is already a label named {name}")));
    }
    let label: Record = db::create("planner_label", &json!({ "plan": key(id_text(&plan)), "name": name, "color": input.color }))?;
    Ok(json!({ "id": key(id_text(&label)) }))
}

#[derive(Deserialize)]
pub struct LabelOnly {
    #[serde(alias = "id")]
    label: String,
}

pub fn delete_label(input: LabelOnly) -> Result<Value> {
    let label = db::get::<Record>("planner_label", key(&input.label))?.ok_or_else(|| Error::msg("there is no such label"))?;
    let (plan, _, _) = require(text(&label, "plan").unwrap_or_default(), Role::Member)?;
    crate::tasks::drop_label(id_text(&plan), key(id_text(&label)))?;
    db::delete::<Record>("planner_label", key(id_text(&label)))?;
    Ok(json!({ "ok": true }))
}

#[allow(dead_code)]
fn unused(_: Filter) {}

#[cfg(test)]
mod tests {
    #[test]
    fn a_new_plan_starts_with_three_buckets() {
        assert_eq!(super::DEFAULT_BUCKETS.len(), 3);
    }
}
