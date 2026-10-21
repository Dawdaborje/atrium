//! Forms: who owns them, whether they are open, and who has answered.

use aether_sdk::prelude::*;

use crate::common::*;

pub const ADMIN_ROLE: &str = "org_admin";
const MAX_FORMS: usize = 200;

pub fn s<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

pub fn is_admin() -> Result<bool> {
    Ok(context::current()?.has_role(ADMIN_ROLE))
}

pub fn form_for(id: &str) -> Result<Record> {
    db::get::<Record>("frm_form", key(id))?.ok_or_else(|| Error::msg("there is no such form"))
}

/// The form, for its owner (or an administrator). Others get "no such form" for a draft, and a refusal otherwise.
pub fn owned(id: &str) -> Result<(Record, String)> {
    let me = me()?;
    let form = form_for(id)?;
    if key(s(&form, "owner")) == key(&me) || is_admin()? {
        return Ok((form, me));
    }
    if s(&form, "status") == "draft" {
        return Err(Error::msg("there is no such form"));
    }
    Err(Error::msg("only the owner of the form can do this"))
}

/// A form is open when it is switched on and its closing time has not come.
pub fn is_open(form: &Record, now: &str) -> bool {
    s(form, "status") == "open" && text(form, "closes_at").map_or(true, |c| c > now)
}

pub fn state(form: &Record, now: &str) -> &'static str {
    match s(form, "status") {
        "open" if is_open(form, now) => "open",
        "open" => "closed",
        "closed" => "closed",
        _ => "draft",
    }
}

pub fn questions_of(form: &str) -> Result<Vec<Record>> {
    let mut rows: Vec<Record> = db::find("frm_question").filter("form", key(form)).limit(500).all()?;
    rows.sort_by_key(|q| q.get("position").and_then(Value::as_i64).unwrap_or(0));
    Ok(rows)
}

pub fn responses_of(form: &str) -> Result<Vec<Record>> {
    db::find("frm_response").filter("form", key(form)).limit(20000).all()
}

pub fn answered(form: &str, user: &str) -> Result<bool> {
    Ok(db::find::<Record>("frm_done").filter("form", key(form)).filter("user", user).limit(2).all()?.len() > 0)
}

#[derive(Deserialize)]
pub struct FormOnly {
    pub id: String,
}

fn row(form: &Record, people: &[Person], now: &str, responses: usize, done: Option<bool>) -> Value {
    json!({
        "id": key(id_text(form)), "title": s(form, "title"), "description": text(form, "description"), "state": state(form, now), "owner": name_of(people, s(form, "owner")),
        "responses": responses, "anonymous": flag(form, "anonymous"), "closes": text(form, "closes_at").map(|c| c.chars().take(10).collect::<String>()),
        "done": done.map(|d| if d { "Answered" } else { "To do" }),
    })
}

pub fn my_forms() -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let now = now()?;
    let all: Vec<Record> = db::find("frm_response").limit(50000).all()?;
    let mut forms: Vec<Record> = db::find("frm_form").filter("owner", me.as_str()).limit(500).all()?;
    forms.sort_by_key(|f| std::cmp::Reverse(s(f, "edited_at").to_string()));
    Ok(forms.iter().map(|f| row(f, &people, &now, all.iter().filter(|r| text(r, "form").is_some_and(|k| key(k) == key(id_text(f)))).count(), None)).collect())
}

/// Open forms the caller can answer, those still to do first.
pub fn forms_to_fill() -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let now = now()?;
    let forms: Vec<Record> = db::find("frm_form").filter("status", "open").limit(1000).all()?;
    let mut out = Vec::new();
    for f in forms.iter().filter(|f| is_open(f, &now)) {
        let done = answered(id_text(f), &me)?;
        out.push((done, s(f, "title").to_lowercase(), row(f, &people, &now, 0, Some(done))));
    }
    out.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(out.into_iter().map(|r| r.2).collect())
}

pub fn form_info(input: FormOnly) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    let now = now()?;
    Ok(json!({
        "id": key(id_text(&form)), "title": s(&form, "title"), "description": s(&form, "description"), "status": state(&form, &now), "anonymous": flag(&form, "anonymous"),
        "once": flag(&form, "once"), "closes_at": text(&form, "closes_at"), "thanks": s(&form, "thanks"),
    }))
}

pub fn form_header(input: FormOnly) -> Result<Value> {
    let me = me()?;
    let form = form_for(&input.id)?;
    let now = now()?;
    let mine = key(s(&form, "owner")) == key(&me) || is_admin()?;
    if !mine && state(&form, &now) == "draft" {
        return Err(Error::msg("there is no such form"));
    }
    let people = org_people()?;
    Ok(json!({
        "title": s(&form, "title"),
        "subtitle": format!("{}{} · by {}", state(&form, &now), if flag(&form, "anonymous") { " · answers are anonymous" } else { "" }, name_of(&people, s(&form, "owner"))),
        "description": s(&form, "description"),
    }))
}

#[derive(Deserialize)]
pub struct NewForm {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    anonymous: bool,
    #[serde(default = "yes")]
    once: bool,
    #[serde(default)]
    closes_at: Option<String>,
    #[serde(default)]
    thanks: Option<String>,
}

fn yes() -> bool {
    true
}

fn closing(value: &Option<String>) -> Result<Option<String>> {
    let Some(v) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) else { return Ok(None) };
    let moment = aether_sdk::dates::parse_datetime(v)
        .or_else(|_| aether_sdk::dates::parse_date(v).map(|d| d.and_hms_opt(23, 59, 59).unwrap_or_default()))
        .or_else(|_| chrono_like(v))
        .map_err(|_| Error::msg("the closing time is a date like 2026-12-31"))?;
    Ok(Some(aether_sdk::dates::format_datetime(moment)))
}

fn chrono_like(v: &str) -> Result<aether_sdk::dates::NaiveDateTime> {
    for format in ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M:%S"] {
        if let Ok(m) = aether_sdk::dates::NaiveDateTime::parse_from_str(v, format) {
            return Ok(m);
        }
    }
    Err(Error::msg("not a time"))
}

pub fn create_form(input: NewForm) -> Result<Value> {
    let me = me()?;
    if db::find::<Record>("frm_form").filter("owner", me.as_str()).limit(MAX_FORMS as u32 + 1).all()?.len() >= MAX_FORMS {
        return Err(Error::msg(format!("you can have at most {MAX_FORMS} forms")));
    }
    let form: Record = db::create(
        "frm_form",
        &json!({
            "owner": me, "title": clean("a title", &input.title, 200)?, "description": input.description.as_deref().map(str::trim), "status": "draft", "anonymous": input.anonymous,
            "once": input.once, "closes_at": closing(&input.closes_at)?, "thanks": input.thanks.as_deref().map(str::trim).map(|t| t.chars().take(300).collect::<String>()), "edited_at": now()?,
        }),
    )?;
    Ok(json!({ "id": key(id_text(&form)) }))
}

#[derive(Deserialize)]
pub struct UpdateForm {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    anonymous: Option<bool>,
    #[serde(default)]
    once: Option<bool>,
    #[serde(default)]
    closes_at: Option<String>,
    #[serde(default)]
    thanks: Option<String>,
}

pub fn update_form(input: UpdateForm) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    let has_answers = !responses_of(id_text(&form))?.is_empty();
    let mut data = json!({ "edited_at": now()? });
    if let Some(t) = input.title {
        data["title"] = json!(clean("a title", &t, 200)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim());
    }
    if let Some(a) = input.anonymous {
        if a != flag(&form, "anonymous") && has_answers {
            return Err(Error::msg("a form with answers cannot change between named and anonymous"));
        }
        data["anonymous"] = json!(a);
    }
    if let Some(o) = input.once {
        data["once"] = json!(o);
    }
    if input.closes_at.is_some() {
        data["closes_at"] = json!(closing(&input.closes_at)?);
    }
    if let Some(t) = input.thanks {
        data["thanks"] = json!(t.trim().chars().take(300).collect::<String>());
    }
    db::update::<Record>("frm_form", key(id_text(&form)), &data)?;
    Ok(json!({ "ok": true }))
}

pub fn set_status(input: FormOnly, status: &str) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    if status == "open" {
        let qs = questions_of(id_text(&form))?;
        if qs.is_empty() {
            return Err(Error::msg("add at least one question before opening the form"));
        }
    }
    if status == "draft" && !responses_of(id_text(&form))?.is_empty() {
        return Err(Error::msg("a form with answers cannot go back to a draft: close it instead"));
    }
    db::update::<Record>("frm_form", key(id_text(&form)), &json!({ "status": status, "edited_at": now()? }))?;
    Ok(json!({ "status": status }))
}

pub fn delete_form(input: FormOnly) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    let id = key(id_text(&form)).to_string();
    for q in questions_of(&id)? {
        db::delete::<Record>("frm_question", key(id_text(&q)))?;
    }
    for r in responses_of(&id)? {
        db::delete::<Record>("frm_response", key(id_text(&r)))?;
    }
    for d in db::find::<Record>("frm_done").filter("form", id.as_str()).limit(20000).all()? {
        db::delete::<Record>("frm_done", key(id_text(&d)))?;
    }
    db::delete::<Record>("frm_form", &id)?;
    Ok(json!({ "deleted": true }))
}
