//! Who may see and change a plan: its members; its owner manages it.

use aether_sdk::prelude::*;

use crate::common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Member,
    Owner,
}

pub fn member_row(plan: &str, user: &str) -> Result<Option<Record>> {
    let rows: Vec<Record> = db::find("planner_member").filter("plan", key(plan)).limit(500).all()?;
    Ok(rows.into_iter().find(|m| text(m, "user").is_some_and(|u| key(u) == key(user))))
}

pub fn members_of(plan: &str) -> Result<Vec<Record>> {
    db::find("planner_member").filter("plan", key(plan)).limit(500).all()
}

pub fn role_of(plan: &str, user: &str) -> Result<Option<Role>> {
    Ok(member_row(plan, user)?.map(|m| if text(&m, "role") == Some("owner") { Role::Owner } else { Role::Member }))
}

/// The plan and the caller's role in it, at least `min`. A plan the caller is not in does not exist for them.
pub fn require(plan: &str, min: Role) -> Result<(Record, Role, String)> {
    let me = me()?;
    let record = db::get::<Record>("planner_plan", key(plan))?.ok_or_else(|| Error::msg("there is no such plan"))?;
    match role_of(key(id_text(&record)), &me)? {
        Some(role) if role >= min => Ok((record, role, me)),
        Some(_) => Err(Error::msg("only the owner of the plan can do this")),
        None => Err(Error::msg("there is no such plan")),
    }
}

/// A task, its plan and the caller's role.
pub fn task_for(task: &str) -> Result<(Record, Record, Role, String)> {
    let record = db::get::<Record>("planner_task", key(task))?.ok_or_else(|| Error::msg("there is no such task"))?;
    let (plan, role, me) = require(text(&record, "plan").unwrap_or_default(), Role::Member)?;
    Ok((record, plan, role, me))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_owner_outranks_a_member() {
        assert!(Role::Owner > Role::Member);
    }
}
