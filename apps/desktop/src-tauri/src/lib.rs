//! NEXUS Desktop: Tauri shell around `nexus-core` with an integrated PTY terminal.

pub mod commands;
pub mod error;
pub mod pty;
pub mod shells;
pub mod state;

use std::sync::Arc;

use tauri::{Manager, RunEvent};

use nexus_core::{db, paths, session};

use crate::state::{AppCtx, Ctx};

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env("NEXUS_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).with_target(false).try_init();
}

fn open_context() -> Result<AppCtx, Box<dyn std::error::Error>> {
    let data_dir = paths::data_dir()?;
    let db_path = data_dir.join(db::DB_FILE);
    let conn = db::open(&db_path)?;
    let interrupted = session::reconcile_on_startup(&conn)?;
    if interrupted > 0 {
        tracing::info!(interrupted, "sessions from a previous run are no longer attached");
    }
    let home = paths::home_dir()?;
    tracing::info!(data_dir = %data_dir.display(), "NEXUS data directory");
    Ok(AppCtx::new(conn, home, data_dir, db_path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    let app = tauri::Builder::default()
        // Must be first: a second launch focuses the existing window instead of opening the same database.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let ctx: Ctx = Arc::new(open_context()?);
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
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the NEXUS application");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Children must not outlive NEXUS invisibly. Conversations are persisted by Claude itself and
            // the sessions show as "interrupted" (resumable) on the next start.
            if let Some(ctx) = handle.try_state::<Ctx>() {
                ctx.pty.terminate_all();
            }
        }
    });
}
