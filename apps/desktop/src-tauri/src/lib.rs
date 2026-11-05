//! NEXUS Desktop: Tauri shell around `nexus-core` with an integrated PTY terminal.

// The test-only WebView2 debugging feature can never be part of a release build (threat T18, ADR-0015).
#[cfg(all(feature = "e2e-webview-debugging", not(debug_assertions)))]
compile_error!("the `e2e-webview-debugging` feature is for debug test builds only and cannot be compiled into a release build");

pub mod commands;
pub mod self_check;
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
    // A restore scheduled in Settings is applied before the database is opened (ADR-0032).
    let restore = db::backup::apply_pending(&data_dir).map(|r| r.map(|x| x.restored).map_err(|e| e.to_string()));
    let (conn, recovery) = db::open_or_recover(&db_path)?;
    // At most one automatic backup a day (the newest 7 are kept).
    if let Err(e) = db::backup::automatic_if_due(&conn, &data_dir) {
        tracing::warn!(error = %e, "automatic backup failed");
    }
    // Single instance: any earlier desktop host is dead; its live sessions become interrupted (ADR-0014).
    let me = host::register(&conn, HostKind::Desktop)?;
    host::stop_others(&conn, HostKind::Desktop, &me.id)?;
    let interrupted = host::reconcile(&conn, now_ms())?;
    // Gate executions of a crashed host are interrupted and verified again (ADR-0029).
    let _ = nexus_core::workflow::run::reconcile(&conn, now_ms());
    // Sync operations of a crashed host are interrupted (ADR-0031).
    let _ = nexus_core::workspace_sync::history::reconcile(&conn, now_ms());
    if interrupted > 0 {
        tracing::info!(interrupted, "sessions whose host is gone are no longer attached");
    }
    let home = paths::home_dir()?;
    tracing::info!(data_dir = %data_dir.display(), "NEXUS data directory");
    Ok(AppCtx::new(conn, home, data_dir, db_path, me, recovery, restore))
}

/// Keep this host's lease fresh and reconcile sessions of hosts that died (e.g. a crashed `nexus launch`).
fn spawn_heartbeat(ctx: Ctx) {
    let _ = std::thread::Builder::new().name("host-heartbeat".into()).spawn(move || {
        loop {
            std::thread::sleep(host::HEARTBEAT_INTERVAL);
            let running = ctx.pty.running_ids();
            let db = ctx.db();
            if let Err(e) = host::heartbeat(&db, &ctx.host.id, &running)
                .and_then(|_| host::reconcile(&db, now_ms()))
                .and_then(|_| nexus_core::workflow::run::reconcile(&db, now_ms()))
                .and_then(|_| nexus_core::workspace_sync::history::reconcile(&db, now_ms()))
            {
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
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // A notification click (`nexus://notification/<id>`) or a second start: focus this window.
            match argv.iter().find_map(|a| notifier::parse_activation(a)) {
                Some(id) => notifier::activate(app, &id),
                None => {
                    if let Some(w) = app.get_webview_window("main") {
                        let _ = w.unminimize();
                        let _ = w.set_focus();
                    }
                }
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());
    #[cfg(not(windows))]
    let app = app.plugin(tauri_plugin_notification::init());
    let app = app
        .setup(|app| {
            let ctx: Ctx = match open_context() {
                Ok(c) => Arc::new(c),
                // Never start with an empty database instead: explain, keep everything, stop.
                Err(e) => {
                    let data = paths::data_dir().map(|d| d.display().to_string()).unwrap_or_default();
                    webview_guard::refuse_to_start(&format!(
                        "NEXUS could not open its database:\n\n{e}\n\nNothing was deleted. Your data is in {data} \
                         (backups in its \"backups\" folder). See the troubleshooting guide, or restore a backup with \
                         `nexus backup list` / `nexus backup restore <id>`."
                    ));
                }
            };
            if let Some(id) = std::env::args().find_map(|a| notifier::parse_activation(&a)) {
                ctx.attention.set_activation(id);
            }
            spawn_heartbeat(ctx.clone());
            notifier::spawn_poller(app.handle().clone(), ctx.clone());
            commands::workflows::spawn_runner(app.handle().clone(), ctx.clone());
            commands::usage::spawn_scanner(app.handle().clone(), ctx.clone());
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
            commands::workspaces::workspace_migration_plan,
            commands::workspaces::workspace_migrate,
            commands::projects::project_scan,
            commands::projects::project_clone,
            commands::projects::project_context_init,
            commands::workspaces::workspace_doctor,
            commands::workspaces::workspace_repair_plan,
            commands::workspaces::workspace_repair,
            commands::workspaces::template_list,
            commands::workspaces::template_plan,
            commands::workspaces::template_apply,
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
            commands::workflows::workflow_templates,
            commands::workflows::workflow_list,
            commands::workflows::workflow_get,
            commands::workflows::workflow_create,
            commands::workflows::workflow_save,
            commands::workflows::workflow_save_raw,
            commands::workflows::workflow_start,
            commands::workflows::workflow_runs,
            commands::workflows::workflow_run,
            commands::workflows::workflow_review,
            commands::workflows::workflow_cancel,
            commands::workflows::workflow_retry,
            commands::workflows::gates_list,
            commands::workflows::gate_upsert,
            commands::sync::sync_status,
            commands::sync::sync_fetch,
            commands::sync::sync_pull,
            commands::sync::sync_diff,
            commands::sync::sync_review_commit,
            commands::sync::sync_commit,
            commands::sync::sync_review_push,
            commands::sync::sync_push,
            commands::sync::sync_clone,
            commands::sync::sync_restore,
            commands::app::backup_list,
            commands::app::backup_create,
            commands::app::backup_schedule_restore,
            commands::app::backup_cancel_restore,
            commands::app::backup_delete,
            commands::app::app_restart,
            commands::usage::usage_report,
            commands::usage::usage_sessions,
            commands::usage::usage_scan,
            commands::workflows::gate_approve,
            commands::workflows::gate_approval_impact,
            commands::workflows::gate_revoke,
            commands::workflows::gate_remove,
            commands::workflows::gate_suggest,
            commands::workflows::gate_run,
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
            commands::notifications::e2e_notifications_withdrawn,
            commands::notifications::notifications_take_activation,
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
