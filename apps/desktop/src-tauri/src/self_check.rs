//! `nexus-desktop --self-check <report.json>`: verify an installed NEXUS without a window or developer tools
//! (ADR-0033) — where the CLI is and that it runs, that webview debugging is off, which data folder and database
//! schema are in use, and the notification sender identity. Read-only: the database is opened read-only and nothing
//! is migrated, registered or written except the report.

use std::path::Path;
use std::time::Duration;

use serde_json::{Value, json};

use nexus_core::{db, paths, proc};

pub const FLAG: &str = "--self-check";

/// `Some(path)` when the desktop was started as `--self-check <path>`.
pub fn requested() -> Option<std::path::PathBuf> {
    let mut args = std::env::args_os().skip(1);
    while let Some(a) = args.next() {
        if a == FLAG {
            return args.next().map(Into::into);
        }
    }
    None
}

fn cli() -> Value {
    let Some(path) = nexus_core::claude::mcp::nexus_next_to_current_exe() else {
        return json!({ "found": false });
    };
    let mut cmd = proc::command(&path);
    cmd.arg("--version");
    let version = proc::run(cmd, Duration::from_secs(10)).ok().filter(|o| o.success()).map(|o| o.stdout_str().trim().to_string());
    json!({ "found": true, "path": path, "version": version })
}

fn database(dir: &Path) -> Value {
    let file = dir.join(db::DB_FILE);
    if !file.is_file() {
        return json!({ "present": false, "supportedSchema": db::SCHEMA_VERSION });
    }
    match db::peek(&file) {
        Ok((v, app)) => json!({ "present": true, "schema": v, "supportedSchema": db::SCHEMA_VERSION, "nexus": app == db::APPLICATION_ID || app == 0, "compatible": v <= db::SCHEMA_VERSION }),
        Err(e) => json!({ "present": true, "error": e.to_string() }),
    }
}

pub fn run(out: &Path) -> i32 {
    let exe = std::env::current_exe().ok();
    let data_dir = paths::data_dir().ok();
    let install_dir = exe.as_ref().and_then(|e| e.parent()).map(|p| p.to_path_buf());
    let data_inside_install = match (&data_dir, &install_dir) {
        (Some(d), Some(i)) => std::fs::canonicalize(d).unwrap_or(d.clone()).starts_with(std::fs::canonicalize(i).unwrap_or(i.clone())),
        _ => false,
    };
    #[cfg(windows)]
    let notifier = {
        let id = crate::notifier::windows_toast::identity("dev.nexus.desktop");
        json!({ "aumid": id.aumid, "display": id.display, "protocol": id.protocol })
    };
    #[cfg(not(windows))]
    let notifier = json!(null);
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "exe": exe,
        "installDir": install_dir,
        "debugAssertions": cfg!(debug_assertions),
        "e2eWebviewDebugging": cfg!(feature = "e2e-webview-debugging"),
        "webviewArgumentsAfterHardening": std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").ok(),
        "cli": cli(),
        "dataDir": data_dir,
        "dataInsideInstallDir": data_inside_install,
        "database": data_dir.as_deref().map(database),
        "backups": data_dir.as_deref().map(|d| db::backup::list(d).map(|l| l.len()).unwrap_or(0)),
        "notifier": notifier,
    });
    match std::fs::write(out, serde_json::to_vec_pretty(&report).unwrap_or_default()) {
        Ok(()) => 0,
        Err(_) => 2,
    }
}
