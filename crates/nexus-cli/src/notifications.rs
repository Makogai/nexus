//! `nexus notifications …`: the notification center from the terminal (ADR-0025). Same rows as the desktop's.

use clap::Subcommand;

use nexus_core::notify::{self, Filter};
use nexus_core::{CoreError, Result};

use crate::{Ctx, out};

#[derive(Subcommand)]
pub enum NotificationsCmd {
    /// Open notifications, newest first.
    List {
        /// Include dismissed and snoozed ones.
        #[arg(long)]
        all: bool,
        #[arg(long)]
        unread: bool,
        #[arg(long, default_value_t = 30)]
        limit: u32,
    },
    /// Mark notifications as read (`--all` for every one).
    Read {
        ids: Vec<String>,
        #[arg(long)]
        all: bool,
    },
    /// Dismiss notifications.
    Dismiss { ids: Vec<String> },
    /// Delete notifications older than the retention period (and all dismissed ones with `--dismissed`).
    Clean {
        #[arg(long)]
        dismissed: bool,
    },
}

/// Exact id, or a unique id suffix (ids are UUIDv7: their prefixes are timestamps and collide).
fn resolve(conn: &nexus_core::db::Connection, r: &str) -> Result<String> {
    let all = notify::list(conn, &Filter { include_dismissed: true, limit: Some(1000), ..Default::default() })?;
    if let Some(n) = all.iter().find(|n| n.id == r) {
        return Ok(n.id.clone());
    }
    let hits: Vec<_> = all.iter().filter(|n| r.len() >= 4 && n.id.ends_with(r)).collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => Err(CoreError::not_found(format!("no notification {r}"))),
        _ => Err(CoreError::invalid(format!("'{r}' matches {} notifications — use more characters", hits.len()))),
    }
}

fn short(id: &str) -> String {
    id.chars().rev().take(8).collect::<Vec<_>>().into_iter().rev().collect()
}

pub fn run(ctx: &Ctx, c: NotificationsCmd) -> Result<()> {
    let conn = ctx.db()?;
    match c {
        NotificationsCmd::List { all, unread, limit } => {
            let list = notify::list(&conn, &Filter { include_dismissed: all, unread_only: unread, limit: Some(limit), ..Default::default() })?;
            if ctx.json {
                out::json(&serde_json::json!({ "counts": notify::counts(&conn)?, "notifications": list }));
                return Ok(());
            }
            let counts = notify::counts(&conn)?;
            println!("{} unread, {} need action", counts.unread, counts.action_required);
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|n| {
                    let state = if n.dismissed_at.is_some() {
                        "dismissed"
                    } else if n.resolved_at.is_some() {
                        "resolved"
                    } else if n.read_at.is_none() {
                        "new"
                    } else {
                        "read"
                    };
                    let times = if n.occurrences > 1 { format!(" (×{})", n.occurrences) } else { String::new() };
                    vec![short(&n.id), n.priority.clone(), n.category.clone(), state.into(), out::ago(Some(n.updated_at)), format!("{}{times}", n.title)]
                })
                .collect();
            out::table(&["ID", "PRIORITY", "CATEGORY", "STATE", "WHEN", "TITLE"], &rows);
        }
        NotificationsCmd::Read { ids, all } => {
            let n = if all {
                notify::mark_all_read(&conn)?
            } else {
                let ids = ids.iter().map(|r| resolve(&conn, r)).collect::<Result<Vec<_>>>()?;
                notify::mark_read(&conn, &ids)?
            };
            println!("{n} marked as read");
        }
        NotificationsCmd::Dismiss { ids } => {
            let ids = ids.iter().map(|r| resolve(&conn, r)).collect::<Result<Vec<_>>>()?;
            println!("{} dismissed", notify::dismiss(&conn, &ids)?);
        }
        NotificationsCmd::Clean { dismissed } => {
            println!("{} deleted", notify::cleanup(&conn, notify::RETENTION_DAYS, dismissed)?);
        }
    }
    Ok(())
}
