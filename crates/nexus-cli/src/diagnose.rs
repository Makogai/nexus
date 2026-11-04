//! `nexus profiles diagnose <profile>` — what a Claude profile's configuration actually contains (ADR-0023).
//! Read-only; secrets are never read out (names, counts and presence only).

use nexus_core::claude::diagnostics::{self, Diagnostics};
use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::claude::profile;
use nexus_core::{Result, env, resolve_ref};

use crate::{Ctx, out};

pub fn run(ctx: &Ctx, query: &str) -> Result<()> {
    let conn = ctx.db()?;
    let list = profile::list(&conn, true)?;
    let p = resolve_ref(&list, query, |p| &p.id, |p| &p.name, "profile")?.clone();
    let d = diagnostics::collect(&conn, &ctx.data_dir, &ctx.home, &p, &MachinePolicyLocations::system(), &env::current())?;
    if ctx.json {
        out::json(&d);
    } else {
        print(&d);
    }
    Ok(())
}

fn section(title: &str) {
    println!("\n{title}");
}

pub fn print(d: &Diagnostics) {
    println!("{} — Claude profile diagnostics", d.profile_name);
    println!(
        "  config   {}{}{}",
        d.config_dir.display(),
        if d.default_location { " (default location)" } else { "" },
        if d.config_exists { "" } else { " — missing" }
    );
    println!(
        "  claude   {} {}",
        d.executable.as_ref().map(|e| e.display().to_string()).unwrap_or_else(|| "not found".into()),
        d.version.as_deref().unwrap_or("")
    );
    if d.policy.organization_managed {
        section("Organisation policy (managed — NEXUS cannot change it)");
        for e in &d.policy_effects {
            println!("  · {e}");
        }
        if d.policy_effects.is_empty() {
            println!("  · managed settings present; no restriction NEXUS depends on");
        }
    }
    let s = &d.settings;
    section("User settings");
    if !s.exists {
        println!("  none ({})", s.path.display());
    } else {
        println!("  model {} · permission mode {} · rules {} allow / {} ask / {} deny", s.model.as_deref().unwrap_or("default"), s.default_permission_mode.as_deref().unwrap_or("default"), s.permission_rules.0, s.permission_rules.1, s.permission_rules.2);
        if !s.env_names.is_empty() {
            println!("  env (names only): {}", s.env_names.join(", "));
        }
        if s.api_key_helper {
            println!("  apiKeyHelper is configured (command not shown)");
        }
    }
    section(&format!("Skills ({})", d.skills.len()));
    for i in &d.skills {
        println!("  {:<6} {}  — {}", i.source, i.name, i.description);
    }
    section(&format!("Agents ({}) · commands ({}) · output styles ({})", d.agents.len(), d.commands.len(), d.output_styles.len()));
    for i in &d.agents {
        println!("  agent  {}  — {}", i.name, i.description);
    }
    for i in &d.commands {
        println!("  /{}", i.name);
    }
    section(&format!("MCP servers ({})", d.mcp_servers.len()));
    for m in &d.mcp_servers {
        println!("  {:<7} {} — {} {}{}", m.scope, m.qualified_name, m.server.transport, m.server.target, m.origin.as_deref().map(|o| format!(" ({o})")).unwrap_or_default());
    }
    section(&format!("Hooks ({})", d.hooks.iter().map(|h| h.group.handlers).sum::<usize>()));
    for h in &d.hooks {
        println!("  {:<6} {} × {} [{}] {}", h.source, h.group.event, h.group.handlers, h.group.types.join(","), h.group.programs.join(", "));
    }
    section(&format!("Plugins ({}) · marketplaces ({})", d.plugins.len(), d.marketplaces.len()));
    for p in &d.plugins {
        let state = match (p.installed, p.enabled) {
            (true, true) => "enabled",
            (true, false) => "disabled",
            (false, _) => "not installed",
        };
        println!("  {} {} — {state} · {} skills · ~{} tokens{}", p.id, p.version.as_deref().unwrap_or(""), p.skills, p.listing_tokens, if p.runs_code { " · runs code (hooks/MCP)" } else { "" });
    }
    for m in &d.marketplaces {
        println!("  marketplace {} ({}) {}", m.name, m.kind, m.location);
    }
    if !d.kits.is_empty() {
        section("NEXUS kits for this profile (session-scoped --plugin-dir)");
        for k in &d.kits {
            println!("  {} — {} skill(s){}", k.inventory.name, k.inventory.skills.len(), k.blocked.as_deref().map(|b| format!(" · not used: {b}")).unwrap_or_default());
        }
    }
    section("Context at every session start (estimate)");
    let c = &d.context;
    println!("  ~{} tokens: skills {} · agents {} · commands {} · user memory {} (excludes built-ins and project files)", c.total_tokens, c.skills_tokens, c.agents_tokens, c.commands_tokens, c.memory_tokens);
    for w in &d.warnings {
        println!("\nwarning: {w}");
    }

}
