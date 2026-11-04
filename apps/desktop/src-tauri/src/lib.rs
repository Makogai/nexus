//! NEXUS Desktop: Tauri shell around `nexus-core` with an integrated PTY terminal.

pub mod commands;
pub mod error;
pub mod notifier;
pub mod pty;
pub mod shells;
pub mod state;
pub mod webview_guard;

use std::sync::Arc;

use tauri::{Manager, RunEvent};

use nexus_core::host::{self, HostKind};
use nexus_core::{db, now_ms, paths};

use crate::state::{AppCtx, Ctx};

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("NEXUS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init();
}

fn open_context() -> Result<AppCtx, Box<dyn std::error::Error>> {
    let data_dir = paths::data_dir()?;
    let db_path = data_dir.join(db::DB_FILE);
    let (conn, recovery) = db::open_or_recover(&db_path)?;
    // Single instance: any earlier desktop host is dead; its live sessions become interrupted (ADR-0014).
    let me = host::register(&conn, HostKind::Desktop)?;
    host::stop_others(&conn, HostKind::Desktop, &me.id)?;
    let interrupted = host::reconcile(&conn, now_ms())?;
    if interrupted > 0 {
        tracing::info!(interrupted, "sessions whose host is gone are no longer attached");
    }
    let home = paths::home_dir()?;
    tracing::info!(data_dir = %data_dir.display(), "NEXUS data directory");
    Ok(AppCtx::new(conn, home, data_dir, db_path, me, recovery))
}

/// Keep this host's lease fresh and reconcile sessions of hosts that died (e.g. a crashed `nexus launch`).
fn spawn_heartbeat(ctx: Ctx) {
    let _ = std::thread::Builder::new().name("host-heartbeat".into()).spawn(move || {
        loop {
            std::thread::sleep(host::HEARTBEAT_INTERVAL);
            let running = ctx.pty.running_ids();
            let db = ctx.db();
            if let Err(e) = host::heartbeat(&db, &ctx.host.id, &running).and_then(|_| host::reconcile(&db, now_ms())) {
                tracing::warn!(error = %e, "host heartbeat failed");
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    if let Some(r) = webview_guard::report() {
        if !r.removed.is_empty() {
            tracing::warn!(removed = ?r.removed, "ignored webview override variables (production build)");
        }
        if r.mode != webview_guard::Mode::Production {
            tracing::info!(mode = ?r.mode, "webview debugging overrides are honoured (not a production build)");
        }
    }
    let app = tauri::Builder::default()
        // Must be first: a second launch focuses the existing window instead of opening the same database.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(not(windows))]
    let app = app.plugin(tauri_plugin_notification::init());
    let app = app
        .setup(|app| {
            let ctx: Ctx = Arc::new(open_context()?);
            spawn_heartbeat(ctx.clone());
            notifier::spawn_poller(app.handle().clone(), ctx.clone());
            app.manage(ctx);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::ui_preferences_get,
            commands::app::ui_preferences_set,
            commands::app::db_data_version,
            commands::workspaces::workspace_list,
            commands::workspaces::workspace_active,
            commands::workspaces::workspace_create,
            commands::workspaces::workspace_import,
            commands::workspaces::workspace_activate,
            commands::workspaces::workspace_forget,
            commands::projects::project_list,
            commands::projects::project_get,
            commands::projects::project_inspect_path,
            commands::projects::project_register,
            commands::projects::project_unregister,
            commands::projects::project_locate,
            commands::projects::project_refresh,
            commands::projects::project_git_status,
            commands::projects::project_checkpoints,
            commands::claude::claude_installations,
            commands::claude::profile_list,
            commands::claude::profile_detect,
            commands::claude::profile_register,
            commands::claude::profile_create,
            commands::claude::profile_update,
            commands::claude::profile_set_archived,
            commands::claude::profile_forget,
            commands::claude::profile_details,
            commands::claude::profile_auth_status,
            commands::claude::launch_options,
            commands::sessions::session_list,
            commands::sessions::session_get,
            commands::sessions::session_start_claude,
            commands::sessions::session_resume,
            commands::sessions::shell_list,
            commands::sessions::session_open_shell,
            commands::sessions::session_attach,
            commands::sessions::session_write,
            commands::sessions::session_resize,
            commands::sessions::session_interrupt,
            commands::sessions::session_terminate,
            commands::sessions::session_release,
            commands::sessions::session_rename,
            commands::sessions::session_set_archived,
            commands::knowledge::knowledge_overview,
            commands::knowledge::knowledge_read,
            commands::knowledge::knowledge_search,
            commands::knowledge::knowledge_create,
            commands::knowledge::knowledge_update_task,
            commands::knowledge::knowledge_approve,
            commands::knowledge::knowledge_task,
            commands::knowledge::project_knowledge,
            commands::knowledge::instructions_inspect,
            commands::knowledge::project_resume,
            commands::knowledge::knowledge_proposals,
            commands::knowledge::knowledge_proposal_approve,
            commands::knowledge::knowledge_proposal_reject,
            commands::kits::profile_diagnostics,
            commands::kits::kit_list,
            commands::kits::kit_register,
            commands::kits::kit_approve,
            commands::kits::kit_update,
            commands::kits::kit_remove,
            commands::kits::kit_enable,
            commands::kits::kit_disable,
            commands::kits::kit_validate,
            commands::context::context_get,
            commands::context::context_set,
            commands::context::context_presets,
            commands::context::context_analyze,
            commands::context::context_docs,
            commands::context::context_inspect,
            commands::constitutions::constitution_list,
            commands::constitutions::constitution_get,
            commands::constitutions::constitution_create,
            commands::constitutions::constitution_save,
            commands::constitutions::constitution_save_raw,
            commands::constitutions::constitution_effective,
            commands::constitutions::constitution_search,
            commands::constitutions::constitution_history,
            commands::constitutions::constitution_version,
            commands::notifications::notifications_list,
            commands::notifications::notifications_counts,
            commands::notifications::notifications_mark_read,
            commands::notifications::notifications_dismiss,
            commands::notifications::notifications_snooze,
            commands::notifications::notifications_prefs,
            commands::notifications::notifications_set_prefs,
            commands::notifications::notifications_set_view,
            commands::notifications::notifications_availability,
            commands::notifications::notifications_test,
            commands::notifications::notification_target,
            commands::notifications::e2e_notifications_recorded,
            commands::notifications::e2e_focus_state,
            commands::notifications::e2e_notification_activate,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the NEXUS application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Safe shutdown (ADR-0014): record live sessions as interrupted (final), stop the host, then end the
            // children NEXUS started — they must not outlive it invisibly. Claude keeps the conversations.
            if let Some(ctx) = handle.try_state::<Ctx>() {
                if let Err(e) = host::shutdown(&ctx.db(), &ctx.host.id) {
                    tracing::error!(error = %e, "could not record shutdown");
                }
                ctx.pty.terminate_all();
            }
        }
    });
}
