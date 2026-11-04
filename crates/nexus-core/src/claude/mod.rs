//! Claude Code integration: installations, profiles, organisation policy, auth status and launch specs.

pub mod auth;
pub mod hooks;
pub mod install;
pub mod launch;
pub mod mcp;
pub mod policy;
pub mod profile;

/// Native reasoning-effort levels accepted by `claude --effort` (Claude Code 2.1.x help text).
pub const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
