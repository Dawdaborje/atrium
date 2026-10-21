//! Answering a form, and reading what came in.

use std::collections::HashMap;

use aether_sdk::dates;
use aether_sdk::prelude::*;

use crate::common::*;
use crate::forms::*;
use crate::questions::options_of;

const MAX_SHORT: usize = 500;
const MAX_LONG: usize = 5000;

/// What the widget shows: the questions, or the reason there is nothing to answer.
pub fn questionnaire(input: FormOnly) -> Result<Value> {
    let me = me()?;
    let form = form_for(&input.id)?;
    let now = now()?;
    let mine = key(s(&form, "owner")) == key(&me) || is_admin()?;
    if !mine && state(&form, &now) == "draft" {
        return Err(Error::msg("there is no such form"));
    }
    let qs: Vec<Value> = questions_of(id_text(&form))?
        .iter()
        .map(|q| json!({ "id": key(id_text(q)), "label": s(q, "label"), "kind": s(q, "kind"), "required": flag(q, "required"), "help": text(q, "help"), "options": options_of(q.get("options").unwrap_or(&Value::Null)) }))
        .collect();
    let mut locked: Option<String> = None;
    if !is_open(&form, &now) {
        locked = Some(match state(&form, &now) {
            "draft" => "This form is not open yet: it is a preview. Open it to collect answers.".to_string(),
            _ => "This form is closed.".to_string(),
        });
    } else if flag(&form, "once") && answered(id_text(&form), &me)? {
        locked = Some("You have answered this form. Thank you.".to_string());
    }
    Ok(json!({ "questions": qs, "locked": locked, "intro": text(&form, "description"), "anonymous": flag(&form, "anonymous"), "thanks": text(&form, "thanks").unwrap_or("Thank you for answering.") }))
}

/// One answer, checked against its question: `None` when it is left empty.
pub fn check_answer(kind: &str, options: &[String], required: bool, value: &Value) -> Result<Option<Value>> {
    let empty = match value {
        Value::Null => true,
        Value::String(s) => s.trim().is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    };
    if empty {
        return if required { Err(Error::msg("this question needs an answer")) } else { Ok(None) };
    }
    let text_of = |v: &Value| v.as_str().map(|s| s.trim().to_string());
    match kind {
        "text" | "long" => {
            let t = text_of(value).ok_or_else(|| Error::msg("answer with text"))?;
            let max = if kind == "text" { MAX_SHORT } else { MAX_LONG };
            if t.chars().count() > max {
                return Err(Error::msg(format!("an answer is at most {max} characters")));
            }
            Ok(Some(json!(t)))
        }
        "choice" | "dropdown" => {
            let t = text_of(value).ok_or_else(|| Error::msg("choose one"))?;
            options.iter().find(|o| o.eq_ignore_ascii_case(&t)).map(|o| Some(json!(o))).ok_or_else(|| Error::msg("that is not one of the choices"))
        }
        "multi" => {
            let items: Vec<String> = match value {
                Value::Array(a) => a.iter().filter_map(text_of).collect(),
                Value::String(s) => s.split('\n').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
                _ => return Err(Error::msg("choose from the list")),
            };
            let mut picked: Vec<String> = Vec::new();
            for i in &items {
                let o = options.iter().find(|o| o.eq_ignore_ascii_case(i)).ok_or_else(|| Error::msg("that is not one of the choices"))?;
                if !picked.contains(o) {
                    picked.push(o.clone());
                }
            }
            Ok(Some(json!(picked)))
        }
        "rating" => {
            let n = value.as_i64().or_else(|| value.as_str().and_then(|s| s.trim().parse().ok())).ok_or_else(|| Error::msg("a rating is 1 to 5"))?;
            if !(1..=5).contains(&n) {
                return Err(Error::msg("a rating is 1 to 5"));
            }
            Ok(Some(json!(n)))
        }
        "yes_no" => match text_of(value).map(|t| t.to_lowercase()).as_deref().or(if value.as_bool() == Some(true) { Some("yes") } else if value.as_bool() == Some(false) { Some("no") } else { None }) {
            Some("yes") => Ok(Some(json!("yes"))),
            Some("no") => Ok(Some(json!("no"))),
            _ => Err(Error::msg("answer yes or no")),
        },
        "number" => {
            let n = value.as_f64().or_else(|| value.as_str().and_then(|s| s.trim().replace(',', ".").parse().ok())).ok_or_else(|| Error::msg("answer with a number"))?;
            if !n.is_finite() || n.abs() > 1e12 {
                return Err(Error::msg("that number is too large"));
            }
            Ok(Some(json!(n)))
        }
        "date" => {
            let t = text_of(value).ok_or_else(|| Error::msg("answer with a date"))?;
            dates::parse_date(&t).map_err(|_| Error::msg("a date is like 2026-12-31"))?;
            Ok(Some(json!(t)))
        }
        _ => Err(Error::msg("this question cannot be answered")),
    }
}

#[derive(Deserialize)]
pub struct Submit {
    pub id: String,
    #[serde(default)]
    pub answers: HashMap<String, Value>,
}

pub fn submit_response(input: Submit) -> Result<Value> {
    let me = me()?;
    let form = form_for(&input.id)?;
    let now = now()?;
    if !is_open(&form, &now) {
        return Err(Error::msg(if state(&form, &now) == "draft" { "this form is not open yet" } else { "this form is closed" }));
    }
    let id = key(id_text(&form)).to_string();
    if flag(&form, "once") && answered(&id, &me)? {
        return Err(Error::msg("you have answered this form already"));
    }
    let qs = questions_of(&id)?;
    let mut stored = serde_json::Map::new();
    for q in &qs {
        let k = key(id_text(q)).to_string();
        let value = input.answers.get(&k).unwrap_or(&Value::Null);
        let options = options_of(q.get("options").unwrap_or(&Value::Null));
        match check_answer(s(q, "kind"), &options, flag(q, "required"), value) {
            Ok(Some(v)) => {
                stored.insert(k, v);
            }
            Ok(None) => {}
            Err(e) => return Err(Error::msg(format!("“{}”: {}", s(q, "label"), e))),
        }
    }
    if let Some(unknown) = input.answers.keys().find(|k| !qs.iter().any(|q| key(id_text(q)) == key(k))) {
        return Err(Error::msg(format!("there is no question {unknown}")));
    }
    if stored.is_empty() {
        return Err(Error::msg("answer at least one question"));
    }
    let anonymous = flag(&form, "anonymous");
    db::create::<Record>("frm_response", &json!({ "form": id, "user": if anonymous { Value::Null } else { json!(me) }, "answers": stored, "submitted_at": now }))?;
    db::create::<Record>("frm_done", &json!({ "form": id, "user": me }))?;
    Ok(json!({ "ok": true, "thanks": text(&form, "thanks").unwrap_or("Thank you for answering.") }))
}

// ---- results ------------------------------------------------------------------------------------------------

fn answers_to(responses: &[Record], q: &Record) -> Vec<Value> {
    responses.iter().filter_map(|r| r.get("answers").and_then(|a| a.get(key(id_text(q)))).cloned()).collect()
}

/// A question's answers summed up, as Markdown lines.
pub fn summarize(kind: &str, options: &[String], given: &[Value]) -> String {
    let n = given.len();
    let percent = |c: usize| if n == 0 { 0 } else { (c * 100 + n / 2) / n };
    let mut out = String::new();
    match kind {
        "choice" | "dropdown" | "multi" => {
            for o in options {
                let c = given.iter().filter(|g| g.as_str() == Some(o) || g.as_array().is_some_and(|a| a.iter().any(|x| x.as_str() == Some(o)))).count();
                out.push_str(&format!("- **{o}**: {c} ({}%)\n", percent(c)));
            }
        }
        "yes_no" => {
            for o in ["yes", "no"] {
                let c = given.iter().filter(|g| g.as_str() == Some(o)).count();
                out.push_str(&format!("- **{o}**: {c} ({}%)\n", percent(c)));
            }
        }
        "rating" => {
            let nums: Vec<i64> = given.iter().filter_map(Value::as_i64).collect();
            if !nums.is_empty() {
                out.push_str(&format!("- **Average**: {:.1} of 5\n", nums.iter().sum::<i64>() as f64 / nums.len() as f64));
            }
            for star in (1..=5).rev() {
                let c = nums.iter().filter(|x| **x == star).count();
                out.push_str(&format!("- {star}: {c}\n"));
            }
        }
        "number" => {
            let nums: Vec<f64> = given.iter().filter_map(Value::as_f64).collect();
            if !nums.is_empty() {
                let sum: f64 = nums.iter().sum();
                let (lo, hi) = nums.iter().fold((f64::MAX, f64::MIN), |(l, h), x| (l.min(*x), h.max(*x)));
                out.push_str(&format!("- **Average**: {:.2}\n- **Lowest**: {lo}\n- **Highest**: {hi}\n- **Total**: {sum}\n", sum / nums.len() as f64));
            }
        }
        _ => {
            for g in given.iter().rev().take(20) {
                out.push_str(&format!("- {}\n", g.as_str().unwrap_or_default().replace('\n', " ")));
            }
            if n > 20 {
                out.push_str(&format!("- … and {} more (see the responses)\n", n - 20));
            }
        }
    }
    out
}

pub fn results(input: FormOnly) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    let id = id_text(&form);
    let responses = responses_of(id)?;
    let mut report = format!("**{} response{}**\n\n", responses.len(), if responses.len() == 1 { "" } else { "s" });
    for (i, q) in questions_of(id)?.iter().enumerate() {
        let given = answers_to(&responses, q);
        report.push_str(&format!("### {}. {}\n\n*{} answered*\n\n{}\n", i + 1, s(q, "label"), given.len(), summarize(s(q, "kind"), &options_of(q.get("options").unwrap_or(&Value::Null)), &given)));
    }
    Ok(json!({ "title": s(&form, "title"), "responses": responses.len(), "report": report }))
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join("; "),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

pub fn responses(input: FormOnly) -> Result<Vec<Value>> {
    let (form, _) = owned(&input.id)?;
    let id = id_text(&form);
    let people = org_people()?;
    let qs = questions_of(id)?;
    let mut rows = responses_of(id)?;
    rows.sort_by_key(|r| std::cmp::Reverse(s(r, "submitted_at").to_string()));
    Ok(rows
        .iter()
        .map(|r| {
            let preview: Vec<String> = qs.iter().filter_map(|q| r.get("answers").and_then(|a| a.get(key(id_text(q)))).map(|v| show(v))).take(3).collect();
            json!({
                "id": key(id_text(r)), "who": if text(r, "user").is_some() { name_of(&people, s(r, "user")) } else { "Anonymous".to_string() },
                "when": s(r, "submitted_at").chars().take(16).collect::<String>().replace('T', " "), "preview": preview.join(" · "),
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct ResponseOnly {
    pub id: String,
}

pub fn delete_response(input: ResponseOnly) -> Result<Value> {
    let r = db::get::<Record>("frm_response", key(&input.id))?.ok_or_else(|| Error::msg("there is no such response"))?;
    owned(s(&r, "form"))?;
    db::delete::<Record>("frm_response", key(id_text(&r)))?;
    Ok(json!({ "deleted": true }))
}

fn csv_cell(v: &str) -> String {
    let guard = if v.starts_with(['=', '+', '-', '@']) { format!("'{v}") } else { v.to_string() };
    if guard.contains([',', '"', '\n', '\r']) { format!("\"{}\"", guard.replace('"', "\"\"")) } else { guard }
}

/// The responses as CSV text, one column for each question; a cell that would run as a formula in a spreadsheet is quoted.
pub fn responses_csv(input: FormOnly) -> Result<Value> {
    let (form, _) = owned(&input.id)?;
    let id = id_text(&form);
    let people = org_people()?;
    let qs = questions_of(id)?;
    let mut lines = vec![std::iter::once("Submitted".to_string()).chain(std::iter::once("Who".to_string())).chain(qs.iter().map(|q| csv_cell(s(q, "label")))).collect::<Vec<_>>().join(",")];
    let mut rows = responses_of(id)?;
    rows.sort_by_key(|r| s(r, "submitted_at").to_string());
    for r in &rows {
        let who = if text(r, "user").is_some() { name_of(&people, s(r, "user")) } else { String::new() };
        let mut cells = vec![csv_cell(s(r, "submitted_at")), csv_cell(&who)];
        for q in &qs {
            cells.push(csv_cell(&r.get("answers").and_then(|a| a.get(key(id_text(q)))).map(show).unwrap_or_default()));
        }
        lines.push(cells.join(","));
    }
    Ok(json!({ "id": key(id), "rows": rows.len(), "csv": lines.join("\n") }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Vec<String> {
        vec!["Red".into(), "Blue".into()]
    }

    #[test]
    fn answers_are_checked_against_their_question() {
        assert_eq!(check_answer("choice", &opts(), false, &json!("red")).unwrap(), Some(json!("Red")));
        assert!(check_answer("choice", &opts(), false, &json!("green")).is_err());
        assert_eq!(check_answer("multi", &opts(), false, &json!(["blue", "Red", "BLUE"])).unwrap(), Some(json!(["Blue", "Red"])));
        assert_eq!(check_answer("rating", &[], false, &json!("4")).unwrap(), Some(json!(4)));
        assert!(check_answer("rating", &[], false, &json!(6)).is_err());
        assert_eq!(check_answer("yes_no", &[], false, &json!("Yes")).unwrap(), Some(json!("yes")));
        assert!(check_answer("yes_no", &[], false, &json!("maybe")).is_err());
        assert_eq!(check_answer("number", &[], false, &json!("3,5")).unwrap(), Some(json!(3.5)));
        assert!(check_answer("date", &[], false, &json!("tomorrow")).is_err());
        assert!(check_answer("text", &[], false, &json!("x".repeat(501))).is_err());
    }

    #[test]
    fn empty_answers_are_skipped_unless_required() {
        assert_eq!(check_answer("text", &[], false, &json!("  ")).unwrap(), None);
        assert!(check_answer("text", &[], true, &json!(null)).is_err());
        assert!(check_answer("multi", &opts(), true, &json!([])).is_err());
    }

    #[test]
    fn choices_are_counted_with_percentages() {
        let s = summarize("choice", &opts(), &[json!("Red"), json!("Red"), json!("Blue"), json!("Red")]);
        assert!(s.contains("**Red**: 3 (75%)") && s.contains("**Blue**: 1 (25%)"), "{s}");
        assert!(summarize("rating", &[], &[json!(5), json!(3)]).contains("4.0 of 5"));
    }

    #[test]
    fn formulas_in_a_csv_are_defused() {
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}
