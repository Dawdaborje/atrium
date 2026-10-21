//! Atrium wiki: spaces of Markdown pages in a tree, links between pages with [[Title]], the past of every page,
//! comments (the kernel's chatter) and search. See docs/design.md.

mod access;
mod common;
mod pages;
mod spaces;

use aether_sdk::prelude::*;

handler! {
    // ---- spaces and their people
    fn my_spaces(input: spaces::Search) -> Vec<Value> { spaces::my_spaces(input) }
    fn create_space(input: spaces::NewSpace) -> Value { spaces::create_space(input) }
    fn edit_space(input: spaces::EditSpace) -> Value { spaces::edit_space(input) }
    fn delete_space(input: spaces::SpaceOnly) -> Value { spaces::delete_space(input) }
    fn space_header(input: spaces::SpaceOnly) -> Value { spaces::space_header(input) }
    fn space_info(input: spaces::SpaceOnly) -> Value { spaces::space_info(input) }
    fn space_members(input: spaces::SpaceOnly) -> Vec<Value> { spaces::space_members(input) }
    fn add_space_members(input: spaces::AddMembers) -> Value { spaces::add_space_members(input) }
    fn remove_space_member(input: spaces::RemoveMember) -> Value { spaces::remove_space_member(input) }
    fn set_space_role(input: spaces::SetRole) -> Value { spaces::set_space_role(input) }
    // ---- pages
    fn space_pages(input: spaces::SpaceOnly) -> Vec<Value> { pages::space_pages(input) }
    fn get_page(input: pages::PageOnly) -> Value { pages::get_page(input) }
    fn page_header(input: pages::PageOnly) -> Value { pages::page_header(input) }
    fn create_page(input: pages::NewPage) -> Value { pages::create_page(input) }
    fn update_page(input: pages::UpdatePage) -> Value { pages::update_page(input) }
    fn delete_page(input: pages::PageOnly) -> Value { pages::delete_page(input) }
    fn page_history(input: pages::PageOnly) -> Vec<Value> { pages::page_history(input) }
    fn restore_version(input: pages::Restore) -> Value { pages::restore_version(input) }
    fn search_pages(input: pages::Query) -> Vec<Value> { pages::search_pages(input) }
    fn recent_pages(input: Empty) -> Vec<Value> { pages::recent(input) }
    // ---- pickers on the pages, and the record rule
    fn pick_pages(input: pages::PickParent) -> Vec<Value> { pages::pick_pages(input) }
    fn pick_other_people(input: spaces::Pick) -> Vec<Value> { spaces::pick_other_people(input) }
    fn rule_var_my_spaces(_input: Empty) -> Vec<String> { spaces::rule_my_spaces() }
}
