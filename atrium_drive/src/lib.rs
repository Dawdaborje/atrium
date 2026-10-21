//! Atrium drive: folders and files with sharing, versions and a trash, kept on the media store.
//!
//! The bytes live in the kernel's media store; this plugin keeps the folders, the names, who may see what, and every
//! version of a file. The stored files are put under the plugin's guard (`media::protect`), so the kernel serves one
//! only when [`items::can_read_media`] allows the person asking. See docs/design.md.

mod access;
mod browse;
mod common;
mod editor;
mod index;
mod items;
mod links;
mod quota;
mod settings;
mod shares;
mod social;

use aether_sdk::prelude::*;

handler! {
    // ---- looking
    fn drive_browse(input: browse::Browse) -> Value { browse::browse(input) }
    fn search_files(input: browse::Search) -> Value { browse::search(input) }
    fn folder_tree(_input: Empty) -> Vec<Value> { browse::folder_tree() }
    fn storage_used(_input: Empty) -> Value { browse::storage_used() }
    fn drive_nav(_input: Empty) -> Vec<Value> { browse::nav() }
    fn list_people(_input: Empty) -> Vec<Value> { browse::list_people() }
    fn open_in_editor(input: editor::Open) -> Value { editor::open_in_editor(input) }
    fn file_info(input: browse::FileInfo) -> Value { browse::file_info(input) }
    fn file_versions(input: items::FileOnly) -> Vec<Value> { items::versions(input) }
    fn list_shares(input: shares::ItemOnly) -> Vec<Value> { shares::list_shares(input) }
    // ---- making and changing
    fn create_folder(input: items::NewFolder) -> Value { items::create_folder(input) }
    fn register_file(input: items::Register) -> Value { items::register_file(input) }
    fn new_version(input: items::NewVersion) -> Value { items::new_version(input) }
    fn restore_version(input: items::RestoreVersion) -> Value { items::restore_version(input) }
    fn rename_item(input: items::Rename) -> Value { items::rename(input) }
    fn move_item(input: items::Move) -> Value { items::move_item(input) }
    fn trash_item(input: items::Target) -> Value { items::trash(input) }
    fn restore_item(input: items::Target) -> Value { items::restore(input) }
    fn purge_item(input: items::Target) -> Value { items::purge(input) }
    fn empty_trash(_input: Empty) -> Value { items::empty_trash() }
    // ---- links, comments, favourites
    fn create_link(input: links::NewLink) -> Value { links::create_link(input) }
    fn list_links(input: links::ItemOnly) -> Vec<Value> { links::list_links(input) }
    fn revoke_link(input: links::LinkOnly) -> Value { links::revoke_link(input) }
    fn open_link(input: links::Open) -> Value { links::open_link(input) }
    fn file_comments(input: social::FileOnly) -> Vec<Value> { social::file_comments(input) }
    fn add_comment(input: social::NewComment) -> Value { social::add_comment(input) }
    fn delete_comment(input: social::CommentOnly) -> Value { social::delete_comment(input) }
    fn toggle_favourite(input: social::Star) -> Value { social::toggle_favourite(input) }
    // ---- the organization's limits
    fn drive_settings(_input: Empty) -> Value { settings::get_settings() }
    fn save_drive_settings(input: settings::Change) -> Value { settings::save_settings(input) }
    // ---- the kernel calls this when it has read a file for search
    fn index_done(input: Value) -> Value { index::index_done(input) }
    // ---- sharing
    fn share_item(input: shares::ShareIn) -> Value { shares::share(input) }
    fn unshare_item(input: shares::Unshare) -> Value { shares::unshare(input) }
    // ---- the kernel asks this before it serves a stored file
    fn can_read_media(input: items::KeyIn) -> Value { items::can_read_media(input) }
}
