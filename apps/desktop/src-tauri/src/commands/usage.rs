//! Usage dashboard (ADR-0030): token usage read from Claude Code's local session transcripts. The desktop scans in
//! the background ([`spawn_scanner`]); reports are read-only queries. Telemetry settings are only reported.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::usage::{self, report};

use crate::error::{IpcResult, blocking};
use crate::state::Ctx;

pub const USAGE_EVENT: &str = "nexus://usage";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageView {
    pub report: report::Report,
    pub telemetry: Vec<report::TelemetryNote>,
}

#[tauri::command]
pub async fn usage_report(ctx: State<'_, Ctx>, filter: report::Filter, by: report::Dimension) -> IpcResult<UsageView> {
    let ctx = ctx.inner().clone();
    blocking(move || {
        let db = ctx.db();
        Ok(UsageView { report: report::report(&db, &filter, by)?, telemetry: report::telemetry(&db, &ctx.home, &MachinePolicyLocations::system())? })
    })
    .await
}

#[tauri::command]
pub async fn usage_sessions(ctx: State<'_, Ctx>, filter: report::Filter, limit: Option<usize>) -> IpcResult<Vec<report::SessionUsage>> {
    let ctx = ctx.inner().clone();
    blocking(move || Ok(report::sessions(&ctx.db(), &filter, limit.unwrap_or(100))?)).await
}

/// Read new transcript data now (the scanner also does this every few seconds).
#[tauri::command]
pub async fn usage_scan(app: AppHandle, ctx: State<'_, Ctx>) -> IpcResult<usize> {
    let ctx = ctx.inner().clone();
    let n = blocking(move || Ok(usage::scan_due(&ctx.db(), &ctx.home, 50)?.messages)).await?;
    let _ = app.emit(USAGE_EVENT, ());
    Ok(n)
}

/// Background scanner: live sessions, sessions that just ended, never-scanned ones — a few per tick.
pub fn spawn_scanner(app: AppHandle, ctx: Ctx) {
    let _ = std::thread::Builder::new().name("usage-scanner".into()).spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(8));
            match usage::scan_due(&ctx.db(), &ctx.home, 4) {
                Ok(s) if s.messages > 0 || s.finished > 0 => {
                    let _ = app.emit(USAGE_EVENT, ());
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("usage scan: {e}"),
            }
        }
    });
}
