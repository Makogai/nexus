//! `nexus sync …`: Git-based workspace synchronisation from the terminal (ADR-0031, M29). Same operations as the
//! desktop's Sync Center. Commits and pushes need the token of the review you just looked at (`--confirm`), so they
//! only ever publish what you saw; pulls fast-forward only; nothing runs automatically.

use clap::Subcommand;

use nexus_core::git::{DiffEntry, LogEntry};
use nexus_core::host::{self, HostKind};
use nexus_core::workspace_sync::inspect::{Inspection, Severity};
use nexus_core::workspace_sync::ops::{self, Side, Sync};
use nexus_core::workspace_sync::{self as sync, history};
use nexus_core::{CoreError, Result};

use crate::{Ctx, active_workspace, out};

#[derive(Subcommand)]
pub enum SyncCmd {
    /// Repository state, incoming and outgoing commits (as of the last fetch), local changes, issues.
    Status,
    /// Fetch from the remote (no local files change).
    Fetch,
    /// Fetch, then fast-forward to the remote branch (refused when diverged, conflicted or overlapping local changes).
    Pull {
        /// Use what the last fetch brought instead of fetching first.
        #[arg(long)]
        no_fetch: bool,
    },
    /// The diff of one changed file (default: your local change).
    Diff {
        path: String,
        #[arg(long, conflicts_with = "outgoing")]
        incoming: bool,
        #[arg(long)]
        outgoing: bool,
    },
    /// Review what a commit would contain: files and the security inspection; prints the token `commit` needs.
    Review {
        /// Only these files (default: every local change).
        #[arg(long = "path")]
        paths: Vec<String>,
    },
    /// Commit exactly the reviewed changes (never pushes).
    Commit {
        #[arg(short, long)]
        message: String,
        /// The token printed by `nexus sync review` for the same selection.
        #[arg(long)]
        confirm: String,
        #[arg(long = "path")]
        paths: Vec<String>,
    },
    /// Without --confirm: review what a push would send (commits, files, inspection) and print its token.
    /// With --confirm: push exactly that (never forced).
    Push {
        #[arg(long)]
        confirm: Option<String>,
    },
    /// Recent sync operations of this workspace.
    History,
    /// Onboard a workspace from an existing Git repository into FOLDER (new or empty) and make it active.
    Clone { url: String, folder: String },
    /// What restoring the active workspace on this machine still needs.
    Restore,
}

fn commits(title: &str, list: &[LogEntry]) {
    if list.is_empty() {
        return;
    }
    println!("{title} ({}):", list.len());
    for c in list.iter().take(15) {
        println!("  {} {} — {} ({})", &c.oid[..c.oid.len().min(8)], out::ago(Some(c.timestamp_ms)), c.summary, c.author);
    }
}

fn files(title: &str, list: &[DiffEntry]) {
    if list.is_empty() {
        return;
    }
    println!("{title} ({}): {}", list.len(), list.iter().take(10).map(|e| format!("{} {}", e.status, e.path)).collect::<Vec<_>>().join(", "));
}

fn print_inspection(i: &Inspection) {
    if i.findings.is_empty() {
        println!("inspection: nothing found in {} file(s)", i.files);
        return;
    }
    println!("inspection ({} file(s)):", i.files);
    for f in &i.findings {
        let sev = if f.severity == Severity::Block { "BLOCK" } else { "warn " };
        let at = if f.path.is_empty() { String::new() } else { format!("{}{}: ", f.path, f.line.map(|l| format!(":{l}")).unwrap_or_default()) };
        println!("  {sev} {at}{} [{}]", f.message, f.kind);
    }
}

pub fn run(ctx: &Ctx, c: SyncCmd) -> Result<i32> {
    let conn = ctx.db()?;
    let me = host::register(&conn, HostKind::Cli)?;
    let result = run_inner(ctx, &conn, &me.id, c);
    host::shutdown(&conn, &me.id)?;
    result
}

fn run_inner(ctx: &Ctx, conn: &nexus_core::db::Connection, host_id: &str, c: SyncCmd) -> Result<i32> {
    if let SyncCmd::Clone { url, folder } = &c {
        let (ws, r) = ops::clone_workspace(conn, &ctx.data_dir, host_id, url, folder)?;
        if ctx.json {
            out::json(&serde_json::json!({ "workspace": ws, "restore": r }));
        } else {
            println!("workspace {} opened from Git at {}", ws.name, ws.root_path.display());
            for s in &r.steps {
                println!("  next: {s}");
            }
        }
        return Ok(0);
    }
    let ws = active_workspace(conn)?;
    let s = Sync { conn, ws: &ws, data_dir: &ctx.data_dir, home: &ctx.home, host_id };
    match c {
        SyncCmd::Status => {
            let st = sync::status(conn, &ws, &ctx.data_dir)?;
            if ctx.json {
                out::json(&st);
                return Ok(0);
            }
            let r = &st.readiness;
            println!("{} — {}", ws.name, st.state);
            if r.repo {
                println!(
                    "  branch {} · upstream {} · remote {} · last fetch {}",
                    r.branch.as_deref().unwrap_or("–"),
                    r.upstream.as_deref().unwrap_or("none"),
                    r.remote.as_deref().unwrap_or("none"),
                    out::ago(st.last_fetch_at)
                );
                commits("incoming", &st.incoming);
                commits("outgoing", &st.outgoing);
                if !st.working.is_empty() {
                    println!("local changes ({}):", st.working.len());
                    for w in st.working.iter().take(20) {
                        let flag = w.findings.iter().map(|f| f.kind).collect::<Vec<_>>().join(", ");
                        println!("  {} {}{}", w.status, w.path, if flag.is_empty() { String::new() } else { format!("  [{flag}]") });
                    }
                }
            }
            if let Some(p) = &st.in_progress {
                println!("  attention: {p}");
            }
            for i in &r.issues {
                println!("  {}: {}", i.severity, i.message);
            }
        }
        SyncCmd::Fetch => {
            let f = ops::fetch(&s)?;
            if ctx.json { out::json(&f) } else { println!("fetched from {}: {} incoming, {} outgoing commit(s)", f.remote, f.incoming, f.outgoing) }
        }
        SyncCmd::Pull { no_fetch } => {
            if !no_fetch {
                ops::fetch(&s)?;
            }
            let p = ops::pull(&s)?;
            if ctx.json {
                out::json(&p);
                return Ok(0);
            }
            if p.commits == 0 {
                println!("already up to date");
            } else {
                println!("fast-forwarded {} commit(s), {} file(s)", p.commits, p.files.len());
            }
            for n in &p.needs_approval {
                println!("  needs approval: {} “{}” (`nexus knowledge approve {}` after reviewing it)", n.kind, n.title, n.id);
            }
            for k in &p.kits_changed {
                println!("  AI Kit changed: {k} — review it again under `nexus kits`");
            }
            for w in &p.warnings {
                println!("  warning: {}{}", if w.path.is_empty() { String::new() } else { format!("{}: ", w.path) }, w.message);
            }
        }
        SyncCmd::Diff { path, incoming, outgoing } => {
            let side = if incoming { Side::Incoming } else if outgoing { Side::Outgoing } else { Side::Working };
            let d = ops::file_diff(&ws, side, &path)?;
            if ctx.json {
                out::json(&d);
            } else if d.binary {
                println!("{path}: binary file");
            } else {
                print!("{}", d.text);
            }
        }
        SyncCmd::Review { paths } => {
            let sel = (!paths.is_empty()).then_some(paths.as_slice());
            let r = ops::review_commit(&s, sel)?;
            if ctx.json {
                out::json(&r);
                return Ok(if r.inspection.blocked { 1 } else { 0 });
            }
            if r.files.is_empty() {
                println!("nothing to commit");
                return Ok(0);
            }
            println!("a commit on {} would contain {} file(s):", r.branch.as_deref().unwrap_or("–"), r.files.len());
            for f in &r.files {
                println!("  {} {}", f.status, f.path);
            }
            print_inspection(&r.inspection);
            if !r.identity {
                println!("Git has no name and e-mail for commits — set them with `git config user.name` / `user.email`");
            }
            if r.inspection.blocked {
                println!("blocked — remove or exclude the blocking files (`--path …`) and review again");
                return Ok(1);
            }
            let sel_args = paths.iter().map(|p| format!(" --path \"{p}\"")).collect::<String>();
            println!("to commit exactly this: nexus sync commit -m \"<message>\" --confirm {}{sel_args}", r.token);
        }
        SyncCmd::Commit { message, confirm, paths } => {
            let sel = (!paths.is_empty()).then_some(paths.as_slice());
            let c = ops::commit(&s, &message, sel, &confirm)?;
            if ctx.json { out::json(&c) } else { println!("committed {} file(s) as {} — nothing was pushed", c.files, &c.oid[..c.oid.len().min(8)]) }
        }
        SyncCmd::Push { confirm } => match confirm {
            None => {
                let r = ops::review_push(&s)?;
                if ctx.json {
                    out::json(&r);
                    return Ok(if r.inspection.blocked { 1 } else { 0 });
                }
                if r.commits.is_empty() {
                    println!("nothing to push");
                    return Ok(0);
                }
                println!("a push to {}/{} would send {} commit(s){}:", r.remote, r.branch, r.commits.len(), if r.first_push { " — the whole history (first push)" } else { "" });
                commits("commits", &r.commits);
                files("files", &r.files);
                print_inspection(&r.inspection);
                if r.behind > 0 {
                    println!("the remote has {} commit(s) you do not have — pull first (NEXUS never force-pushes)", r.behind);
                    return Ok(1);
                }
                if r.inspection.blocked {
                    println!("blocked — fix the history in your Git tool before pushing");
                    return Ok(1);
                }
                println!("to push exactly this: nexus sync push --confirm {}", r.token);
            }
            Some(token) => {
                let p = ops::push(&s, &token)?;
                if ctx.json { out::json(&p) } else { println!("pushed {} commit(s) to {}/{}", p.commits, p.remote, p.branch) }
            }
        },
        SyncCmd::History => {
            let list = history::list(conn, &ws.id, 30)?;
            if ctx.json {
                out::json(&list);
                return Ok(0);
            }
            let rows: Vec<Vec<String>> = list
                .iter()
                .map(|h| vec![out::ago(Some(h.started_at)), h.op.clone(), h.status.clone(), h.error_kind.clone().unwrap_or_default(), h.message.clone().unwrap_or_default()])
                .collect();
            out::table(&["WHEN", "OPERATION", "STATUS", "KIND", "DETAIL"], &rows);
        }
        SyncCmd::Restore => {
            let r = ops::restore(conn, &ctx.data_dir, &ws)?;
            if ctx.json {
                out::json(&r);
                return Ok(0);
            }
            println!("restoring {} on this machine:", r.name);
            for s in &r.steps {
                println!("  - {s}");
            }
        }
        SyncCmd::Clone { .. } => return Err(CoreError::invalid("unreachable")),
    }
    Ok(0)
}
