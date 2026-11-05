//! NEXUS core library.
//!
//! Everything the desktop app, the CLI, the MCP server and the hook handler share lives here.
//! This crate has no dependency on Tauri, PTYs or any UI (see `docs/ARCHITECTURE.md`).

pub mod activity;
pub mod attention;
pub mod checkpoint;
pub mod claude;
pub mod context;
pub mod db;
pub mod doctor;
pub mod env;
pub mod error;
pub mod git;
pub mod host;
pub mod kits;
pub mod knowledge;
pub mod notify;
pub mod paths;
pub mod proc;
pub mod project;
pub mod resume;
pub mod session;
pub mod settings;
pub mod usage;
pub mod workprofile;
pub mod workflow;
pub mod workspace;
pub mod workspace_sync;

pub use error::{CoreError, Result};

/// Current time as Unix milliseconds.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Resolve a user-supplied reference against named records: exact id, then case-insensitive exact name, then a
/// unique id prefix (at least 4 characters). Ambiguity is an error rather than a guess.
pub fn resolve_ref<'a, T>(items: &'a [T], query: &str, id: impl Fn(&T) -> &str, name: impl Fn(&T) -> &str, what: &str) -> Result<&'a T> {
    let q = query.trim();
    if let Some(found) = items.iter().find(|i| id(i) == q) {
        return Ok(found);
    }
    let by_name: Vec<&T> = items.iter().filter(|i| name(i).eq_ignore_ascii_case(q)).collect();
    if by_name.len() == 1 {
        return Ok(by_name[0]);
    }
    if by_name.len() > 1 {
        return Err(CoreError::Conflict(format!("more than one {what} is named '{q}'; use its id")));
    }
    if q.len() >= 4 {
        let by_prefix: Vec<&T> = items.iter().filter(|i| id(i).starts_with(&q.to_ascii_lowercase())).collect();
        match by_prefix.len() {
            1 => return Ok(by_prefix[0]),
            n if n > 1 => return Err(CoreError::Conflict(format!("'{q}' matches {n} {what}s; use more characters"))),
            _ => {}
        }
    }
    Err(CoreError::NotFound(format!("no {what} matches '{q}'")))
}

/// New random identifier (UUID v4, hyphenated, lowercase).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_ref_prefers_id_then_name_then_unique_prefix() {
        let items = [("a1b2c3d4-0000", "Personal"), ("a1b2ffff-0000", "Work"), ("ffff0000-0000", "work")];
        let f = |q: &str| resolve_ref(&items, q, |i| i.0, |i| i.1, "profile").map(|i| i.0);
        assert_eq!(f("Personal").unwrap(), "a1b2c3d4-0000");
        assert_eq!(f("personal").unwrap(), "a1b2c3d4-0000");
        assert_eq!(f("a1b2c").unwrap(), "a1b2c3d4-0000");
        assert_eq!(f("ffff0000-0000").unwrap(), "ffff0000-0000");
        assert_eq!(f("work").unwrap_err().code(), "conflict", "two case-insensitive name matches");
        assert_eq!(f("a1b2").unwrap_err().code(), "conflict", "ambiguous prefix");
        assert_eq!(f("abc").unwrap_err().code(), "not_found", "prefixes need 4+ characters");
        assert_eq!(f("nope").unwrap_err().code(), "not_found");
    }
}
