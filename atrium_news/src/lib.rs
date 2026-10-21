//! Atrium news: posts for the whole organization, written by chosen authors, with comments, likes, a pinned place at the
//! top and a note to everyone when one is published. See docs/design.md.

mod common;
mod posts;

use aether_sdk::prelude::*;

handler! {
    fn feed(input: posts::Feed) -> Vec<Value> { posts::feed(input) }
    fn my_posts(input: Empty) -> Vec<Value> { posts::my_posts(input) }
    fn category_options(_input: Empty) -> Vec<Value> { posts::category_options() }
    fn news_access(_input: Empty) -> Value { posts::news_access() }
    fn get_post(input: posts::PostOnly) -> Value { posts::get_post(input) }
    fn create_post(input: posts::NewPost) -> Value { posts::create_post(input) }
    fn update_post(input: posts::UpdatePost) -> Value { posts::update_post(input) }
    fn publish_post(input: posts::Publish) -> Value { posts::publish_post(input) }
    fn unpublish_post(input: posts::PostOnly) -> Value { posts::unpublish_post(input) }
    fn delete_post(input: posts::PostOnly) -> Value { posts::delete_post(input) }
    fn pin_post(input: posts::Pin) -> Value { posts::pin_post(input) }
    fn toggle_like(input: posts::PostOnly) -> Value { posts::toggle_like(input) }
    fn post_readers(input: posts::PostOnly) -> Vec<Value> { posts::post_readers(input) }
    // ---- who may write
    fn authors(_input: Empty) -> Vec<Value> { posts::authors() }
    fn add_authors(input: posts::AddAuthors) -> Value { posts::add_authors(input) }
    fn remove_author(input: posts::PostOnly) -> Value { posts::remove_author(input) }
    fn pick_people(input: posts::Pick) -> Vec<Value> { posts::pick_people(input) }
}
