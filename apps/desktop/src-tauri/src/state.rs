use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use nexus_core::claude::policy::MachinePolicyLocations;
use nexus_core::db::{Connection, Recovery};
use nexus_core::host::Host;

use crate::pty::PtyManager;

pub struct AppCtx {
    db: Mutex<Connection>,
    pub pty: PtyManager,
    pub home: PathBuf,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub machine_policy: MachinePolicyLocations,
    /// This process's session-host registration (ADR-0014).
    pub host: Host,
    /// Set when the database was unreadable at startup and has been moved aside.
    pub recovery: Option<Recovery>,
}

pub type Ctx = Arc<AppCtx>;

impl AppCtx {
    pub fn new(
        conn: Connection,
        home: PathBuf,
        data_dir: PathBuf,
        db_path: PathBuf,
        host: Host,
        recovery: Option<Recovery>,
    ) -> Self {
        AppCtx {
            db: Mutex::new(conn),
            pty: PtyManager::default(),
            home,
            data_dir,
            db_path,
            machine_policy: MachinePolicyLocations::system(),
            host,
            recovery,
        }
    }

    /// The UI's database connection. Never hold it across an `.await` or a slow external call.
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }
}
