use std::collections::HashSet;
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
    /// A restore scheduled from Settings and applied at this start: `Ok(backup id)` or the reason it failed.
    pub restore: Option<Result<String, String>>,
    /// Where the user is looking, for notification decisions (ADR-0025).
    pub attention: AttentionState,
    pub notifier: Box<dyn crate::notifier::Notifier>,
    /// The last Dev Environment inventory (ADR-0051 §4): when it was taken and what was found. Memory only.
    inventory: Mutex<Option<Inventory>>,
}

type Inventory = (i64, Vec<nexus_core::toolchain::Installed>);

#[derive(Default)]
pub struct AttentionState {
    stopped: Mutex<HashSet<String>>,
    view: Mutex<ViewFocus>,
    /// A notification click that started this process (`nexus://notification/<id>`), until the UI takes it.
    activation: Mutex<Option<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct ViewFocus {
    /// The session whose terminal is visible in the NEXUS window, if any.
    pub visible_session: Option<String>,
    /// `Date.getTimezoneOffset()` of the UI (minutes, UTC − local) — for quiet hours.
    pub utc_offset_minutes: i64,
}

impl AttentionState {
    pub fn mark_stopped(&self, id: &str) {
        self.stopped.lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string());
    }
    pub fn take_stopped(&self, id: &str) -> bool {
        self.stopped.lock().unwrap_or_else(|p| p.into_inner()).remove(id)
    }
    pub fn set_view(&self, v: ViewFocus) {
        *self.view.lock().unwrap_or_else(|p| p.into_inner()) = v;
    }
    pub fn view(&self) -> ViewFocus {
        self.view.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    pub fn set_activation(&self, id: String) {
        *self.activation.lock().unwrap_or_else(|p| p.into_inner()) = Some(id);
    }
    pub fn take_activation(&self) -> Option<String> {
        self.activation.lock().unwrap_or_else(|p| p.into_inner()).take()
    }
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
        restore: Option<Result<String, String>>,
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
            restore,
            attention: AttentionState::default(),
            notifier: crate::notifier::select(),
            inventory: Mutex::new(None),
        }
    }

    pub fn inventory(&self) -> Option<Inventory> {
        self.inventory.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set_inventory(&self, at: i64, tools: Vec<nexus_core::toolchain::Installed>) -> Inventory {
        let v = (at, tools);
        *self.inventory.lock().unwrap_or_else(|p| p.into_inner()) = Some(v.clone());
        v
    }

    /// The UI's database connection. Never hold it across an `.await` or a slow external call.
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }
}
