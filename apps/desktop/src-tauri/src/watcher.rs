//! Knowledge files change outside NEXUS — in an editor, by `git pull`, by Claude (ADR-0049). The desktop watches the
//! knowledge roots of the active workspace, lets a burst of changes settle, refreshes the index on its own connection (a
//! scan never holds the connection the UI uses) and tells the UI when the index changed. The refreshes on window focus
//! and after a session ends (ADR-0048) stay as the fallback for folders whose changes are not reported.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use nexus_core::db::{self, Connection};
use nexus_core::knowledge::service as knowledge;
use nexus_core::knowledge::{Root, roots, watch};
use nexus_core::workspace::{self, Workspace};

use crate::state::Ctx;

/// The knowledge index changed because files changed on disk; payload [`Changed`].
pub const KNOWLEDGE_EVENT: &str = "nexus://knowledge";
/// A burst of changes (an editor's save, a `git pull`) has settled when nothing relevant happened for this long …
const QUIET: Duration = Duration::from_millis(400);
/// … but the index is refreshed at the latest this long after the first change.
const MAX_WAIT: Duration = Duration::from_secs(2);
/// How often the active workspace and the existence of its roots are looked at again.
const RECHECK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub workspace_id: String,
    pub changed: usize,
    pub removed: usize,
}

/// The operating system's watcher over the folders of a set of knowledge roots.
pub struct Folders {
    watcher: RecommendedWatcher,
    events: Receiver<notify::Result<notify::Event>>,
    roots: Vec<Root>,
    watched: Vec<PathBuf>,
    /// The watched folders under their real names: macOS reports changes by those (`watch::real_names`).
    real_names: Vec<(PathBuf, PathBuf)>,
}

impl Folders {
    pub fn new() -> notify::Result<Self> {
        let (tx, events) = channel();
        let watcher = notify::recommended_watcher(tx)?;
        Ok(Folders { watcher, events, roots: Vec::new(), watched: Vec::new(), real_names: Vec::new() })
    }

    /// Watch the folders of `roots` and nothing else. True when what is watched changed (a workspace was activated, a
    /// root appeared or went away). A folder that cannot be watched is tried again only when the set changes.
    pub fn arm(&mut self, roots: Vec<Root>) -> bool {
        let want = watch::targets(&roots);
        self.roots = roots;
        if want == self.watched {
            return false;
        }
        for dir in self.watched.iter().filter(|d| !want.contains(d)) {
            let _ = self.watcher.unwatch(dir); // it may have gone with its folder
        }
        for dir in want.iter().filter(|d| !self.watched.contains(d)) {
            if let Err(e) = self.watcher.watch(dir, RecursiveMode::Recursive) {
                tracing::warn!(error = %e, dir = %dir.display(), "knowledge folder not watched; it refreshes on focus");
            }
        }
        self.real_names = watch::real_names(&want);
        self.watched = want;
        true
    }

    /// Wait up to `wait` for a change that reaches the index, then until the burst settles. True when there was one.
    pub fn next_change(&self, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        loop {
            match self.events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(ev) if self.relevant(&ev) => break,
                Ok(_) => {}
                Err(RecvTimeoutError::Timeout) => return false,
                Err(RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                    return false;
                }
            }
        }
        let first = Instant::now();
        let mut last = first;
        loop {
            let (settled, now) = ((last + QUIET).min(first + MAX_WAIT), Instant::now());
            if now >= settled {
                return true;
            }
            match self.events.recv_timeout(settled - now) {
                Ok(ev) if self.relevant(&ev) => last = Instant::now(),
                Ok(_) => {}
                Err(_) => return true,
            }
        }
    }

    fn relevant(&self, ev: &notify::Result<notify::Event>) -> bool {
        match ev {
            // The watcher lost track (e.g. its buffer overflowed during a large checkout): look at the files again.
            Err(_) => true,
            Ok(e) if e.need_rescan() => true,
            Ok(e) => {
                let moved = matches!(e.kind, EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(_)));
                !matches!(e.kind, EventKind::Access(_)) && e.paths.iter().any(|p| self.affects_index(p, moved))
            }
        }
    }

    fn affects_index(&self, path: &Path, moved: bool) -> bool {
        watch::affects_index(&self.roots, path, moved)
            || watch::as_watched(&self.real_names, path).is_some_and(|p| watch::affects_index(&self.roots, &p, moved))
    }
}

/// Watch the active workspace's knowledge for as long as the app runs.
pub fn spawn(app: AppHandle, ctx: Ctx) {
    let _ = std::thread::Builder::new().name("knowledge-watcher".into()).spawn(move || {
        let mut folders = match Folders::new() {
            Ok(f) => f,
            Err(e) => return tracing::warn!(error = %e, "knowledge files are not watched; they refresh on focus"),
        };
        let mut conn: Option<Connection> = None;
        let mut active: Option<Workspace> = None;
        let mut checked: Option<Instant> = None;
        loop {
            if checked.is_none_or(|t| t.elapsed() >= RECHECK) {
                checked = Some(Instant::now());
                active = with_conn(&ctx, &mut conn, workspace::active).flatten().filter(|w| w.available);
                let roots = active.as_ref().map(|w| roots(&w.id, &w.root_path, &ctx.data_dir)).unwrap_or_default();
                // Newly watched folders may have changed while nobody watched them.
                if folders.arm(roots) {
                    refresh(&app, &ctx, &mut conn, active.as_ref());
                }
            }
            let left = RECHECK.saturating_sub(checked.map(|t| t.elapsed()).unwrap_or_default());
            if folders.next_change(left) {
                refresh(&app, &ctx, &mut conn, active.as_ref());
            }
        }
    });
}

fn refresh(app: &AppHandle, ctx: &Ctx, conn: &mut Option<Connection>, ws: Option<&Workspace>) {
    let Some(ws) = ws else { return };
    if let Some(r) = with_conn(ctx, conn, |c| knowledge::refresh(c, &ctx.data_dir, ws))
        && r.changed + r.removed > 0
    {
        let _ = app.emit(KNOWLEDGE_EVENT, Changed { workspace_id: ws.id.clone(), changed: r.changed, removed: r.removed });
    }
}

/// The watcher's own connection, opened when first needed and kept.
fn with_conn<T>(ctx: &Ctx, conn: &mut Option<Connection>, f: impl FnOnce(&Connection) -> nexus_core::Result<T>) -> Option<T> {
    if conn.is_none() {
        match db::open(&ctx.data_dir.join(db::DB_FILE)) {
            Ok(c) => *conn = Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "knowledge watcher: database not opened");
                return None;
            }
        }
    }
    f(conn.as_ref()?).map_err(|e| tracing::warn!(error = %e, "knowledge watcher: index not refreshed")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Forget what is queued (events of the previous step that arrive late).
    fn drain(f: &Folders) {
        std::thread::sleep(Duration::from_millis(300));
        while f.events.try_recv().is_ok() {}
    }

    /// On macOS the temporary folder is `/var/…` and its changes are reported as `/private/var/…` (`watch::real_names`).
    #[test]
    fn documents_saved_created_moved_or_removed_are_seen_and_nothing_else_is() {
        let t = tempfile::tempdir().unwrap();
        let (ws, data) = (t.path().join("ws"), t.path().join("data"));
        std::fs::create_dir_all(ws.join("tasks")).unwrap();
        let mut f = Folders::new().unwrap();
        assert!(f.arm(roots("w1", &ws, &data)), "the workspace folder is watched");
        assert!(!f.arm(roots("w1", &ws, &data)), "the same roots change nothing");
        assert_eq!(f.watched, vec![ws.clone()]);

        std::fs::write(ws.join("tasks/.a.md.tmp"), "x").unwrap();
        std::fs::create_dir_all(ws.join(".git")).unwrap();
        std::fs::write(ws.join(".git/index"), "x").unwrap();
        std::fs::write(ws.join("tasks/notes.txt"), "x").unwrap();
        std::fs::write(ws.join("nexus-workspace.json"), "{}").unwrap();
        assert!(!f.next_change(Duration::from_millis(1500)), "temp files, .git and other files are not knowledge");

        let started = Instant::now();
        std::fs::write(ws.join("tasks/a.md"), "x").unwrap();
        assert!(f.next_change(Duration::from_secs(10)), "a saved document");
        assert!(started.elapsed() >= QUIET, "…after the burst settled");
        drain(&f);

        std::fs::create_dir_all(ws.join("project-context/app")).unwrap();
        std::fs::write(ws.join("project-context/app/overview.md"), "x").unwrap();
        assert!(f.next_change(Duration::from_secs(10)), "a document in a folder created after watching began");
        drain(&f);

        std::fs::rename(ws.join("project-context/app"), ws.join("project-context/web")).unwrap();
        assert!(f.next_change(Duration::from_secs(10)), "a folder of documents renamed");
        drain(&f);

        // One watch on the workspace folder: the folders inside stay free to be removed (`git checkout`).
        std::fs::remove_dir_all(ws.join("tasks")).unwrap();
        assert!(f.next_change(Duration::from_secs(10)), "a folder of documents removed");
        drain(&f);

        std::fs::create_dir_all(data.join("global")).unwrap();
        assert!(f.arm(roots("w1", &ws, &data)), "the global folder appeared");
        assert_eq!(f.watched, vec![ws.clone(), data.join("global")]);
        assert!(f.arm(Vec::new()) && f.watched.is_empty(), "no active workspace: nothing is watched");
    }
}
