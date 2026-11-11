//! OpenAI Codex CLI as a NEXUS agent (ADR-0052): a pure launch builder (`-c` overrides only), what a session gets
//! (`prepare`), and the `notify` program that tells NEXUS a turn finished (`notify`).

pub mod launch;
pub mod notify;
pub mod prepare;
