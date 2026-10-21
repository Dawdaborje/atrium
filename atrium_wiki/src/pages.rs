//! Pages: a tree of Markdown pages in a space, links between them, their past, and search.

use std::collections::HashMap;

use aether_sdk::{history, prelude::*};

use crate::access::*;
use crate::common::*;
use crate::spaces::{pages_of, SpaceOnly};

const MAX_BODY: usize = 200_000;
const MAX_DEPTH: usize = 12;

fn str_of<'a>(page: &'a Record, field: &str) -> &'a str {
    text(page, field).unwrap_or_default()
}

/// The page and the caller's role in its space, at least `min`.
pub fn page_for(id: &str, min: Role) -> Result<(Record, Record, Role, String)> {
    let page = db::get::<Record>("wiki_page", key(id))?.ok_or_else(|| Error::msg("there is no such page"))?;
    let (space, role, me) = require(str_of(&page, "space"), min).map_err(|e| if min == Role::Reader { Error::msg("there is no such page") } else { e })?;
    Ok((page, space, role, me))
}

/// `[[Title]]` and `[[Title|words]]` become links to the page of that title; names with no page stay as they were typed.
pub fn link_pages(body: &str, pages: &[(String, String)]) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("]]") {
            Some(end) if !after[..end].contains('\n') && !after[..end].is_empty() => {
                let inner = &after[..end];
                let (title, label) = inner.split_once('|').map(|(t, l)| (t.trim(), l.trim())).unwrap_or((inner.trim(), inner.trim()));
                match pages.iter().find(|(t, _)| t.eq_ignore_ascii_case(title)) {
                    Some((_, id)) => out.push_str(&format!("[{label}](/wiki/p/{id})")),
                    None => out.push_str(&rest[start..start + 2 + end + 2]),
                }
                rest = &after[end + 2..];
            }
            _ => {
                out.push_str("[[");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn titles(space: &str) -> Result<Vec<(String, String)>> {
    Ok(pages_of(space)?.iter().map(|p| (str_of(p, "title").to_string(), key(id_text(p)).to_string())).collect())
}

/// The ids from a page up to the top of the tree, nearest first.
fn ancestors(page: &Record, by_id: &HashMap<String, &Record>) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = str_of(page, "parent").to_string();
    while !at.is_empty() && out.len() < 64 && !out.contains(&at) {
        out.push(at.clone());
        at = by_id.get(&at).map(|p| str_of(p, "parent").to_string()).unwrap_or_default();
    }
    out
}

/// A space's pages in reading order: each page followed by its subpages.
pub fn space_pages(input: SpaceOnly) -> Result<Vec<Value>> {
    let (space, role, _) = require(&input.space, Role::Reader)?;
    let pages = pages_of(id_text(&space))?;
    let people = org_people()?;
    let known: Vec<String> = pages.iter().map(|p| key(id_text(p)).to_string()).collect();
    let mut kids: HashMap<String, Vec<&Record>> = HashMap::new();
    for p in &pages {
        let parent = key(str_of(p, "parent"));
        let top = if known.iter().any(|k| k == parent) { parent.to_string() } else { String::new() };
        kids.entry(top).or_default().push(p);
    }
    for v in kids.values_mut() {
        v.sort_by_key(|p| str_of(p, "title").to_lowercase());
    }
    let mut out = Vec::new();
    let mut stack: Vec<(&Record, usize)> = kids.get("").map(|v| v.iter().rev().map(|p| (*p, 0)).collect()).unwrap_or_default();
    while let Some((page, depth)) = stack.pop() {
        let id = key(id_text(page)).to_string();
        let edited = text(page, "edited_at").map(|t| t.chars().take(10).collect::<String>()).unwrap_or_default();
        out.push(json!({
            "id": id, "title": str_of(page, "title"), "tree_title": format!("{}{}", "– ".repeat(depth), str_of(page, "title")), "depth": depth,
            "edited": edited, "by": name_of(&people, str_of(page, "updated_by")), "role": role_name(role),
        }));
        if depth < MAX_DEPTH {
            if let Some(children) = kids.get(&id) {
                stack.extend(children.iter().rev().map(|c| (*c, depth + 1)));
            }
        }
    }
    Ok(out)
}

#[derive(Deserialize)]
pub struct PageOnly {
    pub id: String,
}

pub fn get_page(input: PageOnly) -> Result<Value> {
    let (page, space, role, _) = page_for(&input.id, Role::Reader)?;
    let people = org_people()?;
    let sid = key(id_text(&space)).to_string();
    let all = pages_of(&sid)?;
    let by_id: HashMap<String, &Record> = all.iter().map(|p| (key(id_text(p)).to_string(), p)).collect();
    let mut path: Vec<String> = ancestors(&page, &by_id).iter().rev().filter_map(|a| by_id.get(a).map(|p| str_of(p, "title").to_string())).collect();
    path.insert(0, str_of(&space, "name").to_string());
    let mut subpages: Vec<&Record> = all.iter().filter(|p| key(str_of(p, "parent")) == key(id_text(&page))).collect();
    subpages.sort_by_key(|p| str_of(p, "title").to_lowercase());
    let links = titles(&sid)?;
    let mut rendered = link_pages(str_of(&page, "body"), &links);
    if !subpages.is_empty() {
        rendered.push_str("\n\n---\n\n**Pages under this one**\n\n");
        for p in &subpages {
            rendered.push_str(&format!("- [{}](/wiki/p/{})\n", str_of(p, "title"), key(id_text(p))));
        }
    }
    let backlinks: Vec<Value> = all
        .iter()
        .filter(|p| key(id_text(p)) != key(id_text(&page)) && str_of(p, "body").to_lowercase().contains(&format!("[[{}", str_of(&page, "title").to_lowercase())))
        .map(|p| json!({ "id": key(id_text(p)), "title": str_of(p, "title") }))
        .collect();
    Ok(json!({
        "id": key(id_text(&page)), "space": sid, "space_name": str_of(&space, "name"), "title": str_of(&page, "title"), "body": str_of(&page, "body"),
        "parent": text(&page, "parent").filter(|p| !p.is_empty()).map(key), "rendered": rendered, "path": path.join(" › "),
        "updated_by_name": name_of(&people, str_of(&page, "updated_by")), "edited_at": text(&page, "edited_at"), "role": role_name(role), "can_edit": role >= Role::Editor,
        "linked_from": backlinks.iter().filter_map(|b| b["title"].as_str()).collect::<Vec<_>>().join(", "),
    }))
}

pub fn page_header(input: PageOnly) -> Result<Value> {
    let page = get_page(input)?;
    Ok(json!({ "title": page["title"], "subtitle": format!("{} · edited {} by {}", page["path"].as_str().unwrap_or_default(), page["edited_at"].as_str().map(|t| t.chars().take(10).collect::<String>()).unwrap_or_default(), page["updated_by_name"].as_str().unwrap_or_default()), "space": page["space"], "can_edit": page["can_edit"] }))
}

#[derive(Deserialize)]
pub struct NewPage {
    space: String,
    title: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    parent: Option<String>,
}

fn check_title(space: &str, title: &str, except: Option<&str>) -> Result<String> {
    let title = clean("a title", title, 200)?;
    if title.contains(['[', ']', '|']) {
        return Err(Error::msg("a title cannot hold [ ] or |"));
    }
    if pages_of(space)?.iter().any(|p| str_of(p, "title").eq_ignore_ascii_case(&title) && !except.is_some_and(|e| key(e) == key(id_text(p)))) {
        return Err(Error::msg("another page of this space has that title"));
    }
    Ok(title)
}

fn check_body(body: &str) -> Result<()> {
    if body.chars().count() > MAX_BODY {
        return Err(Error::msg(format!("a page is at most {MAX_BODY} characters")));
    }
    Ok(())
}

fn check_parent(space: &str, parent: &str, page: Option<&str>) -> Result<Option<String>> {
    let parent = key(parent.trim());
    if parent.is_empty() {
        return Ok(None);
    }
    let all = pages_of(space)?;
    let by_id: HashMap<String, &Record> = all.iter().map(|p| (key(id_text(p)).to_string(), p)).collect();
    let target = by_id.get(parent).ok_or_else(|| Error::msg("that page is not in this space"))?;
    if let Some(me) = page {
        if key(me) == parent || ancestors(target, &by_id).iter().any(|a| key(a) == key(me)) {
            return Err(Error::msg("a page cannot go under itself"));
        }
    }
    Ok(Some(parent.to_string()))
}

pub fn create_page(input: NewPage) -> Result<Value> {
    let (space, _, me) = require(&input.space, Role::Editor)?;
    let sid = key(id_text(&space)).to_string();
    let title = check_title(&sid, &input.title, None)?;
    let body = input.body.unwrap_or_default();
    check_body(&body)?;
    let parent = check_parent(&sid, input.parent.as_deref().unwrap_or(""), None)?;
    let page: Record = db::create("wiki_page", &json!({ "space": sid, "parent": parent, "title": title, "body": body, "updated_by": me, "edited_at": now()? }))?;
    Ok(json!({ "id": key(id_text(&page)) }))
}

#[derive(Deserialize)]
pub struct UpdatePage {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    parent: Option<String>,
}

pub fn update_page(input: UpdatePage) -> Result<Value> {
    let (page, space, _, me) = page_for(&input.id, Role::Editor)?;
    let sid = key(id_text(&space)).to_string();
    let mut data = json!({ "updated_by": me, "edited_at": now()? });
    if let Some(t) = &input.title {
        data["title"] = json!(check_title(&sid, t, Some(id_text(&page)))?);
    }
    if let Some(b) = &input.body {
        check_body(b)?;
        data["body"] = json!(b);
    }
    if let Some(p) = &input.parent {
        data["parent"] = json!(check_parent(&sid, p, Some(id_text(&page)))?);
    }
    db::update::<Record>("wiki_page", key(id_text(&page)), &data)?;
    Ok(json!({ "ok": true }))
}

/// Deleting a page lifts its subpages one level.
pub fn delete_page(input: PageOnly) -> Result<Value> {
    let (page, space, _, _) = page_for(&input.id, Role::Editor)?;
    let id = key(id_text(&page)).to_string();
    let up = text(&page, "parent").filter(|p| !p.is_empty()).map(key).map(str::to_string);
    for child in pages_of(id_text(&space))?.iter().filter(|p| key(str_of(p, "parent")) == id) {
        db::update::<Record>("wiki_page", key(id_text(child)), &json!({ "parent": up }))?;
    }
    db::delete::<Record>("wiki_page", &id)?;
    Ok(json!({ "deleted": true }))
}

/// Pages to pick as a parent: the space's pages except the page itself and what is under it.
#[derive(Deserialize, Default)]
pub struct PickParent {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    space: Option<String>,
    #[serde(default)]
    q: Option<String>,
}

pub fn pick_pages(input: PickParent) -> Result<Vec<Value>> {
    let space = match (&input.space, &input.id) {
        (Some(s), _) => s.clone(),
        (None, Some(id)) => key(str_of(&db::get::<Record>("wiki_page", key(id))?.ok_or_else(|| Error::msg("there is no such page"))?, "space")).to_string(),
        _ => return Ok(Vec::new()),
    };
    let (space, _, _) = require(&space, Role::Reader)?;
    let all = pages_of(id_text(&space))?;
    let by_id: HashMap<String, &Record> = all.iter().map(|p| (key(id_text(p)).to_string(), p)).collect();
    let needle = input.q.unwrap_or_default().to_lowercase();
    let mut out: Vec<Value> = all
        .iter()
        .filter(|p| input.id.as_deref().map_or(true, |me| key(me) != key(id_text(p)) && !ancestors(p, &by_id).iter().any(|a| key(a) == key(me))))
        .filter(|p| needle.is_empty() || str_of(p, "title").to_lowercase().contains(&needle))
        .map(|p| json!({ "id": key(id_text(p)), "name": str_of(p, "title") }))
        .collect();
    out.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    out.truncate(200);
    Ok(out)
}

// ---- history ----------------------------------------------------------------------------------------------

pub fn page_history(input: PageOnly) -> Result<Vec<Value>> {
    let (page, _, _, _) = page_for(&input.id, Role::Reader)?;
    let people = org_people()?;
    let id = key(id_text(&page)).to_string();
    Ok(history::list("wiki_page", &id, None)?
        .iter()
        .map(|v| {
            let seq = v["seq"].as_u64().unwrap_or(0);
            let fields: Vec<&str> = v["fields"].as_array().map(|a| a.iter().filter_map(Value::as_str).filter(|f| !matches!(*f, "updated_by" | "edited_at")).collect()).unwrap_or_default();
            json!({
                "id": format!("{id}|{seq}"), "seq": seq, "op": v["op"], "when": v["at"].as_str().map(|t| t.chars().take(16).collect::<String>().replace('T', " ")),
                "who": name_of(&people, v["actor"].as_str().unwrap_or_default()), "changed": fields.join(", "),
            })
        })
        .collect())
}

#[derive(Deserialize)]
pub struct Restore {
    id: String,
}

/// Puts the page back to how it was after a version (a new version; nothing is lost).
pub fn restore_version(input: Restore) -> Result<Value> {
    let (id, seq) = input.id.split_once('|').ok_or_else(|| Error::msg("choose a version"))?;
    let (page, _, _, me) = page_for(id, Role::Editor)?;
    let seq: u64 = seq.parse().map_err(|_| Error::msg("choose a version"))?;
    let out = history::restore("wiki_page", key(id_text(&page)), seq)?;
    db::update::<Record>("wiki_page", key(id_text(&page)), &json!({ "updated_by": me, "edited_at": now()? }))?;
    Ok(json!({ "restored": out.get("restored").cloned().unwrap_or(Value::Null) }))
}

// ---- search and recent ------------------------------------------------------------------------------------

fn snippet(body: &str, needle: &str) -> String {
    let lower = body.to_lowercase();
    let Some(at) = lower.find(needle) else { return body.chars().take(120).collect::<String>().replace('\n', " ") };
    let start = body[..at].char_indices().rev().nth(40).map(|(i, _)| i).unwrap_or(0);
    let tail: String = body[start..].chars().take(160).collect();
    tail.replace('\n', " ")
}

#[derive(Deserialize, Default)]
pub struct Query {
    #[serde(default)]
    pub q: Option<String>,
}

pub fn search_pages(input: Query) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    if needle.chars().count() < 2 {
        return Ok(Vec::new());
    }
    let mut hits: Vec<(u8, String, Value)> = Vec::new();
    for sid in my_space_keys(&me)? {
        let Some(space) = db::get::<Record>("wiki_space", &sid)? else { continue };
        for p in pages_of(&sid)? {
            let (title, body) = (str_of(&p, "title"), str_of(&p, "body"));
            let rank = if title.to_lowercase() == needle { 0 } else if title.to_lowercase().contains(&needle) { 1 } else if body.to_lowercase().contains(&needle) { 2 } else { continue };
            hits.push((rank, title.to_lowercase(), json!({ "id": key(id_text(&p)), "title": title, "space_name": str_of(&space, "name"), "found": if rank < 2 { "title" } else { "text" }, "snippet": snippet(body, &needle) })));
        }
    }
    hits.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(hits.into_iter().take(50).map(|h| h.2).collect())
}

pub fn recent(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let mut rows: Vec<(String, Value)> = Vec::new();
    for sid in my_space_keys(&me)? {
        let Some(space) = db::get::<Record>("wiki_space", &sid)? else { continue };
        for p in pages_of(&sid)? {
            let at = str_of(&p, "edited_at").to_string();
            rows.push((at.clone(), json!({ "id": key(id_text(&p)), "title": str_of(&p, "title"), "space_name": str_of(&space, "name"), "edited": at.chars().take(16).collect::<String>().replace('T', " "), "by": name_of(&people, str_of(&p, "updated_by")) })));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    Ok(rows.into_iter().take(30).map(|r| r.1).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages() -> Vec<(String, String)> {
        vec![("Onboarding".into(), "p1".into()), ("Expense policy".into(), "p2".into())]
    }

    #[test]
    fn titles_in_double_brackets_become_links() {
        assert_eq!(link_pages("See [[Onboarding]] and [[expense policy|the policy]].", &pages()), "See [Onboarding](/wiki/p/p1) and [the policy](/wiki/p/p2).");
    }

    #[test]
    fn unknown_titles_and_stray_brackets_stay_as_typed() {
        assert_eq!(link_pages("[[Nothing]] and [[ open", &pages()), "[[Nothing]] and [[ open");
        assert_eq!(link_pages("a [[]] b", &pages()), "a [[]] b");
        assert_eq!(link_pages("plain", &pages()), "plain");
    }

    #[test]
    fn a_snippet_is_taken_around_the_match() {
        let body = format!("{} the needle is here {}", "x".repeat(100), "y".repeat(200));
        let s = snippet(&body, "needle");
        assert!(s.contains("needle") && s.chars().count() <= 160);
        assert_eq!(snippet("short text", "zzz"), "short text");
    }
}
