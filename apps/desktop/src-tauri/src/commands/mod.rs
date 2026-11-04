//! Tauri IPC commands. Thin adapters: validate/convert input, call `nexus-core`, map errors.
//! Slow work (process spawns, git, filesystem scans) runs on the blocking pool.

pub mod app;
pub mod claude;
pub mod kits;
pub mod knowledge;
pub mod projects;
pub mod sessions;
pub mod workspaces;
