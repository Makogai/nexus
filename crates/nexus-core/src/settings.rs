//! Application settings: a small typed key/value store in SQLite.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};

use crate::error::{CoreError, Result};

pub const ACTIVE_WORKSPACE: &str = "active_workspace_id";
pub const UI_PREFERENCES: &str = "ui_preferences";

pub fn get<T: DeserializeOwned>(conn: &Connection, key: &str) -> Result<Option<T>> {
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM app_settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?;
    raw.map(|s| {
        serde_json::from_str(&s).map_err(|source| CoreError::Json {
            what: format!("setting '{key}'"),
            source,
        })
    })
    .transpose()
}

pub fn set<T: Serialize>(conn: &Connection, key: &str, value: &T) -> Result<()> {
    let json = serde_json::to_string(value).map_err(|source| CoreError::Json {
        what: format!("setting '{key}'"),
        source,
    })?;
    conn.execute(
        "INSERT INTO app_settings(key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![key, json],
    )?;
    Ok(())
}

pub fn remove(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM app_settings WHERE key = ?1", [key])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let conn = crate::db::open_in_memory().unwrap();
        assert_eq!(get::<String>(&conn, "x").unwrap(), None);
        set(&conn, "x", &"hello").unwrap();
        set(&conn, "x", &"world").unwrap();
        assert_eq!(get::<String>(&conn, "x").unwrap().as_deref(), Some("world"));
        remove(&conn, "x").unwrap();
        assert_eq!(get::<String>(&conn, "x").unwrap(), None);
    }
}
