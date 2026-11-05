//! `nexus backup …`: backups of NEXUS's machine-local state and a safe restore (ADR-0032).

use clap::Subcommand;

use nexus_core::db::{self, backup};
use nexus_core::{CoreError, Result};

use crate::{Ctx, out};

#[derive(Subcommand)]
pub enum BackupCmd {
    /// Back up the database and the machine-local global knowledge now.
    Create,
    /// Backups, newest first, and whether this version can restore them.
    List,
    /// Replace the current state with backup ID (the current state is backed up first and kept).
    /// Refused while NEXUS Desktop or a `nexus launch` session runs.
    Restore {
        id: String,
        /// Confirm the replacement (without it, only what would happen is shown).
        #[arg(long)]
        yes: bool,
    },
    Delete {
        id: String,
    },
}

fn size(b: u64) -> String {
    if b >= 1 << 20 { format!("{:.1} MB", b as f64 / f64::from(1 << 20)) } else { format!("{} KB", b.div_ceil(1024)) }
}

pub fn run(ctx: &Ctx, c: BackupCmd) -> Result<i32> {
    match c {
        BackupCmd::Create => {
            let conn = ctx.db()?;
            let b = backup::create(&conn, &ctx.data_dir, "manual")?;
            if ctx.json { out::json(&b) } else { println!("backup {} ({}) in {}", b.id, size(b.bytes), b.path.display()) }
        }
        BackupCmd::List => {
            let list = backup::list(&ctx.data_dir)?;
            if ctx.json {
                out::json(&list);
                return Ok(0);
            }
            if list.is_empty() {
                println!("no backups yet — `nexus backup create`");
                return Ok(0);
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|b| {
                    vec![
                        b.id.clone(),
                        b.kind.clone(),
                        out::ago(Some(b.created_at)),
                        b.schema_version.map(|v| format!("v{v}")).unwrap_or_else(|| "–".into()),
                        size(b.bytes),
                        if b.restorable { "yes".into() } else { format!("no — {}", b.problem.clone().unwrap_or_default()) },
                    ]
                })
                .collect();
            out::table(&["ID", "KIND", "WHEN", "SCHEMA", "SIZE", "RESTORABLE"], &rows);
            println!("\nthis version understands schema up to v{}; backups are in {}", db::SCHEMA_VERSION, ctx.data_dir.join(backup::BACKUP_DIR).display());
        }
        BackupCmd::Restore { id, yes } => {
            {
                let conn = ctx.db()?;
                let live = backup::live_hosts(&conn, None)?;
                if !live.is_empty() {
                    return Err(CoreError::Conflict(format!(
                        "NEXUS is running ({}) — close NEXUS Desktop and terminal sessions first, or restore from Settings → Data & backups",
                        live.join(", ")
                    )));
                }
            }
            let b = backup::list(&ctx.data_dir)?.into_iter().find(|b| b.id == id).ok_or_else(|| CoreError::not_found(format!("no backup {id}")))?;
            if !b.restorable {
                return Err(CoreError::invalid(format!("backup {id} cannot be restored: {}", b.problem.unwrap_or_default())));
            }
            if !yes {
                println!(
                    "restoring {id} ({}, schema v{}) replaces the current database and global knowledge.\nThe current state is backed up first and the replaced files are kept. Repeat with --yes to restore.",
                    out::ago(Some(b.created_at)),
                    b.schema_version.unwrap_or(0)
                );
                return Ok(1);
            }
            let r = backup::restore(&ctx.data_dir, &id)?;
            if ctx.json {
                out::json(&r);
            } else {
                println!("restored {} (schema v{})", r.restored, r.schema_version);
                if let Some(s) = &r.safety_backup {
                    println!("the state before the restore is backup {s}");
                }
                println!("the replaced files are kept in {}", r.replaced_kept_in.display());
            }
        }
        BackupCmd::Delete { id } => {
            backup::delete(&ctx.data_dir, &id)?;
            println!("deleted backup {id}");
        }
    }
    Ok(0)
}
