//! Atrium and HR: a chat channel for each department, kept in step with who works there.
//!
//! HR is the record of who is in which department; chat is where they talk. This plugin reads the first and writes
//! the second, through the public functions of both (`hr.list_departments`, `hr.list_employees`, `atrium_chat.ensure_channel`,
//! `atrium_chat.sync_members`), so neither depends on the other.

use aether_sdk::{events::Received, prelude::*};

/// The team the department channels live in.
const TEAM: &str = "Departments";
/// The team the channels of a manager and their reports live in.
const MANAGER_TEAM: &str = "Teams";
/// Statuses of people who no longer work here.
const GONE: &[&str] = &["resigned", "terminated", "retired", "deceased"];

type Record = Value;

fn text<'a>(record: &'a Record, field: &str) -> Option<&'a str> {
    record.get(field).and_then(Value::as_str)
}

fn is_gone(status: Option<&str>) -> bool {
    status.is_some_and(|s| GONE.contains(&s))
}

/// The login accounts of the people working in a department, from HR's employees.
fn people_of(department: &str) -> Result<Vec<String>> {
    let answer: Value = plugins::call("hr", "list_employees", &json!({ "department": department, "limit": 500 }))?;
    let rows = answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(accounts(&rows))
}

/// The linked login of each employee still working here; people with no login have no chat to join.
fn accounts(employees: &[Record]) -> Vec<String> {
    let mut users: Vec<String> = employees
        .iter()
        .filter(|e| !is_gone(text(e, "status")))
        .filter_map(|e| text(e, "user").map(str::trim).filter(|u| !u.is_empty()).map(str::to_string))
        .collect();
    users.sort();
    users.dedup();
    users
}

/// The channel name for a department: its name, as HR spells it.
fn channel_name(department: &Record) -> Option<String> {
    text(department, "name").map(str::trim).filter(|n| !n.is_empty()).map(str::to_string)
}

fn sync_one(department: &Record) -> Result<Value> {
    let Some(name) = channel_name(department) else { return Ok(json!({ "skipped": true })) };
    let id = text(department, "id").unwrap_or_default();
    let users = people_of(id)?;
    let topic = format!("The {name} department");
    let ensured: Value = plugins::call(
        "atrium_chat",
        "ensure_channel",
        &json!({ "name": name, "team": TEAM, "topic": topic, "members": users }),
    )?;
    let channel = ensured.get("id").and_then(Value::as_str).unwrap_or_default();
    // The department decides who is in its channel: transfers out take people out.
    let synced: Value = plugins::call("atrium_chat", "sync_members", &json!({ "channel": channel, "users": users, "remove_others": true }))?;
    Ok(json!({ "department": name, "channel": channel, "people": users.len(), "created": ensured.get("created"), "removed": synced.get("removed") }))
}

/// What the directory profile of an employee says, from HR's record: the job (else the position) as the title, the
/// department, the work contact details, and the manager's login. `manager_user` is empty for no manager.
fn profile_fields(employee: &Record, job: &str, department: &str, manager_user: &str) -> Value {
    let clean = |field: &str| text(employee, field).map(str::trim).unwrap_or_default().to_string();
    let mut fields = json!({
        "started": clean("hire_date"),
        "title": job, "department": department, "manager": manager_user,
        "email": clean("work_email"), "phone": clean("work_phone"), "location": clean("work_location_text"),
    });
    // The photo HR holds, when there is one: a person with no photo in HR keeps the one they chose.
    if let Some(photo) = employee.get("image").filter(|p| p.is_object()) {
        fields["photo"] = photo.clone();
    }
    fields
}

/// The name HR gives a job, position or department, by id; empty when there is none.
fn name_by_id(function: &str, id: Option<&str>) -> String {
    let Some(id) = id.filter(|i| !i.is_empty()) else { return String::new() };
    plugins::call::<Option<Record>>("hr", function, &json!({ "id": id })).ok().flatten().and_then(|r| text(&r, "name").map(str::to_string)).unwrap_or_default()
}

/// Put what HR knows about an employee on their directory profile (people without a login have none). A failure
/// here never stops the rest of the sync: the directory is a convenience.
fn sync_profile(employee: &Record, manager_user: &str) -> Value {
    let Some(user) = text(employee, "user").filter(|u| !u.is_empty()) else { return json!({ "skipped": true }) };
    if is_gone(text(employee, "status")) {
        return json!({ "skipped": true });
    }
    let job = {
        let j = name_by_id("get_job", text(employee, "job"));
        if j.is_empty() { name_by_id("get_position", text(employee, "position")) } else { j }
    };
    let department = name_by_id("get_department", text(employee, "department"));
    let mut fields = profile_fields(employee, &job, &department, manager_user);
    fields["id"] = json!(user);
    match plugins::call::<Value>("atrium_people", "apply_hr", &fields) {
        Ok(_) => json!({ "applied": true }),
        Err(error) => json!({ "applied": false, "error": error.to_string() }),
    }
}

/// People on approved leave today are shown as away until their leave ends; everyone else is shown as here. Needs
/// HR's leave plugin: without it nobody is marked.
fn sync_away() -> Result<Value> {
    let today = records::today()?;
    let away: Vec<Record> = match plugins::call("hr_leave", "who_is_away_between", &json!({ "from": today, "to": today })) {
        Ok(rows) => rows,
        Err(_) => return Ok(json!({ "skipped": true })),
    };
    let answer: Value = plugins::call("hr", "list_employees", &json!({ "limit": 500 }))?;
    let rows = answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut marked = 0;
    for employee in rows.iter().filter(|e| !is_gone(text(e, "status"))) {
        let (Some(id), Some(user)) = (text(employee, "id"), text(employee, "user").filter(|u| !u.is_empty())) else { continue };
        let until = away.iter().filter(|a| text(a, "employee") == Some(id)).filter_map(|a| text(a, "end_date")).max().unwrap_or_default();
        let result = plugins::call::<Value>("atrium_people", "set_away", &json!({ "id": user, "until": until, "note": "On leave" }));
        if until.is_empty() || result.is_err() {
            continue;
        }
        marked += 1;
    }
    Ok(json!({ "away": marked }))
}

/// The teams of managers: a manager and the people who report to them, as logins. Only managers who have a login and at
/// least one report with a login are listed; people who left are in no team.
fn manager_teams(employees: &[Record]) -> Vec<(String, Vec<String>)> {
    let working: Vec<&Record> = employees.iter().filter(|e| !is_gone(text(e, "status"))).collect();
    let mut teams = Vec::new();
    for manager in &working {
        let (Some(id), Some(login)) = (text(manager, "id"), text(manager, "user").filter(|u| !u.is_empty())) else { continue };
        let name = text(manager, "display_name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(login);
        let mut people: Vec<String> =
            working.iter().filter(|e| text(e, "manager") == Some(id)).filter_map(|e| text(e, "user").filter(|u| !u.is_empty()).map(str::to_string)).collect();
        if people.is_empty() {
            continue;
        }
        people.push(login.to_string());
        people.sort();
        people.dedup();
        teams.push((format!("{name}'s team"), people));
    }
    teams
}

/// A private channel for each manager and their reports, kept in step with who reports to whom.
fn sync_manager_teams() -> Result<Value> {
    let answer: Value = plugins::call("hr", "list_employees", &json!({ "limit": 500 }))?;
    let rows = answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut done = 0;
    for (name, users) in manager_teams(&rows) {
        let ensured: Value = plugins::call(
            "atrium_chat",
            "ensure_channel",
            &json!({ "name": name, "team": MANAGER_TEAM, "topic": "A manager and the people who report to them", "private": true, "members": users }),
        )?;
        let channel = ensured.get("id").and_then(Value::as_str).unwrap_or_default();
        plugins::call::<Value>("atrium_chat", "sync_members", &json!({ "channel": channel, "users": users, "remove_others": true }))?;
        done += 1;
    }
    Ok(json!({ "teams": done }))
}

/// Every employee with a login gets their profile in step with HR.
fn sync_profiles() -> Result<Value> {
    let answer: Value = plugins::call("hr", "list_employees", &json!({ "limit": 500 }))?;
    let rows = answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    let logins: std::collections::HashMap<String, String> = rows
        .iter()
        .filter_map(|e| Some((text(e, "id")?.to_string(), text(e, "user")?.to_string())))
        .collect();
    let mut applied = 0;
    for employee in &rows {
        let manager = text(employee, "manager").and_then(|m| logins.get(m)).map(String::as_str).unwrap_or_default();
        if sync_profile(employee, manager)["applied"] == true {
            applied += 1;
        }
    }
    Ok(json!({ "profiles": applied }))
}

/// Every active department gets its channel and its people. Safe to run again.
fn sync_all() -> Result<Value> {
    let departments: Vec<Record> = plugins::call("hr", "list_departments", &json!({}))?;
    let mut done = Vec::new();
    for department in departments.iter().filter(|d| d.get("is_active").and_then(Value::as_bool) != Some(false)) {
        done.push(sync_one(department)?);
    }
    let profiles = sync_profiles().unwrap_or_else(|error| json!({ "error": error.to_string() }));
    let away = sync_away().unwrap_or_else(|error| json!({ "error": error.to_string() }));
    let teams = sync_manager_teams().unwrap_or_else(|error| json!({ "error": error.to_string() }));
    Ok(json!({ "departments": done.len(), "results": done, "profiles": profiles, "away": away, "manager_teams": teams }))
}

/// Someone started, came back or left: bring their department's channel up to date, and take a person who left out
/// of every shared conversation.
fn on_event(received: &Received) -> Result<Value> {
    let Some(employee_id) = received.payload.get("employee").and_then(Value::as_str) else { return Ok(json!({ "ignored": true })) };
    let employee: Option<Record> = plugins::call("hr", "get_employee", &json!({ "id": employee_id }))?;
    let Some(employee) = employee else { return Ok(json!({ "ignored": true })) };
    if is_gone(text(&employee, "status")) {
        if let Some(user) = text(&employee, "user").filter(|u| !u.is_empty()) {
            let removed: Value = plugins::call("atrium_chat", "remove_everywhere", &json!({ "user": user }))?;
            return Ok(json!({ "left": true, "removed": removed.get("removed") }));
        }
        return Ok(json!({ "left": true }));
    }
    let manager_user = text(&employee, "manager")
        .and_then(|m| plugins::call::<Option<Record>>("hr", "get_employee", &json!({ "id": m })).ok().flatten())
        .and_then(|m| text(&m, "user").map(str::to_string))
        .unwrap_or_default();
    sync_profile(&employee, &manager_user);
    let _ = sync_manager_teams();
    match text(&employee, "department").filter(|d| !d.is_empty()) {
        Some(department_id) => {
            let department: Option<Record> = plugins::call("hr", "get_department", &json!({ "id": department_id }))?;
            match department {
                Some(d) => sync_one(&d),
                None => Ok(json!({ "ignored": true })),
            }
        }
        None => Ok(json!({ "ignored": true })),
    }
}

/// Leave was approved: show it on the directory at once (the nightly run also does, and clears what has ended).
fn on_leave(_received: &Received) -> Result<Value> {
    sync_away()
}

handler! {
    fn on_leave_event(received: Received) -> Value { on_leave(&received) }
    fn sync_departments(_input: Empty) -> Value { sync_all() }
    fn on_employee_event(received: Received) -> Value { on_event(&received) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_people_still_working_with_a_login_are_put_in_the_channel() {
        let employees = vec![
            json!({ "user": "users:ann", "status": "active" }),
            json!({ "user": "users:bob", "status": "terminated" }),
            json!({ "user": "", "status": "active" }),
            json!({ "status": "probation" }),
            json!({ "user": "users:cy", "status": "on_leave" }),
            json!({ "user": "users:ann", "status": "active" }),
        ];
        assert_eq!(accounts(&employees), ["users:ann", "users:cy"]);
    }

    #[test]
    fn a_department_is_named_as_hr_spells_it() {
        assert_eq!(channel_name(&json!({ "name": " Sales " })).as_deref(), Some("Sales"));
        assert_eq!(channel_name(&json!({ "name": "" })), None);
        assert_eq!(channel_name(&json!({})), None);
    }

    #[test]
    fn a_profile_takes_hr_titles_and_contacts() {
        let e = json!({ "work_email": " ann@x.test ", "work_phone": "123", "work_location_text": "" });
        let f = profile_fields(&e, "Designer", "Product", "users:boss");
        assert_eq!(f["title"], "Designer");
        assert_eq!((f["email"].as_str(), f["phone"].as_str(), f["location"].as_str()), (Some("ann@x.test"), Some("123"), Some("")));
        assert_eq!(f["manager"], "users:boss");
        assert_eq!(f["started"], "");
        assert!(f.get("photo").is_none());
        let with_photo = profile_fields(&json!({ "image": { "key": "media/x" }, "hire_date": "2020-01-05" }), "", "", "");
        assert_eq!(with_photo["photo"]["key"], "media/x");
        assert_eq!(with_photo["started"], "2020-01-05");
    }

    #[test]
    fn a_manager_and_the_people_reporting_to_them_make_a_team() {
        let employees = vec![
            json!({ "id": "e1", "user": "users:ann", "display_name": "Ann Lee", "status": "active" }),
            json!({ "id": "e2", "user": "users:bob", "manager": "e1", "status": "active" }),
            json!({ "id": "e3", "user": "users:cy", "manager": "e1", "status": "terminated" }),
            json!({ "id": "e4", "user": "", "manager": "e1", "status": "active" }),
            json!({ "id": "e5", "user": "users:di", "manager": "e2", "status": "active" }),
        ];
        let teams = manager_teams(&employees);
        assert_eq!(teams[0], ("Ann Lee's team".to_string(), vec!["users:ann".to_string(), "users:bob".to_string()]));
        assert_eq!(teams[1].1, ["users:bob", "users:di"]);
        assert_eq!(teams.len(), 2);
    }

    #[test]
    fn leaving_statuses_are_known() {
        assert!(is_gone(Some("resigned")) && is_gone(Some("retired")));
        assert!(!is_gone(Some("on_leave")) && !is_gone(None));
    }
}
