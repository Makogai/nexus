//! NEXUS core library.
//!
//! Everything the desktop app, the CLI, the MCP server and the hook handler share lives here.
//! This crate has no dependency on Tauri, PTYs or any UI (see `docs/ARCHITECTURE.md`).

pub mod claude;
pub mod db;
pub mod env;
pub mod error;
pub mod git;
pub mod paths;
pub mod proc;
pub mod project;
pub mod session;
pub mod settings;
pub mod workprofile;
pub mod workspace;

pub use error::{CoreError, Result};

/// Current time as Unix milliseconds.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// New random identifier (UUID v4, hyphenated, lowercase).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
