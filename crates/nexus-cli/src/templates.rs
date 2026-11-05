//! `nexus templates …`: workspace starter templates — list, show, install/update with a reviewed plan (ADR-0037).

use clap::Subcommand;

use nexus_core::Result;
use nexus_core::templates::{self, Action, ComponentKind};

use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum TemplateCmd {
    /// The built-in templates and what each contains.
    List,
    /// Every file TEMPLATE installs (with the templates it extends).
    Show { template: String },
    /// Install TEMPLATE in the active workspace, or update it to this NEXUS's version. Shows the plan; changes nothing
    /// without --yes. Files you changed or created yourself are never overwritten.
    Apply {
        template: String,
        #[arg(long)]
        yes: bool,
    },
}

fn kind_label(k: ComponentKind) -> &'static str {
    match k {
        ComponentKind::Constitution => "constitution",
        ComponentKind::Skill => "skill",
        ComponentKind::Workflow => "workflow",
        ComponentKind::Integration => "integration",
        ComponentKind::File => "file",
    }
}

fn action_label(a: Action) -> &'static str {
    match a {
        Action::Add => "add",
        Action::Update => "update",
        Action::Unchanged => "unchanged",
        Action::KeepCustomized => "keep yours (changed)",
        Action::KeepRemoved => "keep removed",
        Action::SkipExisting => "skip (your file)",
        Action::Retire => "no longer in the template (kept)",
    }
}

pub fn run(ctx: &Ctx, c: TemplateCmd) -> Result<i32> {
    match c {
        TemplateCmd::List => {
            let list = templates::list()?;
            if ctx.json {
                out::json(&list);
                return Ok(0);
            }
            for t in &list {
                println!("{} {} — {}", t.info.id, t.info.version, t.info.title);
                println!("  {}", t.info.summary);
                println!(
                    "  {} constitutions, {} stack rule sets, {} skills, {} workflows, {} integrations · always on: {} characters",
                    t.constitutions, t.stack_rule_sets, t.skills, t.workflows, t.integrations, t.always_on_chars
                );
            }
        }
        TemplateCmd::Show { template } => {
            let comps = templates::components(&template)?;
            if ctx.json {
                out::json(&comps);
                return Ok(0);
            }
            let rows: Vec<Vec<String>> = comps.iter().map(|c| vec![kind_label(c.kind).into(), c.path.clone(), c.template.clone()]).collect();
            out::table(&["KIND", "PATH", "FROM"], &rows);
        }
        TemplateCmd::Apply { template, yes } => {
            let conn = ctx.db()?;
            let ws = active_workspace(&conn)?;
            let plan = templates::plan(&ws.root_path, &template)?;
            if plan.is_noop() {
                if ctx.json {
                    out::json(&serde_json::json!({ "plan": plan, "applied": false }));
                } else {
                    println!("{} already has {} {} — nothing to change.", ws.name, plan.template.id, plan.template.version);
                }
                return Ok(0);
            }
            if !yes {
                if ctx.json {
                    out::json(&serde_json::json!({ "plan": plan, "applied": false }));
                    return Ok(1);
                }
                println!("Installing {} {} in {}:", plan.template.id, plan.template.version, ws.name);
                for a in [Action::Add, Action::Update, Action::KeepCustomized, Action::KeepRemoved, Action::SkipExisting, Action::Retire] {
                    let items: Vec<&str> = plan.items.iter().filter(|i| i.action == a).map(|i| i.path.as_str()).collect();
                    if !items.is_empty() {
                        println!("  {} ({}): {}", action_label(a), items.len(), items.iter().take(12).copied().collect::<Vec<_>>().join(", ") + if items.len() > 12 { ", …" } else { "" });
                    }
                }
                for n in &plan.notes {
                    println!("  · {n}");
                }
                println!("Repeat with --yes to apply.");
                return Ok(1);
            }
            let done = templates::apply(&conn, &ctx.data_dir, &ws, &template)?;
            if ctx.json {
                out::json(&serde_json::json!({ "plan": done.plan, "applied": true, "written": done.written, "savedForReview": done.saved_for_review, "kitId": done.kit_id }));
            } else {
                println!("{} {} is installed in {}: {} file(s) written.", done.plan.template.id, done.plan.template.version, ws.name, done.written.len());
                for s in &done.saved_for_review {
                    println!("  new template version to compare with your file: {s}");
                }
                if done.kit_id.is_some() {
                    println!("  the workspace skills reach this workspace's Claude sessions as `workspace:<skill>`.");
                }
                println!("  review and commit the new files through the Sync Center (`nexus sync review`).");
            }
        }
    }
    Ok(0)
}
