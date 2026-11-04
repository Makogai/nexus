//! `nexus kits …` — AI Kits (ADR-0024): register local or Git-backed Claude Code plugins / skill folders, review and
//! approve them, and enable them for chosen profiles (session-scoped `--plugin-dir`; no Claude config is modified).

use clap::Subcommand;

use nexus_core::claude::policy::{self, MachinePolicyLocations};
use nexus_core::claude::{install, profile};
use nexus_core::kits::{self, KitView, Selection};
use nexus_core::{CoreError, Result, env, resolve_ref};

use crate::{Ctx, out};

#[derive(Subcommand)]
pub enum KitCmd {
    /// Registered kits and the profiles they are enabled for.
    List,
    /// Register a kit: a local folder, or a Git URL with --git (NEXUS clones it into its data folder).
    Add {
        source: String,
        #[arg(long)]
        git: bool,
        /// Branch or tag to follow (Git kits).
        #[arg(long = "ref")]
        git_ref: Option<String>,
        /// Plugin folder inside the source.
        #[arg(long)]
        subdir: Option<String>,
        /// Plugin name when the source is a marketplace.
        #[arg(long)]
        plugin: Option<String>,
    },
    /// Contents: skills, agents, commands, hooks, MCP servers, context cost and problems.
    Show { kit: String },
    /// Approve the kit's current content after reviewing it (`kits show`). Required before any session uses it.
    Approve { kit: String },
    /// Re-read a local kit or fetch the followed ref of a Git kit (changes need a new approval).
    Update { kit: String },
    /// Enable a kit for a profile; only the selected skills reach that profile's sessions.
    Enable {
        kit: String,
        #[arg(long, short = 'p')]
        profile: String,
        /// Skills to include (comma-separated); default: all for small kits, none for large ones.
        #[arg(long, value_delimiter = ',', conflicts_with = "all_skills")]
        skills: Option<Vec<String>>,
        #[arg(long)]
        all_skills: bool,
        #[arg(long)]
        no_agents: bool,
        #[arg(long)]
        no_commands: bool,
        /// Also run the kit's hooks (they execute commands).
        #[arg(long)]
        hooks: bool,
        /// Also start the kit's MCP servers (they execute programs).
        #[arg(long)]
        mcp: bool,
    },
    /// Disable a kit for a profile.
    Disable {
        kit: String,
        #[arg(long, short = 'p')]
        profile: String,
    },
    /// Run Claude Code's own `claude plugin validate` on the kit (with an empty NEXUS-owned config dir).
    Validate {
        kit: String,
        /// Claude executable of this profile (default: the first registered profile's).
        #[arg(long, short = 'p')]
        profile: Option<String>,
    },
    /// Unregister a kit (NEXUS's own checkout is deleted; a local kit folder is never touched).
    Remove { kit: String },
}

fn find(conn: &nexus_core::db::Connection, ctx: &Ctx, q: &str) -> Result<KitView> {
    let all = kits::list(conn, &ctx.data_dir)?;
    resolve_ref(&all, q, |k| &k.kit.id, |k| &k.kit.name, "kit").cloned()
}

fn status_word(v: &KitView) -> &'static str {
    match v.status {
        "approved" => "approved",
        "changed" => "CHANGED — review again",
        "missing" => "files missing",
        _ => "needs review",
    }
}

fn print_kit(v: &KitView, profiles: &[profile::ClaudeProfile]) {
    let i = &v.inventory;
    println!("{} {} — {} kit · {} · {}", i.name, i.version.as_deref().unwrap_or(""), i.kind, v.kit.source_kind, status_word(v));
    println!("  source  {}{}", v.kit.location, if v.kit.subdir.is_empty() { String::new() } else { format!(" ({})", v.kit.subdir) });
    if let Some(d) = &i.description {
        println!("  {d}");
    }
    println!("  context ~{} tokens if every listing is loaded · {} files", i.listing_tokens(), i.files);
    for s in &i.skills {
        println!("  skill   {}:{} — {}", i.name, s.name, s.description);
    }
    for a in &i.agents {
        println!("  agent   {}:{} — {}", i.name, a.name, a.description);
    }
    for c in &i.commands {
        println!("  command /{}:{}", i.name, c.name);
    }
    for h in &i.hooks {
        println!("  hook    {} × {} — runs {}", h.event, h.handlers, if h.programs.is_empty() { h.types.join(",") } else { h.programs.join(", ") });
    }
    for m in &i.mcp_servers {
        println!("  mcp     {} — {} {} (starts a program)", m.name, m.transport, m.target);
    }
    for p in &i.problems {
        println!("  problem {p}");
    }
    for r in &v.profiles {
        let name = profiles.iter().find(|p| p.id == r.profile_id).map(|p| p.name.as_str()).unwrap_or("?");
        let skills = match &r.selection.skills {
            None => "all skills".to_string(),
            Some(s) if s.is_empty() => "no skills".to_string(),
            Some(s) => s.join(", "),
        };
        println!("  enabled for {name}: {skills}{}{}", if r.selection.hooks { " + hooks" } else { "" }, if r.selection.mcp { " + MCP" } else { "" });
    }
}

pub fn run(ctx: &Ctx, cmd: KitCmd) -> Result<()> {
    let conn = ctx.db()?;
    let profiles = profile::list(&conn, true)?;
    let find_profile = |q: &str| resolve_ref(&profiles, q, |p| &p.id, |p| &p.name, "profile").cloned();
    match cmd {
        KitCmd::List => {
            let all = kits::list(&conn, &ctx.data_dir)?;
            if ctx.json {
                out::json(&all);
                return Ok(());
            }
            if all.is_empty() {
                println!("no kits — register one with `nexus kits add <folder>` or `nexus kits add --git <url>`");
            }
            let rows: Vec<Vec<String>> = all
                .iter()
                .map(|v| {
                    let on: Vec<&str> = v.profiles.iter().filter_map(|r| profiles.iter().find(|p| p.id == r.profile_id).map(|p| p.name.as_str())).collect();
                    vec![v.inventory.name.clone(), status_word(v).into(), format!("{} skills", v.inventory.skills.len()), format!("~{} tok", v.inventory.listing_tokens()), on.join(", ")]
                })
                .collect();
            out::table(&["KIT", "STATUS", "SKILLS", "LISTINGS", "PROFILES"], &rows);
        }
        KitCmd::Add { source, git, git_ref, subdir, plugin } => {
            let k = if git {
                kits::register_git(&conn, &ctx.data_dir, &source, git_ref.as_deref(), subdir.as_deref(), plugin.as_deref())?
            } else {
                kits::register_local(&conn, &ctx.data_dir, &source, subdir.as_deref(), plugin.as_deref())?
            };
            let v = kits::view(&conn, &ctx.data_dir, &k)?;
            if ctx.json {
                out::json(&v);
            } else {
                print_kit(&v, &profiles);
                println!("\nreview it, then `nexus kits approve {}` — no session uses it before that", k.name);
            }
        }
        KitCmd::Show { kit } => {
            let v = find(&conn, ctx, &kit)?;
            if ctx.json {
                out::json(&v);
            } else {
                print_kit(&v, &profiles);
            }
        }
        KitCmd::Approve { kit } => {
            let v = find(&conn, ctx, &kit)?;
            let v = kits::approve(&conn, &ctx.data_dir, &v.kit.id)?;
            if ctx.json {
                out::json(&v);
            } else {
                println!("approved {} (this exact content; any change needs a new approval)", v.inventory.name);
            }
        }
        KitCmd::Update { kit } => {
            let v = find(&conn, ctx, &kit)?;
            let v = kits::update(&conn, &ctx.data_dir, &v.kit.id)?;
            if ctx.json {
                out::json(&v);
            } else {
                println!("{}: {}", v.inventory.name, status_word(&v));
            }
        }
        KitCmd::Enable { kit, profile: pq, skills, all_skills, no_agents, no_commands, hooks, mcp } => {
            let v = find(&conn, ctx, &kit)?;
            let p = find_profile(&pq)?;
            let existing = v.profiles.iter().find(|r| r.profile_id == p.id).map(|r| r.selection.clone());
            let base = existing.unwrap_or_else(|| kits::default_selection(&v.inventory));
            let sel = Selection {
                skills: if all_skills { None } else { skills.or(base.skills) },
                agents: !no_agents,
                commands: !no_commands,
                hooks,
                mcp,
            };
            let row = kits::enable(&conn, &ctx.data_dir, &v.kit.id, &p.id, Some(sel))?;
            let eff = kits::effective(&v.inventory, &row.selection);
            let pol = policy::evaluate(&p.effective_config_dir(&ctx.home), &MachinePolicyLocations::system());
            if ctx.json {
                out::json(&row);
            } else {
                println!(
                    "{} enabled for {}: {} of {} skill(s), ~{} tokens per session{}{}",
                    v.inventory.name,
                    p.name,
                    eff.skills.len(),
                    v.inventory.skills.len(),
                    eff.listing_tokens(),
                    if hooks { " · hooks ON" } else { "" },
                    if mcp { " · MCP ON" } else { "" }
                );
                if let Some(why) = kits::blocked_by(&pol) {
                    println!("note: sessions of {} will not get it — {why}", p.name);
                }
                if v.status != "approved" {
                    println!("note: approve the kit first (`nexus kits approve {}`) — until then no session uses it", v.inventory.name);
                }
            }
        }
        KitCmd::Disable { kit, profile: pq } => {
            let v = find(&conn, ctx, &kit)?;
            let p = find_profile(&pq)?;
            kits::disable(&conn, &v.kit.id, &p.id)?;
            if !ctx.json {
                println!("{} disabled for {}", v.inventory.name, p.name);
            }
        }
        KitCmd::Validate { kit, profile: pq } => {
            let v = find(&conn, ctx, &kit)?;
            let p = match pq {
                Some(q) => find_profile(&q)?,
                None => profiles.first().cloned().ok_or_else(|| CoreError::invalid("register a Claude profile first"))?,
            };
            let exe = install::resolve(p.executable.as_deref(), &ctx.home)?;
            let r = kits::validate(&ctx.data_dir, &v.kit, &exe, &env::current())?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("{}\n{}", if r.ok { "Claude's validator: OK" } else { "Claude's validator: FAILED" }, r.output.trim());
            }
            if !r.ok {
                return Err(CoreError::invalid("the kit did not pass `claude plugin validate`"));
            }
        }
        KitCmd::Remove { kit } => {
            let v = find(&conn, ctx, &kit)?;
            kits::remove(&conn, &ctx.data_dir, &v.kit.id)?;
            if !ctx.json {
                println!("removed {}", v.inventory.name);
            }
        }
    }
    Ok(())
}
