//! What a file-system watcher of the knowledge roots looks at (ADR-0049). The watching itself lives in the desktop host;
//! this module decides *where* to watch and *which* changes reach the index, with the same rules as the index walk.

use std::path::{Path, PathBuf};

use super::{Root, SCANNED_FOLDERS};

/// The folders to watch, recursively: every existing root that is not inside another root (the workspace folder also
/// covers its `.nexus-local/`). One watch per root, never one per subfolder — on Windows an open directory handle
/// keeps that folder's parent from being renamed or removed, and `git checkout` must stay free to replace `tasks/`.
pub fn targets(roots: &[Root]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        let covered = roots.iter().any(|o| o.dir != r.dir && r.dir.starts_with(&o.dir));
        if !covered && r.dir.is_dir() && !out.contains(&r.dir) {
            out.push(r.dir.clone());
        }
    }
    out
}

/// True when a change at `path` can change the index: a Markdown file (other than a folder's README) in one of the
/// scanned folders of a root, with no hidden component (editor temp files, `.git`). A folder counts only when it
/// `appeared_or_vanished` (created, removed, renamed): that takes its files along without an event per file, while a
/// folder's "modified" only echoes a change inside it. Whether the path still exists is not asked — it may be gone.
pub fn affects_index(roots: &[Root], path: &Path, appeared_or_vanished: bool) -> bool {
    roots.iter().any(|r| {
        let Ok(rel) = path.strip_prefix(&r.dir) else { return false };
        let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy());
        let Some(first) = parts.next() else { return false };
        if !SCANNED_FOLDERS.contains(&first.as_ref()) || parts.any(|p| p.starts_with('.')) {
            return false;
        }
        match rel.extension() {
            None => appeared_or_vanished,
            Some(x) => x.eq_ignore_ascii_case("md") && !rel.file_name().is_some_and(|n| n.eq_ignore_ascii_case("README.md")),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::index::files_under;
    use crate::knowledge::{LOCAL_DIR, roots};

    fn write(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }

    #[test]
    fn a_change_reaches_the_index_exactly_when_the_index_walk_would_read_the_file() {
        let t = tempfile::tempdir().unwrap();
        let (ws, data) = (t.path().join("ws"), t.path().join("data"));
        let roots = roots("w1", &ws, &data);
        let indexed = [
            ws.join("tasks/a.md"),
            ws.join("rules/security.MD"),
            ws.join("project-context/app/architecture.md"),
            ws.join("knowledge/notes/2026/n.md"),
            ws.join(LOCAL_DIR).join("tasks/private.md"),
            data.join("global/rules/g.md"),
        ];
        let ignored = [
            ws.join("tasks/README.md"),
            ws.join("tasks/.a.md.tmp"),
            ws.join("tasks/.drafts/b.md"),
            ws.join("tasks/a.txt"),
            ws.join("docs/a.md"),
            ws.join("nexus-workspace.json"),
            ws.join(".git/index"),
            ws.join("skills/review/SKILL.md"),
            ws.join(LOCAL_DIR).join("sessions/x.md"),
            t.path().join("elsewhere/tasks/a.md"),
        ];
        for p in indexed.iter().chain(&ignored) {
            write(p);
        }
        let walked: Vec<PathBuf> = roots.iter().flat_map(|r| files_under(&r.dir)).collect();
        for p in &indexed {
            assert!(walked.contains(p), "the index reads {}", p.display());
            assert!(affects_index(&roots, p, false) && affects_index(&roots, p, true), "a change to {} refreshes", p.display());
        }
        for p in &ignored {
            assert!(!walked.contains(p), "the index skips {}", p.display());
            assert!(!affects_index(&roots, p, false) && !affects_index(&roots, p, true), "a change to {} is ignored", p.display());
        }
        // A folder moved or removed in one step takes its documents along; its "modified" only echoes a file inside.
        assert!(affects_index(&roots, &ws.join("tasks"), true));
        assert!(affects_index(&roots, &ws.join("project-context/app"), true));
        assert!(!affects_index(&roots, &ws.join("tasks"), false));
        assert!(!affects_index(&roots, &ws, true), "the workspace folder itself");
        assert!(!affects_index(&roots, &ws.join(".git/refs"), true));
    }

    #[test]
    fn one_watch_per_existing_root_and_none_inside_another() {
        let t = tempfile::tempdir().unwrap();
        let (ws, data) = (t.path().join("ws"), t.path().join("data"));
        let roots = roots("w1", &ws, &data);
        assert!(targets(&roots).is_empty(), "nothing exists yet");
        std::fs::create_dir_all(ws.join(LOCAL_DIR)).unwrap();
        assert_eq!(targets(&roots), vec![ws.clone()], ".nexus-local is covered by the workspace folder");
        std::fs::create_dir_all(data.join("global")).unwrap();
        assert_eq!(targets(&roots), vec![ws, data.join("global")]);
    }
}
