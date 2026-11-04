//! `nexus hook --session <id> <event>`: the lifecycle hook handler Claude runs from the session-scoped `--settings`
//! (ADR-0016). It must never disturb Claude: it always exits 0, reads at most 1 MiB of payload, opens the database
//! without integrity checks or migrations, and prints output only for SessionStart (bounded recovery context).

use std::io::Read;

use serde_json::{Value, json};

use nexus_core::activity::{self, HookEvent};
use nexus_core::db;

use crate::Ctx;

const MAX_PAYLOAD: u64 = 1024 * 1024;

pub fn run(ctx: &Ctx, session: &str, event: &str) -> i32 {
    if let Err(e) = handle(ctx, session, event) {
        // stderr of a successful hook is only shown in Claude's debug output.
        eprintln!("nexus hook {event}: {e}");
    }
    0
}

fn handle(ctx: &Ctx, session: &str, event: &str) -> Result<(), String> {
    let event = HookEvent::parse(event).ok_or_else(|| format!("unknown hook event '{event}'"))?;
    let mut raw = String::new();
    std::io::stdin().take(MAX_PAYLOAD).read_to_string(&mut raw).map_err(|e| e.to_string())?;
    let payload: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
    let Some(conn) = db::open_existing(&ctx.data_dir.join(db::DB_FILE)).map_err(|e| e.to_string())? else {
        return Ok(()); // no NEXUS database on this schema: nothing to record
    };
    let outcome = activity::handle(&conn, session, event, &payload).map_err(|e| e.to_string())?;
    // Live cross-profile instruction bleed (threat T21): needs the home folder, which only the hook process knows.
    if let (HookEvent::InstructionsLoaded, Some(path)) = (event, payload.get("file_path").and_then(Value::as_str)) {
        let _ = nexus_core::context::inspector::check_bleed(&conn, session, path, &ctx.home);
    }
    if let (HookEvent::SessionStart, Some(context)) = (event, outcome.additional_context) {
        let out = json!({ "hookSpecificOutput": { "hookEventName": "SessionStart", "additionalContext": context } });
        println!("{out}");
    }
    Ok(())
}
