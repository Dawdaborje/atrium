//! The directory, profiles, reporting lines and the org chart.

use std::collections::{HashMap, HashSet};

use aether_sdk::dates::{format_date, parse_date, Datelike, Duration, NaiveDate};
use aether_sdk::prelude::*;

use crate::common::*;

/// The fields of a profile that are plain text, with the longest each may be.
const TEXT_FIELDS: [(&str, usize); 7] = [("title", 120), ("department", 120), ("email", 200), ("phone", 60), ("mobile", 60), ("location", 120), ("skills", 400)];
const MAX_BIO: usize = 2000;
const MAX_DEPTH: usize = 40;
const MAX_CHART_ROWS: usize = 2000;
/// A reporting line no deeper than this is drawn.
const CHART_DEPTH: usize = 12;

fn profiles() -> Result<HashMap<String, Record>> {
    Ok(db::find::<Record>("people_profile")
        .limit(5000)
        .all()?
        .into_iter()
        .filter_map(|p| text(&p, "user").map(|u| (key(u).to_string(), p.clone())))
        .collect())
}

fn field(profile: Option<&Record>, name: &str) -> String {
    profile.and_then(|p| text(p, name)).unwrap_or_default().to_string()
}

/// The directory row for a person: the account's name, and whatever their profile says.
fn card(person: &Person, profile: Option<&Record>, names: &HashMap<String, String>) -> Value {
    let manager = field(profile, "manager");
    let today = today().unwrap_or_default();
    let until = field(profile, "away_until");
    let away = !until.is_empty() && until >= today;
    let note = field(profile, "away_note");
    json!({
        "away": away, "away_until": if away { until.clone() } else { String::new() },
        "away_text": if away { away_text(&until, &note) } else { String::new() },
        "birthday": field(profile, "birthday"), "started": field(profile, "started"),
        "id": key(&person.id), "name": person.name, "title": field(profile, "title"), "department": field(profile, "department"),
        "email": field(profile, "email"), "phone": field(profile, "phone"), "mobile": field(profile, "mobile"),
        "location": field(profile, "location"), "skills": field(profile, "skills"), "bio": field(profile, "bio"),
        "photo": profile.and_then(|p| p.get("photo")), "manager": manager,
        "manager_name": names.get(key(&manager)).cloned().unwrap_or_default(),
        "hr_managed": profile.and_then(|p| p.get("hr_managed")).cloned().unwrap_or_else(|| json!([])),
    })
}

/// "Away until 2026-10-20: on leave".
fn away_text(until: &str, note: &str) -> String {
    if note.is_empty() {
        format!("Away until {until}")
    } else {
        format!("Away until {until}: {note}")
    }
}

fn names_of(people: &[Person]) -> HashMap<String, String> {
    people.iter().map(|p| (key(&p.id).to_string(), p.name.clone())).collect()
}

fn matches(row: &Value, needle: &str) -> bool {
    let n = needle.to_lowercase();
    ["name", "title", "department", "email", "location", "skills"]
        .iter()
        .any(|f| row[*f].as_str().is_some_and(|v| v.to_lowercase().contains(&n)))
}

#[derive(Deserialize, Default)]
pub struct Directory {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    department: Option<String>,
}

pub fn directory(input: Directory) -> Result<Vec<Value>> {
    me()?;
    let people = org_people()?;
    let names = names_of(&people);
    let profs = profiles()?;
    let needle = input.q.as_deref().map(str::trim).unwrap_or_default().to_string();
    let dept = input.department.as_deref().map(str::trim).unwrap_or_default().to_lowercase();
    let mut rows: Vec<Value> = people.iter().map(|p| card(p, profs.get(key(&p.id)), &names)).collect();
    rows.retain(|r| (needle.is_empty() || matches(r, &needle)) && (dept.is_empty() || r["department"].as_str().is_some_and(|d| d.to_lowercase() == dept)));
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

/// The departments people named, with how many, for the filter.
pub fn department_options() -> Result<Vec<Value>> {
    me()?;
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
    for p in profiles()?.values() {
        let d = text(p, "department").unwrap_or_default().trim();
        if !d.is_empty() {
            counts.entry(d.to_lowercase()).or_insert_with(|| (d.to_string(), 0)).1 += 1;
        }
    }
    let mut rows: Vec<(String, usize)> = counts.into_values().collect();
    rows.sort_by_key(|(n, _)| n.to_lowercase());
    Ok(rows.into_iter().map(|(n, c)| json!({ "id": n, "name": n, "count": c })).collect())
}

#[derive(Deserialize, Default)]
pub struct PersonIn {
    #[serde(default)]
    id: Option<String>,
}

fn who(input: &PersonIn) -> Result<String> {
    match input.id.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        Some(id) => Ok(key(id).to_string()),
        None => Ok(key(&me()?).to_string()),
    }
}

fn find_person<'a>(people: &'a [Person], id: &str) -> Result<&'a Person> {
    people.iter().find(|p| key(&p.id) == key(id)).ok_or_else(|| Error::msg("there is no such person"))
}

fn can_edit(target: &str) -> Result<bool> {
    Ok(key(&me()?) == key(target) || context::current()?.has_role("org_admin"))
}

pub fn person(input: PersonIn) -> Result<Value> {
    me()?;
    let id = who(&input)?;
    let people = org_people()?;
    let target = find_person(&people, &id)?;
    let profs = profiles()?;
    let mut row = card(target, profs.get(&id), &names_of(&people));
    row["can_edit"] = json!(can_edit(&id)?);
    row["reports"] = json!(profs.values().filter(|p| text(p, "manager").is_some_and(|m| key(m) == id)).count());
    Ok(row)
}

pub fn my_profile() -> Result<Value> {
    person(PersonIn { id: None })
}

pub fn reports(input: PersonIn) -> Result<Vec<Value>> {
    me()?;
    let id = who(&input)?;
    let people = org_people()?;
    let names = names_of(&people);
    let profs = profiles()?;
    let mut rows: Vec<Value> = people
        .iter()
        .filter(|p| profs.get(key(&p.id)).and_then(|pr| text(pr, "manager")).is_some_and(|m| key(m) == id))
        .map(|p| card(p, profs.get(key(&p.id)), &names))
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

/// The managers above a person, nearest first.
pub fn chain(input: PersonIn) -> Result<Vec<Value>> {
    me()?;
    let id = who(&input)?;
    let people = org_people()?;
    let names = names_of(&people);
    let profs = profiles()?;
    let mut out = Vec::new();
    let mut seen = HashSet::from([id.clone()]);
    let mut at = id;
    for _ in 0..MAX_DEPTH {
        let Some(manager) = profs.get(&at).and_then(|p| text(p, "manager")).map(|m| key(m).to_string()).filter(|m| !m.is_empty()) else { break };
        if !seen.insert(manager.clone()) {
            break;
        }
        let Some(person) = people.iter().find(|p| key(&p.id) == manager) else { break };
        out.push(card(person, profs.get(&manager), &names));
        at = manager;
    }
    Ok(out)
}

#[derive(Deserialize, Default)]
pub struct Chart {
    /// Draw only this person and everyone below them; otherwise everyone, from the top.
    #[serde(default)]
    id: Option<String>,
}

/// Where a reporting line starts: a person with no manager, or whose manager is not (or no longer) a member.
fn roots(people: &[Person], profs: &HashMap<String, Record>) -> Vec<String> {
    let members: HashSet<&str> = people.iter().map(|p| key(&p.id)).collect();
    let mut out: Vec<String> = people
        .iter()
        .filter(|p| {
            profs
                .get(key(&p.id))
                .and_then(|pr| text(pr, "manager"))
                .map(key)
                .filter(|m| !m.is_empty() && *m != key(&p.id))
                .is_none_or(|m| !members.contains(m))
        })
        .map(|p| key(&p.id).to_string())
        .collect();
    out.sort_by_key(|k| people.iter().find(|p| key(&p.id) == k).map(|p| p.name.to_lowercase()).unwrap_or_default());
    out
}

pub fn org_chart(input: Chart) -> Result<Vec<Value>> {
    me()?;
    let people = org_people()?;
    let names = names_of(&people);
    let profs = profiles()?;
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for p in &people {
        if let Some(m) = profs.get(key(&p.id)).and_then(|pr| text(pr, "manager")).map(key).filter(|m| !m.is_empty() && *m != key(&p.id)) {
            children.entry(m.to_string()).or_default().push(key(&p.id).to_string());
        }
    }
    for list in children.values_mut() {
        list.sort_by_key(|k| names.get(k).map(|n| n.to_lowercase()).unwrap_or_default());
    }
    let start = match input.id.as_deref().map(str::trim).filter(|i| !i.is_empty()) {
        Some(id) => {
            find_person(&people, id)?;
            vec![key(id).to_string()]
        }
        None => roots(&people, &profs),
    };
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    let mut stack: Vec<(String, usize)> = start.into_iter().rev().map(|k| (k, 0)).collect();
    while let Some((id, depth)) = stack.pop() {
        if rows.len() >= MAX_CHART_ROWS || !seen.insert(id.clone()) {
            continue;
        }
        let Some(person) = people.iter().find(|p| key(&p.id) == id) else { continue };
        let below = children.get(&id).cloned().unwrap_or_default();
        let mut row = card(person, profs.get(&id), &names);
        row["depth"] = json!(depth);
        row["label"] = json!(format!("{}{}", "\u{2003}".repeat(depth), person.name));
        row["reports"] = json!(below.len());
        rows.push(row);
        if depth < CHART_DEPTH {
            for child in below.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }
    Ok(rows)
}

#[derive(Deserialize, Default)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
}

/// People to choose from, for a manager.
pub fn pick_people(input: Pick) -> Result<Vec<Value>> {
    me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let mut rows: Vec<Value> = org_people()?
        .into_iter()
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .map(|p| json!({ "id": p.id, "name": p.name }))
        .collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

// ---- changing -------------------------------------------------------------------------------------------------

fn managed(profile: Option<&Record>) -> Vec<String> {
    strings(profile.and_then(|p| p.get("hr_managed")))
}

/// A manager is a member other than the person, and the line above them does not lead back to the person.
fn check_manager(target: &str, manager: &str, people: &[Person], profs: &HashMap<String, Record>) -> Result<()> {
    if manager.is_empty() {
        return Ok(());
    }
    if key(manager) == target {
        return Err(Error::msg("nobody reports to themselves"));
    }
    find_person(people, manager).map_err(|_| Error::msg("that manager is not a member of this organization"))?;
    let mut at = key(manager).to_string();
    for _ in 0..MAX_DEPTH {
        let Some(next) = profs.get(&at).and_then(|p| text(p, "manager")).map(key).filter(|m| !m.is_empty()) else { return Ok(()) };
        if next == target {
            return Err(Error::msg("that would make a loop: they already report, through others, to this person"));
        }
        at = next.to_string();
    }
    Ok(())
}

fn clean_field(name: &str, max: usize, value: &str) -> Result<String> {
    let v = value.trim();
    if v.chars().count() > max {
        return Err(Error::msg(format!("{name} is at most {max} characters")));
    }
    if name == "email" && !v.is_empty() && (!v.contains('@') || v.contains(' ')) {
        return Err(Error::msg("that is not an email address"));
    }
    Ok(v.to_string())
}

/// Write the given fields onto a person's profile, creating it when there is none.
fn write(target: &str, mut data: serde_json::Map<String, Value>) -> Result<()> {
    data.insert("updated_at".into(), json!(now()?));
    match db::find::<Record>("people_profile").filter("user", target).first()? {
        Some(existing) => {
            db::update::<Record>("people_profile", key(id_text(&existing)), &Value::Object(data))?;
        }
        None => {
            data.insert("user".into(), json!(target));
            db::create::<Record>("people_profile", &Value::Object(data))?;
        }
    }
    events::emit("profile_changed", &json!({ "user": target }))?;
    Ok(())
}

#[derive(Deserialize, Default)]
pub struct Save {
    /// Whose profile; the caller's own when not given. Only an administrator may change someone else's.
    #[serde(default, alias = "id")]
    user: Option<String>,
    #[serde(flatten)]
    fields: serde_json::Map<String, Value>,
}

pub fn save_profile(input: Save) -> Result<Value> {
    let target = match input.user.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => key(u).to_string(),
        None => key(&me()?).to_string(),
    };
    if !can_edit(&target)? {
        return Err(Error::msg("you can only change your own profile"));
    }
    let people = org_people()?;
    find_person(&people, &target)?;
    let profs = profiles()?;
    let kept = managed(profs.get(&target));
    let mut data = serde_json::Map::new();
    let current = profs.get(&target);
    // A field HR keeps cannot be changed here; sending back what is already there (a whole form is sent) is fine.
    let mut refuse = |name: &str, same: bool| -> Result<bool> {
        if !kept.iter().any(|k| k == name) {
            return Ok(false);
        }
        if same {
            return Ok(true);
        }
        Err(Error::msg(format!("{name} is kept up to date from the HR record and cannot be changed here")))
    };
    for (name, max) in TEXT_FIELDS {
        if let Some(v) = input.fields.get(name) {
            let value = clean_field(name, max, v.as_str().unwrap_or_default())?;
            if refuse(name, value == field(current, name))? {
                continue;
            }
            data.insert(name.into(), json!(value));
        }
    }
    if let Some(v) = input.fields.get("bio") {
        let bio = v.as_str().unwrap_or_default().trim();
        if bio.chars().count() > MAX_BIO {
            return Err(Error::msg(format!("a bio is at most {MAX_BIO} characters")));
        }
        data.insert("bio".into(), json!(bio));
    }
    if let Some(photo) = input.fields.get("photo") {
        let same = current.and_then(|p| p.get("photo")).unwrap_or(&Value::Null) == photo;
        if !refuse("photo", same)? {
            data.insert("photo".into(), photo.clone());
        }
    }
    if let Some(v) = input.fields.get("birthday") {
        data.insert("birthday".into(), json!(check_birthday(v.as_str().unwrap_or_default())?));
    }
    // Saying you are away: the last day and a few words. An empty last day ends it.
    if input.fields.contains_key("away_until") || input.fields.contains_key("away_note") {
        let until = input.fields.get("away_until").and_then(Value::as_str).map(str::trim).unwrap_or_default().to_string();
        let note = input.fields.get("away_note").and_then(Value::as_str).map(str::trim).unwrap_or_default();
        if note.chars().count() > 120 {
            return Err(Error::msg("the note is at most 120 characters"));
        }
        // Sent back unchanged with the form while HR's away status stands: leave it alone.
        let hr_away = field(current, "away_source") == "hr" && !field(current, "away_until").is_empty();
        let unchanged = until == field(current, "away_until") && note == field(current, "away_note");
        if !(hr_away && unchanged) {
            if hr_away {
                return Err(Error::msg("you are away because of approved leave; that ends by itself"));
            }
            if until.is_empty() {
                data.insert("away_until".into(), json!(""));
                data.insert("away_note".into(), json!(""));
                data.insert("away_source".into(), json!(""));
            } else {
                parse_date(&until).map_err(|_| Error::msg("write the last day away as YYYY-MM-DD"))?;
                data.insert("away_until".into(), json!(until));
                data.insert("away_note".into(), json!(note));
                data.insert("away_source".into(), json!("self"));
            }
        }
    }
    if let Some(v) = input.fields.get("manager") {
        let manager = v.as_str().unwrap_or_default().trim().to_string();
        if !refuse("manager", key(&manager) == key(&field(current, "manager")))? {
            check_manager(&target, &manager, &people, &profs)?;
            data.insert("manager".into(), json!(manager));
        }
    }
    write(&target, data)?;
    Ok(json!({ "saved": true }))
}

#[derive(Deserialize)]
pub struct ApplyHr {
    #[serde(alias = "id")]
    user: String,
    /// Any of the profile's fields, as HR has them. Each one given is from then on kept by HR.
    #[serde(flatten)]
    fields: serde_json::Map<String, Value>,
}

/// For the HR link: set what HR knows about a person and mark those fields as HR's, so people cannot change them.
pub fn apply_hr(input: ApplyHr) -> Result<Value> {
    context::current()?.require_role("org_admin")?;
    let target = key(&input.user).to_string();
    let people = org_people()?;
    find_person(&people, &target)?;
    let profs = profiles()?;
    let mut data = serde_json::Map::new();
    let mut kept = managed(profs.get(&target));
    for (name, max) in TEXT_FIELDS {
        if let Some(v) = input.fields.get(name) {
            let value = clean_field(name, max, v.as_str().unwrap_or_default()).unwrap_or_default();
            data.insert(name.into(), json!(value));
            if !kept.iter().any(|k| k == name) {
                kept.push(name.to_string());
            }
        }
    }
    if let Some(photo) = input.fields.get("photo") {
        data.insert("photo".into(), photo.clone());
        if !kept.iter().any(|k| k == "photo") {
            kept.push("photo".into());
        }
    }
    if let Some(v) = input.fields.get("started") {
        let day = v.as_str().unwrap_or_default().trim();
        if day.is_empty() || parse_date(day).is_ok() {
            data.insert("started".into(), json!(day));
        }
    }
    if let Some(v) = input.fields.get("manager") {
        let manager = v.as_str().unwrap_or_default().trim().to_string();
        // A line that would loop is left as it was: HR's data is not allowed to break the chart.
        if check_manager(&target, &manager, &people, &profs).is_ok() {
            data.insert("manager".into(), json!(manager));
            if !kept.iter().any(|k| k == "manager") {
                kept.push("manager".into());
            }
        }
    }
    data.insert("hr_managed".into(), json!(kept));
    write(&target, data)?;
    Ok(json!({ "applied": true }))
}

/// For the HR link: give the fields back to the person.
pub fn release_hr(input: PersonIn) -> Result<Value> {
    context::current()?.require_role("org_admin")?;
    let target = who(&input)?;
    if let Some(existing) = db::find::<Record>("people_profile").filter("user", target.as_str()).first()? {
        db::update::<Record>("people_profile", key(id_text(&existing)), &json!({ "hr_managed": [] }))?;
    }
    Ok(json!({ "released": true }))
}

#[derive(Deserialize)]
pub struct SetAway {
    #[serde(alias = "id")]
    user: String,
    #[serde(default)]
    until: Option<String>,
    #[serde(default)]
    note: Option<String>,
}

/// For the HR link: someone is away on approved leave until a day (or no longer is). A status the person set
/// themselves is never touched, and neither does an empty `until` clear one.
pub fn set_away(input: SetAway) -> Result<Value> {
    context::current()?.require_role("org_admin")?;
    let target = key(&input.user).to_string();
    find_person(&org_people()?, &target)?;
    let profs = profiles()?;
    let current = profs.get(&target);
    if field(current, "away_source") == "self" && !field(current, "away_until").is_empty() && field(current, "away_until") >= today()? {
        return Ok(json!({ "kept": true }));
    }
    let until = input.until.as_deref().map(str::trim).unwrap_or_default();
    if until.is_empty() {
        if current.is_none() || field(current, "away_until").is_empty() {
            return Ok(json!({ "unchanged": true }));
        }
        let mut data = serde_json::Map::new();
        for f in ["away_until", "away_note", "away_source"] {
            data.insert(f.into(), json!(""));
        }
        write(&target, data)?;
        return Ok(json!({ "cleared": true }));
    }
    parse_date(until)?;
    let note = input.note.as_deref().unwrap_or("On leave").trim().chars().take(120).collect::<String>();
    if field(current, "away_until") == until && field(current, "away_note") == note && field(current, "away_source") == "hr" {
        return Ok(json!({ "unchanged": true }));
    }
    let mut data = serde_json::Map::new();
    data.insert("away_until".into(), json!(until));
    data.insert("away_note".into(), json!(note));
    data.insert("away_source".into(), json!("hr"));
    write(&target, data)?;
    Ok(json!({ "away": true }))
}

/// `MM-DD`, empty to clear. 02-29 is allowed.
fn check_birthday(value: &str) -> Result<String> {
    let v = value.trim();
    if v.is_empty() {
        return Ok(String::new());
    }
    let bad = || Error::msg("write a birthday as MM-DD, for example 03-14");
    let (m, d) = v.split_once('-').ok_or_else(bad)?;
    let (m, d): (u32, u32) = (m.parse().map_err(|_| bad())?, d.parse().map_err(|_| bad())?);
    // Any leap year checks the day against the month.
    NaiveDate::from_ymd_opt(2024, m, d).ok_or_else(bad)?;
    Ok(format!("{m:02}-{d:02}"))
}

/// The next time a month-and-day comes round on or after `today`; 02-29 falls on 03-01 in other years.
fn next_on(today: NaiveDate, month: u32, day: u32) -> NaiveDate {
    let in_year = |y: i32| NaiveDate::from_ymd_opt(y, month, day).or_else(|| NaiveDate::from_ymd_opt(y, 3, 1)).unwrap_or(today);
    let this = in_year(today.year());
    if this >= today { this } else { in_year(today.year() + 1) }
}

#[derive(Deserialize, Default)]
pub struct Upcoming {
    #[serde(default)]
    days: Option<i64>,
}

/// Birthdays and work anniversaries in the next days, soonest first.
pub fn upcoming(input: Upcoming) -> Result<Vec<Value>> {
    me()?;
    let today = parse_date(&today()?)?;
    let span = input.days.unwrap_or(30).clamp(1, 366);
    let limit = today + Duration::days(span);
    let people = org_people()?;
    let names = names_of(&people);
    let profs = profiles()?;
    let mut rows = Vec::new();
    for p in &people {
        let Some(profile) = profs.get(key(&p.id)) else { continue };
        let mut add = |kind: &str, on: NaiveDate, years: Option<i32>| {
            if on <= limit {
                let mut row = card(p, Some(profile), &names);
                row["kind"] = json!(kind);
                row["on"] = json!(format_date(on));
                row["days_until"] = json!((on - today).num_days());
                row["years"] = json!(years);
                row["what"] = json!(match (kind, years) {
                    ("anniversary", Some(y)) => format!("{y} year{} at work", if y == 1 { "" } else { "s" }),
                    _ => "Birthday".to_string(),
                });
                rows.push(row);
            }
        };
        if let Some((m, d)) = text(profile, "birthday").and_then(|b| b.split_once('-')).and_then(|(m, d)| Some((m.parse().ok()?, d.parse().ok()?))) {
            add("birthday", next_on(today, m, d), None);
        }
        if let Some(start) = text(profile, "started").and_then(|s| parse_date(s).ok()) {
            let on = next_on(today, start.month(), start.day());
            let years = on.year() - start.year();
            if years >= 1 {
                add("anniversary", on, Some(years));
            }
        }
    }
    rows.sort_by_key(|r| (r["days_until"].as_i64().unwrap_or(0), r["name"].as_str().unwrap_or_default().to_lowercase()));
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<Person> {
        ["a", "b", "c"].iter().map(|k| Person { id: format!("users:{k}"), name: k.to_uppercase() }).collect()
    }

    fn profs(pairs: &[(&str, &str)]) -> HashMap<String, Record> {
        pairs.iter().map(|(u, m)| (u.to_string(), json!({ "user": u, "manager": m }))).collect()
    }

    #[test]
    fn nobody_reports_to_themselves_or_in_a_loop() {
        let p = people();
        let profs = profs(&[("b", "users:a"), ("c", "users:b")]);
        assert!(check_manager("a", "users:a", &p, &profs).is_err());
        // a -> c would loop: c reports to b, b to a.
        assert!(check_manager("a", "users:c", &p, &profs).is_err());
        assert!(check_manager("c", "users:a", &p, &profs).is_ok());
        assert!(check_manager("a", "", &p, &profs).is_ok());
        assert!(check_manager("a", "users:nobody", &p, &profs).is_err());
    }

    #[test]
    fn the_chart_starts_with_people_who_report_to_no_member() {
        let p = people();
        let profs = profs(&[("b", "users:a"), ("c", "users:gone")]);
        assert_eq!(roots(&p, &profs), ["a", "c"]);
    }

    #[test]
    fn fields_are_trimmed_and_checked() {
        assert_eq!(clean_field("title", 10, "  Boss ").unwrap(), "Boss");
        assert!(clean_field("title", 3, "Boss").is_err());
        assert!(clean_field("email", 50, "no-at-sign").is_err());
        assert_eq!(clean_field("email", 50, "").unwrap(), "");
    }

    #[test]
    fn birthdays_are_month_and_day() {
        assert_eq!(check_birthday("3-4").unwrap(), "03-04");
        assert_eq!(check_birthday("02-29").unwrap(), "02-29");
        assert_eq!(check_birthday("").unwrap(), "");
        assert!(check_birthday("13-01").is_err() && check_birthday("02-30").is_err() && check_birthday("march").is_err());
    }

    #[test]
    fn the_next_birthday_is_this_year_or_next() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(next_on(d(2026, 10, 18), 10, 18), d(2026, 10, 18));
        assert_eq!(next_on(d(2026, 10, 18), 10, 1), d(2027, 10, 1));
        assert_eq!(next_on(d(2026, 10, 18), 12, 25), d(2026, 12, 25));
        assert_eq!(next_on(d(2026, 10, 18), 2, 29), d(2027, 3, 1));
    }

    #[test]
    fn the_search_looks_at_the_main_fields() {
        let row = json!({ "name": "Ann Lee", "title": "Designer", "department": "Product", "email": "", "location": "Oslo", "skills": "figma, css" });
        assert!(matches(&row, "figma") && matches(&row, "oslo") && matches(&row, "ann"));
        assert!(!matches(&row, "sales"));
    }
}
