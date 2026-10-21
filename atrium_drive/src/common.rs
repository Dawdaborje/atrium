//! Helpers shared by the parts of the plugin.

pub use aether_sdk::records::{text, Record};

use aether_sdk::{prelude::*, transport};

pub fn key(id: &str) -> &str {
    id.split_once(':').map_or(id, |(_, key)| key)
}

pub fn me() -> Result<String> {
    context::current()?.actor.id.ok_or_else(|| Error::msg("sign in to use the drive"))
}

pub fn now() -> Result<String> {
    let ctx = context::current()?;
    if ctx.now.is_empty() {
        return Err(Error::msg("the kernel did not say what time it is"));
    }
    Ok(ctx.now)
}

pub fn id_text(record: &Record) -> &str {
    text(record, "id").unwrap_or_default()
}

pub fn int(record: &Record, field: &str) -> i64 {
    record.get(field).and_then(Value::as_i64).unwrap_or(0)
}

pub fn flag(record: &Record, field: &str) -> bool {
    record.get(field).and_then(Value::as_bool).unwrap_or(false)
}

/// A name for a file or folder: no slashes or control characters, not empty, not too long, not `.` or `..`.
pub fn clean_name(label: &str, value: &str) -> Result<String> {
    let name: String = value.trim().chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').collect();
    let name = name.trim().to_string();
    if name.is_empty() || name == "." || name == ".." {
        return Err(Error::msg(format!("{label} is needed")));
    }
    if name.chars().count() > 200 {
        return Err(Error::msg(format!("{label} is at most 200 characters")));
    }
    Ok(name)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Person {
    pub id: String,
    pub name: String,
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
                    Some(Person { id, name })
                })
                .collect()
        })
        .unwrap_or_default())
}

pub fn name_of(people: &[Person], user: &str) -> String {
    people
        .iter()
        .find(|p| key(&p.id) == key(user))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| key(user).chars().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect())
}

/// `name`, or `name (2)`, `name (3)`… when `taken` already holds it; the number goes before the extension.
pub fn free_name(name: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    (2..1000)
        .map(|n| format!("{stem} ({n}){ext}"))
        .find(|candidate| !taken.iter().any(|t| t.eq_ignore_ascii_case(candidate)))
        .unwrap_or_else(|| format!("{stem} (copy){ext}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_lose_slashes_and_control_characters() {
        assert_eq!(clean_name("a name", " a/b\\c\n.txt ").unwrap(), "abc.txt");
        assert!(clean_name("a name", "  ").is_err());
        assert!(clean_name("a name", "..").is_err());
        assert!(clean_name("a name", &"x".repeat(201)).is_err());
    }

    #[test]
    fn a_taken_name_gets_a_number_before_its_extension() {
        let taken = vec!["report.pdf".to_string(), "Report (2).pdf".to_string(), "notes".to_string()];
        assert_eq!(free_name("report.pdf", &taken), "report (3).pdf");
        assert_eq!(free_name("notes", &taken), "notes (2)");
        assert_eq!(free_name("fresh.txt", &taken), "fresh.txt");
        assert_eq!(free_name(".hidden", &[".hidden".to_string()]), ".hidden (2)");
    }

    #[test]
    fn keys_drop_the_table() {
        assert_eq!(key("mdl_x:abc"), "abc");
        assert_eq!(key("abc"), "abc");
    }
}
