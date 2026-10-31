//! The people of the organization, as the kernel lists them.

use aether_sdk::{prelude::*, transport};

use crate::common::key;

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: String,
    pub name: String,
    /// Where the profile picture is served, for someone who has uploaded one.
    pub image: Option<String>,
}

/// Everyone who can sign in to this organization.
pub fn org_people() -> Result<Vec<Person>> {
    let reply = transport::call("org::members", &json!({}))?;
    Ok(reply
        .get("data")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let id = row.get("id")?.as_str()?.to_string();
                    let name = row.get("display_name").and_then(Value::as_str).unwrap_or(&id).to_string();
                    let image = row.get("avatar_url").and_then(Value::as_str).map(str::to_string);
                    Some(Person { id, name, image })
                })
                .collect()
        })
        .unwrap_or_default())
}

/// A person's name from the list; the end of the id for someone not in it.
pub fn name_of(people: &[Person], user: &str) -> String {
    people
        .iter()
        .find(|p| key(&p.id) == key(user))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| key(user).chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect())
}

pub fn is_person(people: &[Person], user: &str) -> bool {
    people.iter().any(|p| key(&p.id) == key(user))
}

/// A person's profile picture from the list; none for someone without one.
pub fn image_of(people: &[Person], user: &str) -> Option<String> {
    people.iter().find(|p| key(&p.id) == key(user)).and_then(|p| p.image.clone())
}

/// Put `author_image` on a message row, and on the rows nested in a payload (`row`, `rows`, `latest_replies`), from
/// the author's current picture, so it follows a change of picture and is never stored on the message.
pub fn stamp_images(value: &mut Value, people: &[Person]) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| stamp_images(item, people)),
        Value::Object(fields) => {
            if let Some(author) = fields.get("author").and_then(Value::as_str) {
                let image = image_of(people, author);
                fields.insert("author_image".into(), json!(image));
            }
            for nested in ["row", "rows", "latest_replies"] {
                if let Some(inner) = fields.get_mut(nested) {
                    stamp_images(inner, people);
                }
            }
        }
        _ => {}
    }
}

/// `stamp_images` with the organization's people fetched here; a failure leaves the letters.
pub fn stamp_images_now(value: &mut Value) {
    if let Ok(people) = org_people() {
        stamp_images(value, &people);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn people() -> Vec<Person> {
        vec![Person { id: "users:ann".into(), name: "Ann Lee".into(), image: None }]
    }

    #[test]
    fn names_are_found_by_key() {
        assert_eq!(name_of(&people(), "ann"), "Ann Lee");
        assert_eq!(name_of(&people(), "users:ann"), "Ann Lee");
        assert_eq!(name_of(&people(), "users:zzzzzzzzzz"), "zzzzzz");
        assert!(is_person(&people(), "users:ann") && !is_person(&people(), "bob"));
    }

    #[test]
    fn images_follow_the_author() {
        let mut people = people();
        people[0].image = Some("/api/auth/users/ann/avatar".into());
        let mut payload = json!({ "rows": [{ "author": "users:ann" }, { "author": "users:bob" }], "row": { "author": "ann", "latest_replies": [{ "author": "users:ann" }] } });
        stamp_images(&mut payload, &people);
        assert_eq!(payload["rows"][0]["author_image"], "/api/auth/users/ann/avatar");
        assert!(payload["rows"][1]["author_image"].is_null());
        assert_eq!(payload["row"]["latest_replies"][0]["author_image"], "/api/auth/users/ann/avatar");
    }
}

#[derive(Deserialize, Default)]
pub struct Search {
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    users: Vec<String>,
}

/// Rows for a person picker (a new direct message, adding people to a channel): everyone but the caller.
pub fn list_people(input: Search) -> Result<Vec<Value>> {
    let me = crate::common::me()?;
    let needle = input.q.unwrap_or_default().trim().to_lowercase();
    Ok(org_people()?
        .into_iter()
        .filter(|p| key(&p.id) != key(&me))
        .filter(|p| needle.is_empty() || p.name.to_lowercase().contains(&needle))
        .take(200)
        .map(|p| json!({ "id": p.id, "name": p.name }))
        .collect())
}

/// Names for the avatars of `<presence>`: `{ users: [ids] }` -> `[{ id, name }]`.
pub fn names(input: Search) -> Result<Vec<Value>> {
    let people = org_people()?;
    Ok(input
        .users
        .iter()
        .map(|u| json!({ "id": u, "name": name_of(&people, u), "image": image_of(&people, u) }))
        .collect())
}

/// People named in `body` as `@Name` (case-insensitive, ending at a word boundary), in the order they appear, and
/// whether `@channel` or `@everyone` was written.
pub fn find_mentions<'a>(body: &str, people: &'a [Person]) -> (Vec<&'a Person>, bool) {
    let lower = body.to_lowercase();
    let mut found: Vec<(usize, &Person)> = Vec::new();
    for person in people {
        let needle = format!("@{}", person.name.to_lowercase());
        let mut from = 0;
        while let Some(at) = lower[from..].find(&needle) {
            let start = from + at;
            let end = start + needle.len();
            let boundary = lower[end..].chars().next().is_none_or(|c| !c.is_alphanumeric());
            if boundary && !found.iter().any(|(_, p)| p.id == person.id) {
                found.push((start, person));
            }
            from = end;
        }
    }
    found.sort_by_key(|(at, _)| *at);
    let everyone = ["@channel", "@everyone"].iter().any(|word| {
        lower.match_indices(word).any(|(at, _)| lower[at + word.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric()))
    });
    (found.into_iter().map(|(_, p)| p).collect(), everyone)
}

#[cfg(test)]
mod mention_tests {
    use super::*;

    fn people() -> Vec<Person> {
        vec![
            Person { id: "users:ann".into(), name: "Ann Lee".into(), image: None },
            Person { id: "users:bob".into(), name: "Bob".into(), image: None },
            Person { id: "users:bo".into(), name: "Bo".into(), image: None },
        ]
    }

    #[test]
    fn names_are_found_at_word_boundaries_in_order() {
        let p = people();
        let (found, all) = find_mentions("hi @bob and @Ann Lee, not @bobby or @boxer", &p);
        assert_eq!(found.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Bob", "Ann Lee"]);
        assert!(!all);
    }

    #[test]
    fn a_short_name_does_not_match_inside_a_longer_one() {
        let p = people();
        let (found, _) = find_mentions("@Bob", &p);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "users:bob");
    }

    #[test]
    fn channel_and_everyone_mean_all() {
        let p = people();
        assert!(find_mentions("ping @channel!", &p).1);
        assert!(find_mentions("@everyone", &p).1);
        assert!(!find_mentions("@channels", &p).1);
        assert!(find_mentions("no one here", &p).0.is_empty());
    }
}
