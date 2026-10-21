//! Atrium sites: a start page for a team, a department or the whole organization, made of some words, the latest news
//! and links to what the team uses (a wiki space, a drive folder, a plan, a form). See docs/design.md.

mod access;
mod common;
mod sites;

use aether_sdk::prelude::*;

handler! {
    fn my_sites(input: sites::Search) -> Vec<Value> { sites::my_sites(input) }
    fn create_site(input: sites::NewSite) -> Value { sites::create_site(input) }
    fn edit_site(input: sites::EditSite) -> Value { sites::edit_site(input) }
    fn delete_site(input: sites::SiteOnly) -> Value { sites::delete_site(input) }
    fn site_header(input: sites::SiteOnly) -> Value { sites::site_header(input) }
    fn site_info(input: sites::SiteOnly) -> Value { sites::site_info(input) }
    fn view_site(input: sites::SiteOnly) -> Value { sites::view_site(input) }
    // ---- links
    fn site_links(input: sites::SiteOnly) -> Vec<Value> { sites::site_links(input) }
    fn add_link(input: sites::NewLink) -> Value { sites::add_link(input) }
    fn delete_link(input: sites::LinkOnly) -> Value { sites::delete_link(input) }
    fn link_up(input: sites::LinkOnly) -> Value { sites::shift_link(input, -1) }
    fn link_down(input: sites::LinkOnly) -> Value { sites::shift_link(input, 1) }
    // ---- people
    fn site_members(input: sites::SiteOnly) -> Vec<Value> { sites::site_members(input) }
    fn add_site_members(input: sites::AddMembers) -> Value { sites::add_site_members(input) }
    fn remove_site_member(input: sites::RemoveMember) -> Value { sites::remove_site_member(input) }
    fn set_site_role(input: sites::SetRole) -> Value { sites::set_site_role(input) }
    fn pick_other_people(input: sites::Pick) -> Vec<Value> { sites::pick_other_people(input) }
}
