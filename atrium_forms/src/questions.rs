//! The questions of a form.

use aether_sdk::prelude::*;

use crate::common::*;
use crate::forms::*;

pub const KINDS: [&str; 9] = ["text", "long", "choice", "multi", "dropdown", "rating", "yes_no", "number", "date"];
const MAX_QUESTIONS: usize = 100;
const MAX_OPTIONS: usize = 50;

pub fn has_options(kind: &str) -> bool {
    matches!(kind, "choice" | "multi" | "dropdown")
}

/// The choices typed one to a line (or already a list).
pub fn options_of(value: &Value) -> Vec<String> {
    let raw: Vec<String> = match value {
        Value::Array(a) => a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        Value::String(t) => t.lines().map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    for o in raw.iter().map(|o| o.trim()).filter(|o| !o.is_empty()) {
        if !out.iter().any(|x| x.eq_ignore_ascii_case(o)) {
            out.push(o.chars().take(100).collect());
        }
    }
    out
}

fn check_kind(kind: &str, options: &[String]) -> Result<()> {
    if !KINDS.contains(&kind) {
        return Err(Error::msg(format!("a kind of question is one of: {}", KINDS.join(", "))));
    }
    if has_options(kind) && options.len() < 2 {
        return Err(Error::msg("a question with choices needs at least two, one to a line"));
    }
    if options.len() > MAX_OPTIONS {
        return Err(Error::msg(format!("a question has at most {MAX_OPTIONS} choices")));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct QuestionOnly {
    pub id: String,
}

fn question_for(id: &str) -> Result<(Record, Record)> {
    let q = db::get::<Record>("frm_question", key(id))?.ok_or_else(|| Error::msg("there is no such question"))?;
    let (form, _) = owned(s(&q, "form"))?;
    Ok((q, form))
}

fn row(q: &Record, n: usize) -> Value {
    let options = options_of(q.get("options").unwrap_or(&Value::Null));
    json!({
        "id": key(id_text(q)), "position": n, "label": s(q, "label"), "kind": s(q, "kind"), "required": flag(q, "required"), "help": text(q, "help"),
        "options": options.join("\n"), "options_text": options.join(" · "), "number": format!("{n}."),
    })
}

pub fn form_questions(input: FormOnly) -> Result<Vec<Value>> {
    let (form, _) = owned(&input.id)?;
    Ok(questions_of(id_text(&form))?.iter().enumerate().map(|(i, q)| row(q, i + 1)).collect())
}

pub fn get_question(input: QuestionOnly) -> Result<Value> {
    let (q, _) = question_for(&input.id)?;
    Ok(row(&q, 0))
}

#[derive(Deserialize)]
pub struct NewQuestion {
    #[serde(alias = "id")]
    form: String,
    label: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    options: Option<Value>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    help: Option<String>,
}

pub fn add_question(input: NewQuestion) -> Result<Value> {
    let (form, _) = owned(&input.form)?;
    if !responses_of(id_text(&form))?.is_empty() {
        return Err(Error::msg("a form with answers cannot get new questions"));
    }
    let existing = questions_of(id_text(&form))?;
    if existing.len() >= MAX_QUESTIONS {
        return Err(Error::msg(format!("a form has at most {MAX_QUESTIONS} questions")));
    }
    let kind = input.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).unwrap_or("text").to_string();
    let options = input.options.as_ref().map(options_of).unwrap_or_default();
    check_kind(&kind, &options)?;
    let position = existing.last().and_then(|q| q.get("position").and_then(Value::as_i64)).unwrap_or(0) + 1;
    let q: Record = db::create(
        "frm_question",
        &json!({
            "form": key(id_text(&form)), "position": position, "label": clean("a question", &input.label, 300)?, "kind": kind,
            "options": if has_options(&kind) { json!(options) } else { json!([]) }, "required": input.required, "help": input.help.as_deref().map(str::trim),
        }),
    )?;
    Ok(json!({ "id": key(id_text(&q)) }))
}

#[derive(Deserialize)]
pub struct UpdateQuestion {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    options: Option<Value>,
    #[serde(default)]
    required: Option<bool>,
    #[serde(default)]
    help: Option<String>,
}

pub fn update_question(input: UpdateQuestion) -> Result<Value> {
    let (q, form) = question_for(&input.id)?;
    let answered = !responses_of(id_text(&form))?.is_empty();
    let kind = input.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).unwrap_or(s(&q, "kind")).to_string();
    let options = input.options.as_ref().map(options_of).unwrap_or_else(|| options_of(q.get("options").unwrap_or(&Value::Null)));
    check_kind(&kind, &options)?;
    if answered && (kind != s(&q, "kind") || options != options_of(q.get("options").unwrap_or(&Value::Null))) {
        return Err(Error::msg("a form with answers keeps its kinds and choices; you can still reword the question"));
    }
    let mut data = json!({ "kind": kind, "options": if has_options(&kind) { json!(options) } else { json!([]) } });
    if let Some(l) = input.label {
        data["label"] = json!(clean("a question", &l, 300)?);
    }
    if let Some(r) = input.required {
        data["required"] = json!(r);
    }
    if let Some(h) = input.help {
        data["help"] = json!(h.trim());
    }
    db::update::<Record>("frm_question", key(id_text(&q)), &data)?;
    Ok(json!({ "ok": true }))
}

pub fn delete_question(input: QuestionOnly) -> Result<Value> {
    let (q, form) = question_for(&input.id)?;
    if !responses_of(id_text(&form))?.is_empty() {
        return Err(Error::msg("a form with answers keeps its questions"));
    }
    db::delete::<Record>("frm_question", key(id_text(&q)))?;
    renumber(id_text(&form))?;
    Ok(json!({ "deleted": true }))
}

fn renumber(form: &str) -> Result<()> {
    for (i, q) in questions_of(form)?.iter().enumerate() {
        if q.get("position").and_then(Value::as_i64) != Some(i as i64 + 1) {
            db::update::<Record>("frm_question", key(id_text(q)), &json!({ "position": i as i64 + 1 }))?;
        }
    }
    Ok(())
}

pub fn shift(input: QuestionOnly, by: i64) -> Result<Value> {
    let (q, form) = question_for(&input.id)?;
    renumber(id_text(&form))?;
    let all = questions_of(id_text(&form))?;
    let at = all.iter().position(|x| key(id_text(x)) == key(id_text(&q))).unwrap_or(0) as i64;
    let to = at + by;
    if to < 0 || to >= all.len() as i64 {
        return Ok(json!({ "moved": false }));
    }
    let (a, b) = (&all[at as usize], &all[to as usize]);
    db::update::<Record>("frm_question", key(id_text(a)), &json!({ "position": to + 1 }))?;
    db::update::<Record>("frm_question", key(id_text(b)), &json!({ "position": at + 1 }))?;
    Ok(json!({ "moved": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choices_come_one_to_a_line_without_blanks_or_repeats() {
        assert_eq!(options_of(&json!(" Red\n\nblue \nRED\n")), ["Red", "blue"]);
        assert_eq!(options_of(&json!(["a", "b", "A"])), ["a", "b"]);
        assert!(options_of(&json!(null)).is_empty());
    }

    #[test]
    fn choice_questions_need_two_choices_and_kinds_are_known() {
        assert!(check_kind("choice", &["a".into()]).is_err());
        assert!(check_kind("choice", &["a".into(), "b".into()]).is_ok());
        assert!(check_kind("text", &[]).is_ok());
        assert!(check_kind("essay", &[]).is_err());
    }
}
