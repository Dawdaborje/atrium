//! Sites and the people in them.

use aether_sdk::prelude::*;

use crate::access::*;
use crate::common::*;

const MAX_MEMBERS: usize = 300;
const MAX_SITES: usize = 100;

pub fn notify(user: &str, title: &str, link: &str) -> Result<()> {
    Notification::new(title.to_string()).link(link.to_string()).to(Audience::Actors(vec![user.to_string()])).send()?;
    Ok(())
}

/// Org members by full id; each one must belong to this organization.
pub fn check_users(users: &[String]) -> Result<Vec<String>> {
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

fn add_member(site: &str, user: &str, role: &str) -> Result<()> {
    if member_row(site, user)?.is_none() {
        db::create::<Record>("site_member", &json!({ "site": key(site), "user": user, "role": role }))?;
    }
    Ok(())
}

pub fn links_of(site: &str) -> Result<Vec<Record>> {
    db::find("site_link").filter("site", key(site)).limit(5000).all()
}

#[derive(Deserialize, Default)]
pub struct Search {
    #[serde(default)]
    pub q: Option<String>,
}

pub fn my_sites(input: Search) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    let mut out = Vec::new();
    for id in my_site_keys(&me)? {
        let Some(s) = db::get::<Record>("site_site", &id)? else { continue };
        let name = text(&s, "name").unwrap_or_default();
        if !needle.is_empty() && !name.to_lowercase().contains(&needle) && !text(&s, "description").unwrap_or_default().to_lowercase().contains(&needle) {
            continue;
        }
        let role = role_of(&id, &me)?;
        out.push(json!({
            "id": id, "name": name, "description": text(&s, "description"), "open": flag(&s, "open"), "role": role.map(role_name).unwrap_or("visitor"),
            "member": role.is_some(), "pages": links_of(&id)?.len(), "people": members_of(&id)?.len(),
        }));
    }
    out.sort_by_key(|r| r["name"].as_str().unwrap_or_default().to_lowercase());
    Ok(out)
}

#[derive(Deserialize)]
pub struct NewSite {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    open: bool,
    #[serde(default)]
    home: Option<String>,
    #[serde(default)]
    show_news: bool,
    #[serde(default)]
    members: Vec<String>,
}

pub fn create_site(input: NewSite) -> Result<Value> {
    let me = me()?;
    let mine = db::find::<Record>("site_member").filter("user", me.as_str()).limit(1000).all()?.len();
    if mine >= MAX_SITES {
        return Err(Error::msg(format!("you can be in at most {MAX_SITES} sites")));
    }
    let name = clean("a name", &input.name, 120)?;
    let members = check_users(&input.members)?;
    let site: Record = db::create("site_site", &json!({ "name": name, "description": input.description.as_deref().map(str::trim).map(|d| d.chars().take(400).collect::<String>()), "home": check_home(input.home.as_deref())?, "owner": me, "open": input.open, "show_news": input.show_news, "edited_at": now()? }))?;
    let id = key(id_text(&site)).to_string();
    add_member(&id, &me, "owner")?;
    let people = org_people()?;
    for u in members.iter().filter(|u| key(u) != key(&me)) {
        add_member(&id, u, "editor")?;
        notify(u, &format!("{} added you to the site {name}", name_of(&people, &me)), &format!("/sites/s/{id}"))?;
    }
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
pub struct SiteOnly {
    pub site: String,
}

pub fn site_header(input: SiteOnly) -> Result<Value> {
    let (site, role, _) = require(&input.site, Role::Visitor)?;
    Ok(json!({ "title": text(&site, "name"), "subtitle": format!("{}{}", if flag(&site, "open") { "open to everyone · " } else { "" }, format!("you are {}", role_name(role))), "can_edit": role >= Role::Editor, "can_manage": role == Role::Owner }))
}

pub fn site_info(input: SiteOnly) -> Result<Value> {
    let (site, role, _) = require(&input.site, Role::Visitor)?;
    Ok(json!({ "id": key(id_text(&site)), "name": text(&site, "name"), "description": text(&site, "description"), "home": text(&site, "home"), "open": flag(&site, "open"), "show_news": flag(&site, "show_news"), "role": role_name(role) }))
}

#[derive(Deserialize)]
pub struct EditSite {
    site: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    open: Option<bool>,
    #[serde(default)]
    home: Option<String>,
    #[serde(default)]
    show_news: Option<bool>,
}

const MAX_HOME: usize = 50_000;

pub fn check_home(home: Option<&str>) -> Result<Option<String>> {
    match home.map(str::trim).filter(|h| !h.is_empty()) {
        Some(h) if h.chars().count() > MAX_HOME => Err(Error::msg(format!("the start page is at most {MAX_HOME} characters"))),
        other => Ok(other.map(str::to_string)),
    }
}

/// Editors write the start page and show or hide the news; only an owner renames a site or opens it to everyone.
pub fn edit_site(input: EditSite) -> Result<Value> {
    let (site, role, _) = require(&input.site, Role::Editor)?;
    if (input.name.is_some() || input.open.is_some()) && role < Role::Owner {
        return Err(Error::msg("only an owner of the site can do this"));
    }
    let mut data = json!({ "edited_at": now()? });
    if let Some(h) = &input.home {
        data["home"] = json!(check_home(Some(h))?);
    }
    if let Some(n) = input.show_news {
        data["show_news"] = json!(n);
    }
    if let Some(n) = input.name {
        data["name"] = json!(clean("a name", &n, 120)?);
    }
    if let Some(d) = input.description {
        data["description"] = json!(d.trim().chars().take(400).collect::<String>());
    }
    if let Some(o) = input.open {
        data["open"] = json!(o);
    }
    db::update::<Record>("site_site", key(id_text(&site)), &data)?;
    Ok(json!({ "ok": true }))
}

pub fn delete_site(input: SiteOnly) -> Result<Value> {
    let (site, _, _) = require(&input.site, Role::Owner)?;
    let id = key(id_text(&site)).to_string();
    for page in links_of(&id)? {
        db::delete::<Record>("site_link", key(id_text(&page)))?;
    }
    for row in members_of(&id)? {
        db::delete::<Record>("site_member", key(id_text(&row)))?;
    }
    db::delete::<Record>("site_site", &id)?;
    Ok(json!({ "deleted": true }))
}

// ---- people -----------------------------------------------------------------------------------------------

pub fn site_members(input: SiteOnly) -> Result<Vec<Value>> {
    let (site, _, _) = require(&input.site, Role::Visitor)?;
    let people = org_people()?;
    let mut rows: Vec<Value> = members_of(id_text(&site))?
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
    site: String,
    users: Vec<String>,
    #[serde(default)]
    role: Option<String>,
}

pub fn add_site_members(input: AddMembers) -> Result<Value> {
    let (site, _, me) = require(&input.site, Role::Owner)?;
    let role = parse_role(input.role.as_deref().unwrap_or("editor"))?;
    let users = check_users(&input.users)?;
    let id = key(id_text(&site)).to_string();
    if members_of(&id)?.len() + users.len() > MAX_MEMBERS {
        return Err(Error::msg(format!("a site has at most {MAX_MEMBERS} people")));
    }
    let people = org_people()?;
    let mut added = 0;
    for u in &users {
        if member_row(&id, u)?.is_none() {
            add_member(&id, u, role_name(role))?;
            added += 1;
            notify(u, &format!("{} added you to the site {}", name_of(&people, &me), text(&site, "name").unwrap_or("a site")), &format!("/sites/s/{id}"))?;
        }
    }
    Ok(json!({ "added": added }))
}

#[derive(Deserialize)]
pub struct RemoveMember {
    site: String,
    id: String,
}

fn owners(site: &str) -> Result<usize> {
    Ok(members_of(site)?.iter().filter(|m| text(m, "role") == Some("owner")).count())
}

pub fn remove_site_member(input: RemoveMember) -> Result<Value> {
    let (site, _, me) = require(&input.site, Role::Visitor)?;
    if key(&input.id) != key(&me) {
        require(&input.site, Role::Owner)?;
    }
    let id = key(id_text(&site)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this site"))?;
    if text(&row, "role") == Some("owner") && owners(&id)? <= 1 {
        return Err(Error::msg("a site keeps at least one owner"));
    }
    db::delete::<Record>("site_member", key(id_text(&row)))?;
    Ok(json!({ "removed": true }))
}

#[derive(Deserialize)]
pub struct SetRole {
    site: String,
    id: String,
    role: String,
}

pub fn set_site_role(input: SetRole) -> Result<Value> {
    let (site, _, _) = require(&input.site, Role::Owner)?;
    let role = parse_role(&input.role)?;
    let id = key(id_text(&site)).to_string();
    let row = member_row(&id, &input.id)?.ok_or_else(|| Error::msg("that person is not in this site"))?;
    if text(&row, "role") == Some("owner") && role != Role::Owner && owners(&id)? <= 1 {
        return Err(Error::msg("a site keeps at least one owner"));
    }
    db::update::<Record>("site_member", key(id_text(&row)), &json!({ "role": role_name(role) }))?;
    Ok(json!({ "ok": true }))
}

#[derive(Deserialize, Default)]
pub struct Pick {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    search: Option<String>,
}

pub fn pick_other_people(input: Pick) -> Result<Vec<Value>> {
    let me = me()?;
    let needle = input.q.or(input.search).unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?.into_iter().filter(|p| key(&p.id) != key(&me) && (needle.is_empty() || p.name.to_lowercase().contains(&needle))).take(200).map(|p| json!({ "id": p.id, "name": p.name })).collect())
}

// ---- the start page ------------------------------------------------------------------------------------------

fn s_of<'a>(r: &'a Record, f: &str) -> &'a str {
    text(r, f).unwrap_or_default()
}

pub fn sorted_links(site: &str) -> Result<Vec<Record>> {
    let mut rows = links_of(site)?;
    rows.sort_by_key(|l| l.get("position").and_then(Value::as_i64).unwrap_or(0));
    Ok(rows)
}

/// The latest news, when the news plugin is installed and the site asks for it.
fn news_lines() -> String {
    if !plugins::available("atrium_news").unwrap_or(false) {
        return String::new();
    }
    let rows: Vec<Value> = plugins::call("atrium_news", "feed", &json!({})).unwrap_or_default();
    rows.iter()
        .take(5)
        .map(|r| format!("- [{}](/news/p/{}){}\n", s_of(r, "title"), key(s_of(r, "id")), text(r, "summary").map(|t| format!(" — {t}")).unwrap_or_default()))
        .collect()
}

/// The start page as Markdown: the home text, the news, and the links.
pub fn render(site: &Record, links: &[Record], news: &str) -> String {
    let mut out = String::new();
    if let Some(d) = text(site, "description") {
        out.push_str(&format!("*{d}*\n\n"));
    }
    out.push_str(s_of(site, "home"));
    if !news.is_empty() {
        out.push_str(&format!("\n\n### Latest news\n\n{news}"));
    }
    if !links.is_empty() {
        out.push_str("\n\n### Quick links\n\n");
        for l in links {
            out.push_str(&format!("- [{}]({}){}\n", s_of(l, "title").replace(['[', ']'], ""), s_of(l, "url"), text(l, "about").map(|a| format!(" — {a}")).unwrap_or_default()));
        }
    }
    out
}

pub fn view_site(input: SiteOnly) -> Result<Value> {
    let (site, role, _) = require(&input.site, Role::Visitor)?;
    let id = key(id_text(&site)).to_string();
    let links = sorted_links(&id)?;
    let news = if flag(&site, "show_news") { news_lines() } else { String::new() };
    Ok(json!({ "id": id, "name": s_of(&site, "name"), "page": render(&site, &links, &news), "role": role_name(role), "can_edit": role >= Role::Editor }))
}

// ---- links -----------------------------------------------------------------------------------------------------

const MAX_LINKS: usize = 60;

/// Where a link may lead: a page of this app (one slash), or a web address.
pub fn check_url(url: &str) -> Result<String> {
    let u = url.trim();
    let ok = (u.starts_with('/') && !u.starts_with("//") && !u.contains(['\\', ' ']) && u.len() <= 500)
        || ((u.starts_with("https://") || u.starts_with("http://")) && !u.contains(' ') && u.len() <= 500 && u.len() > 10);
    if ok { Ok(u.to_string()) } else { Err(Error::msg("a link is a page of this app like /wiki, or a web address starting with https://")) }
}

pub fn site_links(input: SiteOnly) -> Result<Vec<Value>> {
    let (site, _, _) = require(&input.site, Role::Visitor)?;
    Ok(sorted_links(id_text(&site))?.iter().enumerate().map(|(i, l)| json!({ "id": key(id_text(l)), "number": format!("{}.", i + 1), "title": s_of(l, "title"), "url": s_of(l, "url"), "about": text(l, "about") })).collect())
}

#[derive(Deserialize)]
pub struct NewLink {
    site: String,
    title: String,
    url: String,
    #[serde(default)]
    about: Option<String>,
}

pub fn add_link(input: NewLink) -> Result<Value> {
    let (site, _, _) = require(&input.site, Role::Editor)?;
    let id = key(id_text(&site)).to_string();
    let existing = sorted_links(&id)?;
    if existing.len() >= MAX_LINKS {
        return Err(Error::msg(format!("a site has at most {MAX_LINKS} links")));
    }
    let position = existing.last().and_then(|l| l.get("position").and_then(Value::as_i64)).unwrap_or(0) + 1;
    let link: Record = db::create(
        "site_link",
        &json!({ "site": id, "position": position, "title": clean("a title", &input.title, 120)?, "url": check_url(&input.url)?, "about": input.about.as_deref().map(str::trim).map(|a| a.chars().take(200).collect::<String>()) }),
    )?;
    Ok(json!({ "id": key(id_text(&link)) }))
}

#[derive(Deserialize)]
pub struct LinkOnly {
    pub id: String,
}

fn link_for(id: &str) -> Result<(Record, Record)> {
    let link = db::get::<Record>("site_link", key(id))?.ok_or_else(|| Error::msg("there is no such link"))?;
    let (site, _, _) = require(s_of(&link, "site"), Role::Editor)?;
    Ok((link, site))
}

pub fn delete_link(input: LinkOnly) -> Result<Value> {
    let (link, site) = link_for(&input.id)?;
    db::delete::<Record>("site_link", key(id_text(&link)))?;
    for (i, l) in sorted_links(id_text(&site))?.iter().enumerate() {
        if l.get("position").and_then(Value::as_i64) != Some(i as i64 + 1) {
            db::update::<Record>("site_link", key(id_text(l)), &json!({ "position": i as i64 + 1 }))?;
        }
    }
    Ok(json!({ "deleted": true }))
}

pub fn shift_link(input: LinkOnly, by: i64) -> Result<Value> {
    let (link, site) = link_for(&input.id)?;
    let all = sorted_links(id_text(&site))?;
    let at = all.iter().position(|x| key(id_text(x)) == key(id_text(&link))).unwrap_or(0) as i64;
    let to = at + by;
    if to < 0 || to >= all.len() as i64 {
        return Ok(json!({ "moved": false }));
    }
    for (i, l) in all.iter().enumerate() {
        let new = if i as i64 == at { to } else if i as i64 == to { at } else { i as i64 };
        db::update::<Record>("site_link", key(id_text(l)), &json!({ "position": new + 1 }))?;
    }
    Ok(json!({ "moved": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_lead_to_this_app_or_the_web_and_nowhere_else() {
        assert!(check_url("/wiki/s/abc").is_ok() && check_url("https://example.org/a").is_ok());
        for bad in ["//evil.example", "javascript:alert(1)", "/a b", "ftp://x.org/f", "wiki", "https://", "/\\evil"] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_start_page_is_markdown_with_news_and_links() {
        let site = json!({ "description": "Sales team", "home": "# Hello" });
        let links = vec![json!({ "title": "Pipeline [x]", "url": "/crm", "about": "deals" })];
        let page = render(&site, &links, "- [News](/news/p/1)\n");
        assert!(page.starts_with("*Sales team*\n\n# Hello") && page.contains("### Latest news") && page.contains("- [Pipeline x](/crm) — deals"), "{page}");
        assert_eq!(render(&json!({}), &[], ""), "");
    }
}
