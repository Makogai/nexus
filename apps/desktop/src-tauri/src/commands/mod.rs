//! Tauri IPC commands. Thin adapters: validate/convert input, call `nexus-core`, map errors.
//! Slow work (process spawns, git, filesystem scans) runs on the blocking pool.

pub mod app;
pub mod claude;
pub mod constitutions;
pub mod context;
pub mod devenv;
pub mod kits;
pub mod knowledge;
pub mod notifications;
pub mod projects;
pub mod sessions;
pub mod skills;
pub mod sync;
pub mod usage;
pub mod workflows;
pub mod workspaces;
