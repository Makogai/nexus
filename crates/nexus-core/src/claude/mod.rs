//! Claude Code integration: installations, profiles, organisation policy, auth status and launch specs.

pub mod auth;
pub mod diagnostics;
pub mod hooks;
pub mod install;
pub mod instructions;
pub mod isolation;
pub mod launch;
pub mod mcp;
pub mod plugin;
pub mod policy;
pub mod prepare;
pub mod profile;
pub mod setup;

/// Native reasoning-effort levels accepted by `claude --effort` (Claude Code 2.1.x help text).
pub const EFFORT_LEVELS: &[&str] = &["low", "medium", "high", "xhigh", "max"];
