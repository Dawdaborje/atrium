//! Posts, their authors, who has read them, and likes.

use aether_sdk::prelude::*;

use crate::common::*;

pub const ADMIN_ROLE: &str = "org_admin";
const MAX_BODY: usize = 100_000;
const MAX_NOTIFIED: usize = 1000;

fn s<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

fn is_admin() -> Result<bool> {
    Ok(context::current()?.has_role(ADMIN_ROLE))
}

fn author_rows() -> Result<Vec<Record>> {
    db::find("news_author").limit(1000).all()
}

fn can_post(me: &str) -> Result<bool> {
    Ok(is_admin()? || author_rows()?.iter().any(|a| text(a, "user").is_some_and(|u| key(u) == key(me))))
}

fn must_post(me: &str) -> Result<()> {
    if can_post(me)? { Ok(()) } else { Err(Error::msg("only the authors of the news can do this; ask an administrator")) }
}

pub fn news_access() -> Result<Value> {
    let me = me()?;
    Ok(json!({ "can_post": can_post(&me)?, "is_admin": is_admin()? }))
}

fn post_for(id: &str) -> Result<Record> {
    db::get::<Record>("news_post", key(id))?.ok_or_else(|| Error::msg("there is no such post"))
}

fn mine_or_admin(post: &Record, me: &str) -> Result<()> {
    if s(post, "author") != "" && key(s(post, "author")) == key(me) || is_admin()? {
        Ok(())
    } else {
        Err(Error::msg("only the author or an administrator can do this"))
    }
}

/// A draft is seen by its author and by administrators; a published post by everyone.
fn visible(post: &Record, me: &str) -> Result<bool> {
    Ok(s(post, "status") == "published" || key(s(post, "author")) == key(me) || is_admin()?)
}

fn day(t: &str) -> String {
    t.chars().take(10).collect()
}

fn like_rows(post: &str) -> Result<Vec<Record>> {
    db::find("news_like").filter("post", key(post)).limit(5000).all()
}

fn read_rows(post: &str) -> Result<Vec<Record>> {
    db::find("news_read").filter("post", key(post)).limit(5000).all()
}

fn row(post: &Record, people: &[Person], me: &str, read: bool, likes: usize) -> Value {
    json!({
        "id": key(id_text(post)), "title": s(post, "title"), "summary": text(post, "summary"), "category": text(post, "category"),
        "author": name_of(people, s(post, "author")), "when": day(text(post, "published_at").unwrap_or(s(post, "edited_at"))),
        "pinned": flag(post, "pinned"), "tag": if s(post, "status") == "draft" { "Draft" } else if flag(post, "pinned") { "Pinned" } else if !read && key(s(post, "author")) != key(me) { "New" } else { "" },
        "status": s(post, "status"), "likes": likes,
    })
}

#[derive(Deserialize, Default)]
pub struct Feed {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

pub fn feed(input: Feed) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let cat = input.category.unwrap_or_default().trim().to_lowercase();
    let posts: Vec<Record> = db::find("news_post").filter("status", "published").limit(2000).all()?;
    let reads: Vec<Record> = db::find("news_read").filter("user", me.as_str()).limit(5000).all()?;
    let likes: Vec<Record> = db::find("news_like").limit(20000).all()?;
    let mut rows: Vec<(bool, String, Value)> = posts
        .iter()
        .filter(|p| needle.is_empty() || [s(p, "title"), s(p, "summary"), s(p, "body")].iter().any(|f| f.to_lowercase().contains(&needle)))
        .filter(|p| cat.is_empty() || s(p, "category").to_lowercase() == cat)
        .map(|p| {
            let read = reads.iter().any(|r| text(r, "post").is_some_and(|k| key(k) == key(id_text(p))));
            let n = likes.iter().filter(|l| text(l, "post").is_some_and(|k| key(k) == key(id_text(p)))).count();
            (flag(p, "pinned"), s(p, "published_at").to_string(), row(p, &people, &me, read, n))
        })
        .collect();
    rows.sort_by(|a, b| (b.0, &b.1).cmp(&(a.0, &a.1)));
    Ok(rows.into_iter().map(|r| r.2).collect())
}

/// What the caller wrote, drafts first.
pub fn my_posts(_input: Empty) -> Result<Vec<Value>> {
    let me = me()?;
    let people = org_people()?;
    let mut posts: Vec<Record> = db::find("news_post").filter("author", me.as_str()).limit(2000).all()?;
    posts.sort_by_key(|p| (s(p, "status") != "draft", std::cmp::Reverse(s(p, "edited_at").to_string())));
    Ok(posts.iter().map(|p| row(p, &people, &me, true, 0)).collect())
}

pub fn category_options() -> Result<Vec<Value>> {
    me()?;
    let posts: Vec<Record> = db::find("news_post").filter("status", "published").limit(2000).all()?;
    let mut names: Vec<String> = Vec::new();
    for p in &posts {
        let c = s(p, "category").trim();
        if !c.is_empty() && !names.iter().any(|n| n.eq_ignore_ascii_case(c)) {
            names.push(c.to_string());
        }
    }
    names.sort_by_key(|n| n.to_lowercase());
    Ok(names.into_iter().map(|n| json!({ "id": n, "name": n })).collect())
}

#[derive(Deserialize)]
pub struct PostOnly {
    pub id: String,
}

pub fn get_post(input: PostOnly) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    let id = key(id_text(&post)).to_string();
    let people = org_people()?;
    let published = s(&post, "status") == "published";
    let by_me = key(s(&post, "author")) == key(&me);
    if published && !by_me && !read_rows(&id)?.iter().any(|r| text(r, "user").is_some_and(|u| key(u) == key(&me))) {
        db::create::<Record>("news_read", &json!({ "post": id, "user": me }))?;
    }
    let likes = like_rows(&id)?;
    let can_edit = by_me || is_admin()?;
    let header = format!("{} · {}{}", name_of(&people, s(&post, "author")), if published { day(s(&post, "published_at")) } else { "draft".to_string() }, if s(&post, "category").is_empty() { String::new() } else { format!(" · {}", s(&post, "category")) });
    let rendered = format!("*{}*\n\n{}{}", header, if s(&post, "summary").is_empty() { String::new() } else { format!("**{}**\n\n", s(&post, "summary")) }, s(&post, "body"));
    Ok(json!({
        "id": id, "title": s(&post, "title"), "summary": s(&post, "summary"), "body": s(&post, "body"), "category": s(&post, "category"), "status": s(&post, "status"),
        "pinned": flag(&post, "pinned"), "author": name_of(&people, s(&post, "author")), "published_at": text(&post, "published_at"), "rendered": rendered,
        "likes": likes.len(), "liked": likes.iter().any(|l| text(l, "user").is_some_and(|u| key(u) == key(&me))), "can_edit": can_edit,
        "readers": if can_edit && published { Some(read_rows(&id)?.len()) } else { None },
        "reactions": format!("{} like{}", likes.len(), if likes.len() == 1 { "" } else { "s" }),
    }))
}

fn check_text(body: &str) -> Result<()> {
    if body.chars().count() > MAX_BODY {
        return Err(Error::msg(format!("a post is at most {MAX_BODY} characters")));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct NewPost {
    title: String,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    pinned: bool,
    /// Publish at once instead of keeping a draft.
    #[serde(default)]
    publish: bool,
}

pub fn create_post(input: NewPost) -> Result<Value> {
    let me = me()?;
    must_post(&me)?;
    let body = input.body.unwrap_or_default();
    check_text(&body)?;
    let post: Record = db::create(
        "news_post",
        &json!({
            "title": clean("a title", &input.title, 200)?, "summary": input.summary.as_deref().map(str::trim).map(|t| t.chars().take(400).collect::<String>()), "body": body,
            "category": input.category.as_deref().map(str::trim).filter(|c| !c.is_empty()), "author": me, "status": "draft", "pinned": input.pinned, "edited_at": now()?,
        }),
    )?;
    let id = key(id_text(&post)).to_string();
    if input.publish {
        announce(&post, true)?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct UpdatePost {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    pinned: Option<bool>,
}

pub fn update_post(input: UpdatePost) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    mine_or_admin(&post, &me)?;
    let mut data = json!({ "edited_at": now()? });
    if let Some(t) = input.title {
        data["title"] = json!(clean("a title", &t, 200)?);
    }
    if let Some(t) = input.summary {
        data["summary"] = json!(t.trim().chars().take(400).collect::<String>());
    }
    if let Some(b) = input.body {
        check_text(&b)?;
        data["body"] = json!(b);
    }
    if let Some(c) = input.category {
        data["category"] = json!(c.trim());
    }
    if let Some(p) = input.pinned {
        data["pinned"] = json!(p);
    }
    db::update::<Record>("news_post", key(id_text(&post)), &data)?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize)]
pub struct Publish {
    id: String,
    #[serde(default = "yes")]
    notify: bool,
}

fn yes() -> bool {
    true
}

fn announce(post: &Record, notify: bool) -> Result<()> {
    let me = me()?;
    db::update::<Record>("news_post", key(id_text(post)), &json!({ "status": "published", "published_at": now()?, "edited_at": now()? }))?;
    if notify {
        let people = org_people()?;
        let everyone: Vec<String> = people.iter().filter(|p| key(&p.id) != key(&me)).take(MAX_NOTIFIED).map(|p| p.id.clone()).collect();
        if !everyone.is_empty() {
            Notification::new(format!("News: {}", s(post, "title"))).link(format!("/news/p/{}", key(id_text(post)))).to(Audience::Actors(everyone)).send()?;
        }
    }
    Ok(())
}

pub fn publish_post(input: Publish) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    mine_or_admin(&post, &me)?;
    must_post(&me)?;
    if s(&post, "status") == "published" {
        return Err(Error::msg("this post is already published"));
    }
    if s(&post, "body").trim().is_empty() && s(&post, "summary").trim().is_empty() {
        return Err(Error::msg("write something before publishing"));
    }
    announce(&post, input.notify)?;
    Ok(json!({ "ok": true }))
}

pub fn unpublish_post(input: PostOnly) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    mine_or_admin(&post, &me)?;
    db::update::<Record>("news_post", key(id_text(&post)), &json!({ "status": "draft", "edited_at": now()? }))?;
    Ok(json!({ "ok": true }))
}

pub fn delete_post(input: PostOnly) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    mine_or_admin(&post, &me)?;
    let id = key(id_text(&post)).to_string();
    for r in read_rows(&id)? {
        db::delete::<Record>("news_read", key(id_text(&r)))?;
    }
    for l in like_rows(&id)? {
        db::delete::<Record>("news_like", key(id_text(&l)))?;
    }
    db::delete::<Record>("news_post", &id)?;
    Ok(json!({ "deleted": true }))
}

#[derive(Deserialize)]
pub struct Pin {
    id: String,
    pinned: bool,
}

pub fn pin_post(input: Pin) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if !visible(&post, &me)? {
        return Err(Error::msg("there is no such post"));
    }
    mine_or_admin(&post, &me)?;
    db::update::<Record>("news_post", key(id_text(&post)), &json!({ "pinned": input.pinned }))?;
    Ok(json!({ "ok": true }))
}

pub fn toggle_like(input: PostOnly) -> Result<Value> {
    let me = me()?;
    let post = post_for(&input.id)?;
    if s(&post, "status") != "published" {
        return Err(Error::msg("there is no such post"));
    }
    let id = key(id_text(&post)).to_string();
    let likes = like_rows(&id)?;
    let liked = match likes.iter().find(|l| text(l, "user").is_some_and(|u| key(u) == key(&me))) {
        Some(l) => {
            db::delete::<Record>("news_like", key(id_text(l)))?;
            false
        }
        None => {
            db::create::<Record>("news_like", &json!({ "post": id, "user": me }))?;
            true
        }
    };
    Ok(json!({ "liked": liked, "likes": like_rows(&id)?.len() }))
}

/// Who has opened a published post: for its author and administrators.
pub fn post_readers(input: PostOnly) -> Result<Vec<Value>> {
    let me = me()?;
    let post = post_for(&input.id)?;
    mine_or_admin(&post, &me)?;
    let people = org_people()?;
    let mut names: Vec<Value> = read_rows(key(id_text(&post)))?.iter().map(|r| json!({ "id": s(r, "user"), "name": name_of(&people, s(r, "user")) })).collect();
    names.sort_by_key(|n| n["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(names)
}

// ---- authors ------------------------------------------------------------------------------------------------

pub fn authors() -> Result<Vec<Value>> {
    me()?;
    let people = org_people()?;
    let mut rows: Vec<Value> = author_rows()?.iter().map(|a| json!({ "id": s(a, "user"), "name": name_of(&people, s(a, "user")) })).collect();
    rows.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(rows)
}

#[derive(Deserialize)]
pub struct AddAuthors {
    users: Vec<String>,
}

pub fn add_authors(input: AddAuthors) -> Result<Value> {
    context::current()?.require_role(ADMIN_ROLE)?;
    let people = org_people()?;
    let existing = author_rows()?;
    let mut added = 0;
    for u in &input.users {
        let full = people.iter().find(|p| key(&p.id) == key(u)).map(|p| p.id.clone()).ok_or_else(|| Error::msg("that person is not a member of this organization"))?;
        if !existing.iter().any(|a| text(a, "user").is_some_and(|x| key(x) == key(&full))) {
            db::create::<Record>("news_author", &json!({ "user": full }))?;
            added += 1;
        }
    }
    Ok(json!({ "added": added }))
}

pub fn remove_author(input: PostOnly) -> Result<Value> {
    context::current()?.require_role(ADMIN_ROLE)?;
    let rows = author_rows()?;
    let row = rows.iter().find(|a| text(a, "user").is_some_and(|u| key(u) == key(&input.id))).ok_or_else(|| Error::msg("that person is not an author"))?;
    db::delete::<Record>("news_author", key(id_text(row)))?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize, Default)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
}

pub fn pick_people(input: Pick) -> Result<Vec<Value>> {
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?.into_iter().filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle)).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}
