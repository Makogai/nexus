//! Pruning what sessions leave in the data folder (0.5 alpha; a known gap until now). Each launch writes
//! `<data>/sessions/<id>/` (the `--mcp-config` and `--settings` files), and kit overlays live per profile under
//! `<data>/kits/<kit>/overlay/<profile>/`. Hosts prune at start:
//!
//! * a session folder once its session ended more than [`KEEP_MS`] ago — or, without a session row, once the folder
//!   itself is that old. A live session's folder is never touched (Claude reads those files while it runs);
//! * a kit overlay of a profile that no longer exists (nothing can launch with it again).
//!
//! Only these NEXUS-made folders are removed; symlinks are never followed.

use std::path::Path;
use std::time::UNIX_EPOCH;

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::error::Result;

/// Session artifacts are kept this long after the session ended (30 days).
pub const KEEP_MS: i64 = 30 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pruned {
    pub session_dirs: usize,
    pub overlays: usize,
}

fn mtime_ms(p: &Path) -> Option<i64> {
    std::fs::symlink_metadata(p).ok()?.modified().ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as i64)
}

fn real_dirs(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink())).collect()).unwrap_or_default()
}

/// Remove session folders and kit overlays nothing will use again (see the module docs). `now` in Unix ms.
pub fn artifacts(conn: &Connection, data_dir: &Path, now: i64) -> Result<Pruned> {
    let mut out = Pruned::default();
    let cutoff = now - KEEP_MS;
    for e in real_dirs(&data_dir.join("sessions")) {
        let id = e.file_name().to_string_lossy().into_owned();
        let row: Option<(String, Option<i64>)> = conn.query_row("SELECT state, ended_at FROM sessions WHERE id = ?1", [&id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
        let stale = match row {
            Some((state, _)) if state == "starting" || state == "running" => false,
            Some((_, Some(ended))) => ended < cutoff,
            // Ended without a time, or no session row at all: the folder's own age decides.
            _ => mtime_ms(&e.path()).is_some_and(|m| m < cutoff),
        };
        if stale && std::fs::remove_dir_all(e.path()).is_ok() {
            out.session_dirs += 1;
        }
    }
    let profiles: Vec<String> = conn.prepare("SELECT id FROM claude_profiles")?.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    for kit in real_dirs(&data_dir.join("kits")) {
        for overlay in real_dirs(&kit.path().join("overlay")) {
            let profile = overlay.file_name().to_string_lossy().into_owned();
            if !profiles.contains(&profile) && std::fs::remove_dir_all(overlay.path()).is_ok() {
                out.overlays += 1;
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{self, NewSession};

    #[test]
    fn old_session_folders_and_orphaned_overlays_go_live_and_recent_ones_stay() {
        let t = tempfile::tempdir().unwrap();
        let data = t.path();
        let conn = crate::db::open_in_memory().unwrap();
        let mk = |title: &str| session::insert_starting(&conn, NewSession { title: title.into(), cwd: data.to_path_buf(), program: "claude".into(), ..Default::default() }).unwrap();
        let (live, recent, old) = (mk("live"), mk("recent"), mk("old"));
        let now = crate::now_ms();
        conn.execute("UPDATE sessions SET state = 'running' WHERE id = ?1", [&live.id]).unwrap();
        conn.execute("UPDATE sessions SET state = 'exited', ended_at = ?2 WHERE id = ?1", rusqlite::params![recent.id, now - 86_400_000]).unwrap();
        conn.execute("UPDATE sessions SET state = 'exited', ended_at = ?2 WHERE id = ?1", rusqlite::params![old.id, now - KEEP_MS - 1]).unwrap();
        for id in [&live.id, &recent.id, &old.id, &"no-row-recent".to_string()] {
            let d = data.join("sessions").join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("mcp.json"), "{}").unwrap();
        }
        // A kit overlay of a profile that exists, and one of a profile that is gone.
        let input = crate::claude::profile::ProfileInput { name: "P".into(), config_dir: Some(data.join("cfg").to_string_lossy().into_owned()), ..Default::default() };
        let prof = crate::claude::profile::create_managed(&conn, &input, &data.join("home")).unwrap();
        for p in [prof.id.as_str(), "gone-profile"] {
            std::fs::create_dir_all(data.join("kits").join("k1").join("overlay").join(p).join("team-kit")).unwrap();
        }

        // Long running: only "old" is past the 30 days; the live one never goes, however old.
        assert_eq!(artifacts(&conn, data, now).unwrap(), Pruned { session_dirs: 1, overlays: 1 });
        let left: Vec<bool> = [&live.id, &recent.id, &old.id].iter().map(|id| data.join("sessions").join(id).exists()).collect();
        assert_eq!(left, [true, true, false]);
        assert!(data.join("sessions").join("no-row-recent").exists(), "a folder without a row goes only when it is old itself");
        assert!(data.join("kits").join("k1").join("overlay").join(&prof.id).exists());
        assert!(!data.join("kits").join("k1").join("overlay").join("gone-profile").exists());
        // Far in the future: the ended sessions' folders go; the live one stays.
        let later = artifacts(&conn, data, now + 2 * KEEP_MS).unwrap();
        assert_eq!(later.session_dirs, 2, "recent (now old) and the row-less folder");
        assert!(data.join("sessions").join(&live.id).exists());
    }
}
