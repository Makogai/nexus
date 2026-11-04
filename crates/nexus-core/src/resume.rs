//! "Resume project" (M18, ADR-0021): reconstruct where a project stands from **verified** live state (Git, now) and
//! **saved** history (task files, branch-relevant notes and decisions, session checkpoints, session records), and
//! recommend one next action with the evidence for it. Deterministic and bounded — no model calls; everything that
//! originates from files or from Claude is clipped and kept apart from what was verified.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::Connection;
use serde::Serialize;

use crate::checkpoint;
use crate::claude::profile;
use crate::error::Result;
use crate::git;
use crate::knowledge::branches::{self, GitRef, Relevance};
use crate::knowledge::{service as knowledge, tasks};
use crate::session::{self, Filter, Kind as SessionKind, Session};
use crate::{now_ms, project, workspace};

const MAX_TASKS: usize = 8;
const MAX_NOTES: usize = 6;
const MAX_CHECKPOINTS: usize = 5;
const MAX_SESSIONS: usize = 3;
const MAX_TEXT: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommitRef {
    pub oid: String,
    pub summary: String,
    pub at: i64,
}

/// Facts read from Git just now.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Verified {
    pub checked_at: i64,
    pub available: bool,
    pub repo: bool,
    pub branch: Option<String>,
    pub detached: bool,
    pub head: Option<CommitRef>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub staged: u32,
    pub unstaged: u32,
    pub untracked: u32,
    pub conflicted: u32,
    pub recent_commits: Vec<CommitRef>,
}

impl Verified {
    pub fn uncommitted(&self) -> u32 {
        self.staged + self.unstaged + self.untracked
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskBrief {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: Option<String>,
    pub branch: Option<String>,
    pub checklist_done: usize,
    pub checklist_total: usize,
    pub last_progress: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NoteBrief {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: Option<String>,
    pub relevance: Relevance,
    pub branch: Option<String>,
    pub updated: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointBrief {
    pub kind: String,
    pub summary: String,
    pub session_title: String,
    pub at: i64,
    /// Branch the session was on when it was launched (machine-local record).
    pub branch: Option<String>,
    pub other_branch: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionBrief {
    pub id: String,
    pub title: String,
    pub state: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub resumable: bool,
    pub task_id: Option<String>,
    pub branch: Option<String>,
}

/// History saved by people, Claude and NEXUS — true when recorded, not re-verified.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub tasks: Vec<TaskBrief>,
    pub notes: Vec<NoteBrief>,
    pub decisions: Vec<NoteBrief>,
    pub checkpoints: Vec<CheckpointBrief>,
    pub sessions: Vec<SessionBrief>,
    pub hidden_other_branch: usize,
    /// Memory proposals from past sessions waiting for review (not knowledge yet, ADR-0022).
    pub pending_proposals: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    ResumeSession,
    LaunchTask,
    ResolveConflicts,
    ReviewChanges,
    Pull,
    Launch,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NextAction {
    pub kind: ActionKind,
    pub label: String,
    /// Facts the recommendation is based on, each marked by where it comes from.
    pub evidence: Vec<String>,
    pub session_id: Option<String>,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResumeSummary {
    pub project_id: String,
    pub project_name: String,
    pub verified: Verified,
    pub saved: Saved,
    pub next: NextAction,
    pub warnings: Vec<String>,
}

fn clip(s: &str, max: usize) -> String {
    tasks::escape_untrusted(s, max)
}

fn verified(repo: Option<&Path>) -> Verified {
    let mut v = Verified { checked_at: now_ms(), ..Default::default() };
    let Some(repo) = repo.filter(|r| r.is_dir()) else { return v };
    v.available = true;
    let Ok(st) = git::status(repo) else { return v };
    v.repo = st.is_repo;
    v.branch = st.branch.clone();
    v.detached = st.detached;
    v.upstream = st.upstream.clone();
    (v.ahead, v.behind) = (st.ahead, st.behind);
    (v.staged, v.unstaged, v.untracked, v.conflicted) = (st.staged, st.unstaged, st.untracked, st.conflicted);
    v.head = st.last_commit.as_ref().map(|c| CommitRef { oid: c.oid.chars().take(12).collect(), summary: clip(&c.summary, 120), at: c.timestamp_ms });
    if v.repo {
        v.recent_commits = git::recent_commits(repo, 5)
            .unwrap_or_default()
            .into_iter()
            .map(|c| CommitRef { oid: c.oid.chars().take(12).collect(), summary: clip(&c.summary, 120), at: c.timestamp_ms })
            .collect();
    }
    v
}

const STATUS_ORDER: &[&str] = &["in-progress", "blocked", "review", "ready", "backlog"];

/// Build the summary for a project. `home` is needed to tell whether a past conversation can be resumed.
pub fn summarize(conn: &Connection, data_dir: &Path, home: &Path, project_id: &str) -> Result<ResumeSummary> {
    let p = project::get(conn, project_id)?;
    let repo = p.local_path.clone().filter(|l| l.is_dir());
    let verified = verified(repo.as_deref());
    let mut saved = Saved::default();
    let mut warnings = Vec::new();

    // Knowledge (files): open tasks, branch-relevant notes and decisions.
    if let Some(ws) = workspace::get(conn, &p.workspace_id).ok().filter(|w| w.available) {
        knowledge::refresh(conn, data_dir, &ws)?;
        let items: Vec<_> = knowledge::list(conn, &ws, &[], false)?
            .into_iter()
            .filter(|i| i.conflict.is_none() && (i.project_id.is_none() || i.project_id.as_deref() == Some(p.id.as_str())))
            .collect();
        let named = items.iter().any(|i| i.branch.is_some() || i.commit.is_some());
        // Full commit ids for relevance (the summary shows short ones).
        let full_ref = repo.as_deref().filter(|_| named).map(GitRef::of).unwrap_or_default();
        let history: HashSet<String> = repo.as_deref().filter(|_| named).map(branches::history).unwrap_or_default();
        let mut open: Vec<_> = items.iter().filter(|i| i.kind.as_deref() == Some("task") && i.status.as_deref() != Some("done")).collect();
        open.sort_by_key(|t| (STATUS_ORDER.iter().position(|s| t.status.as_deref() == Some(*s)).unwrap_or(9), std::cmp::Reverse(t.updated.clone())));
        for t in open.into_iter().take(MAX_TASKS) {
            let Some(id) = &t.id else { continue };
            let Ok(v) = tasks::view(conn, &ws, id, 0) else { continue };
            saved.tasks.push(TaskBrief {
                id: v.id.clone(),
                title: clip(&v.title, 120),
                status: v.status.clone(),
                priority: v.priority.clone(),
                branch: v.branch.clone(),
                checklist_done: v.checked(),
                checklist_total: v.checklist.len(),
                last_progress: v.recent_log.last().map(|l| clip(l, MAX_TEXT)),
                updated: t.updated.clone(),
            });
        }
        for i in &items {
            let kind = i.kind.as_deref().unwrap_or_default();
            if !matches!(kind, "note" | "decision") {
                continue;
            }
            let rel = branches::relevance(i, &full_ref, &history);
            if !rel.visible() {
                saved.hidden_other_branch += 1;
                continue;
            }
            let brief = NoteBrief {
                id: i.id.clone().unwrap_or_default(),
                kind: kind.to_string(),
                title: clip(&i.title, 120),
                status: i.status.clone(),
                relevance: rel,
                branch: i.branch.clone(),
                updated: i.updated.clone(),
            };
            if kind == "decision" {
                if i.status.as_deref() != Some("superseded") {
                    saved.decisions.push(brief);
                }
            } else {
                saved.notes.push(brief);
            }
        }
        let rank = |n: &NoteBrief| (n.relevance != Relevance::ThisBranch, std::cmp::Reverse(n.updated.clone()));
        saved.notes.sort_by_key(rank);
        saved.decisions.sort_by_key(rank);
        saved.notes.truncate(MAX_NOTES);
        saved.decisions.truncate(MAX_NOTES);
    }

    saved.pending_proposals = crate::knowledge::proposals::pending_count(conn, &p.workspace_id, Some(&p.id)).unwrap_or(0);

    // Machine-local history: checkpoints and sessions.
    let sessions_all = session::list(conn, &Filter { project_id: Some(p.id.clone()), kind: Some(SessionKind::Claude), include_archived: false, limit: Some(50), ..Default::default() })?;
    let branch_of: std::collections::HashMap<&str, Option<&str>> = sessions_all.iter().map(|s| (s.id.as_str(), s.branch.as_deref())).collect();
    for c in checkpoint::for_project(conn, &p.id, MAX_CHECKPOINTS as u32)? {
        let branch = branch_of.get(c.session_id.as_str()).copied().flatten().map(str::to_string);
        let other_branch = matches!((&branch, &verified.branch), (Some(b), Some(cur)) if b != cur);
        saved.checkpoints.push(CheckpointBrief { kind: c.kind, summary: clip(&c.summary, MAX_TEXT), session_title: clip(&c.session_title, 80), at: c.created_at, branch, other_branch });
    }
    let profiles = profile::list(conn, true)?;
    let resumable = |s: &Session| -> bool {
        !s.state.is_live()
            && s.cwd.is_dir()
            && match (&s.profile_id, &s.claude_session_id) {
                (Some(pid), Some(csid)) => profiles.iter().find(|p| &p.id == pid).is_some_and(|p| session::conversation_recorded(&p.effective_config_dir(home), csid)),
                _ => false,
            }
    };
    for s in sessions_all.iter().take(MAX_SESSIONS) {
        saved.sessions.push(SessionBrief {
            id: s.id.clone(),
            title: clip(&s.title, 80),
            state: format!("{:?}", s.state).to_lowercase(),
            started_at: s.started_at,
            ended_at: s.ended_at,
            resumable: resumable(s),
            task_id: s.task_id.clone(),
            branch: s.branch.clone(),
        });
    }
    // The most recent resumable conversation per task (may be older than the three listed).
    let task_session = |task_id: &str| -> Option<&Session> { sessions_all.iter().find(|s| s.task_id.as_deref() == Some(task_id) && resumable(s)) };

    if !verified.available {
        warnings.push("The project folder is not available on this machine — only saved context is shown.".into());
    } else if !verified.repo {
        warnings.push("The folder is not a Git repository — there is no verified branch state.".into());
    }
    if saved.checkpoints.iter().any(|c| c.other_branch) {
        warnings.push("Some checkpoints were recorded on another branch than the one checked out now.".into());
    }
    if let Some(s) = sessions_all.first().filter(|s| s.state == session::State::Interrupted) {
        warnings.push(format!("The last session (“{}”) was interrupted, not finished.", clip(&s.title, 60)));
    }

    let next = recommend(&verified, &saved, |tid| task_session(tid).map(|s| (s.id.clone(), s.branch.clone(), s.ended_at.unwrap_or(s.started_at))));
    if let Some(tid) = &next.task_id
        && let Some(t) = saved.tasks.iter().find(|t| &t.id == tid)
        && let (Some(tb), Some(cur)) = (&t.branch, &verified.branch)
        && tb != cur
    {
        warnings.push(format!("“{}” was linked to branch {tb}, but {cur} is checked out — switch branches first if needed (NEXUS does not).", t.title));
    }
    Ok(ResumeSummary { project_id: p.id, project_name: p.name, verified, saved, next, warnings })
}

/// The next action, from facts only. `resumable_session(task_id)` returns (session id, branch, last activity) of the
/// task's most recent resumable conversation.
pub fn recommend(v: &Verified, s: &Saved, resumable_session: impl Fn(&str) -> Option<(String, Option<String>, i64)>) -> NextAction {
    let action = |kind, label: String, evidence: Vec<String>, session_id: Option<String>, task_id: Option<String>| NextAction { kind, label, evidence, session_id, task_id };
    let task_ev = |t: &TaskBrief| {
        let mut e = vec![format!("saved: task “{}” is {}", t.title, t.status)];
        if t.checklist_total > 0 {
            e.push(format!("saved: checklist {}/{} done", t.checklist_done, t.checklist_total));
        }
        if let Some(l) = &t.last_progress {
            e.push(format!("saved: last progress — {l}"));
        }
        e
    };
    if v.conflicted > 0 {
        return action(ActionKind::ResolveConflicts, format!("Resolve {} merge conflict(s)", v.conflicted), vec![format!("verified: git reports {} conflicted file(s)", v.conflicted)], None, None);
    }
    // Continue unfinished task work where a conversation can be resumed.
    for status in ["in-progress", "blocked", "review"] {
        for t in s.tasks.iter().filter(|t| t.status == status) {
            if let Some((sid, branch, _)) = resumable_session(&t.id) {
                let mut e = task_ev(t);
                e.push(format!("saved: its last session can be resumed{}", branch.map(|b| format!(" (started on {b})")).unwrap_or_default()));
                return action(ActionKind::ResumeSession, format!("Resume work on “{}”", t.title), e, Some(sid), Some(t.id.clone()));
            }
        }
    }
    if let Some(t) = s.tasks.iter().find(|t| t.status == "blocked") {
        return action(ActionKind::LaunchTask, format!("Unblock “{}”", t.title), task_ev(t), None, Some(t.id.clone()));
    }
    if let Some(t) = s.tasks.iter().find(|t| t.status == "in-progress") {
        return action(ActionKind::LaunchTask, format!("Continue “{}”", t.title), task_ev(t), None, Some(t.id.clone()));
    }
    if v.uncommitted() > 0 {
        return action(
            ActionKind::ReviewChanges,
            format!("Review {} uncommitted change(s)", v.uncommitted()),
            vec![format!("verified: {} staged, {} modified, {} untracked on {}", v.staged, v.unstaged, v.untracked, v.branch.as_deref().unwrap_or("the current checkout"))],
            None,
            None,
        );
    }
    if v.behind > 0 {
        return action(
            ActionKind::Pull,
            format!("Pull {} new commit(s) from {}", v.behind, v.upstream.as_deref().unwrap_or("upstream")),
            vec![format!("verified: {} commit(s) behind {} (as of the last fetch)", v.behind, v.upstream.as_deref().unwrap_or("upstream"))],
            None,
            None,
        );
    }
    if let Some(t) = s.tasks.iter().find(|t| t.status == "review") {
        return action(ActionKind::LaunchTask, format!("Review “{}”", t.title), task_ev(t), None, Some(t.id.clone()));
    }
    if let Some(t) = s.tasks.iter().filter(|t| matches!(t.status.as_str(), "ready" | "backlog")).min_by_key(|t| {
        let p = match t.priority.as_deref() {
            Some("urgent") => 0,
            Some("high") => 1,
            Some("low") => 3,
            _ => 2,
        };
        (t.status != "ready", p)
    }) {
        return action(ActionKind::LaunchTask, format!("Start “{}”", t.title), task_ev(t), None, Some(t.id.clone()));
    }
    let mut e = vec!["saved: no open tasks for this project".to_string()];
    if let Some(c) = s.checkpoints.first() {
        e.push(format!("saved: latest checkpoint — [{}] {}", c.kind, c.summary));
    }
    action(ActionKind::Launch, "Launch Claude to plan the next step".into(), e, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: &str, priority: Option<&str>) -> TaskBrief {
        TaskBrief { id: id.into(), title: id.into(), status: status.into(), priority: priority.map(Into::into), branch: None, checklist_done: 1, checklist_total: 2, last_progress: Some("did x".into()), updated: None }
    }

    #[test]
    fn recommendation_rules_follow_the_evidence() {
        let clean = Verified { repo: true, available: true, branch: Some("main".into()), ..Default::default() };
        let none = |_: &str| None;
        let saved = |tasks: Vec<TaskBrief>| Saved { tasks, ..Default::default() };

        let conflicts = Verified { conflicted: 2, ..clean.clone() };
        assert_eq!(recommend(&conflicts, &saved(vec![task("a", "in-progress", None)]), none).kind, ActionKind::ResolveConflicts);

        let s = saved(vec![task("a", "review", None), task("b", "in-progress", None)]);
        let r = recommend(&clean, &s, |t| (t == "b").then(|| ("sess".to_string(), Some("main".to_string()), 0)));
        assert_eq!((r.kind, r.session_id.as_deref(), r.task_id.as_deref()), (ActionKind::ResumeSession, Some("sess"), Some("b")));
        assert!(r.evidence.iter().any(|e| e.starts_with("saved: checklist 1/2")), "{:?}", r.evidence);

        assert_eq!(recommend(&clean, &saved(vec![task("a", "blocked", None), task("b", "in-progress", None)]), none).label, "Unblock “a”");
        assert_eq!(recommend(&clean, &saved(vec![task("b", "in-progress", None)]), none).kind, ActionKind::LaunchTask);

        let dirty = Verified { unstaged: 3, ..clean.clone() };
        let r = recommend(&dirty, &saved(vec![task("r", "review", None)]), none);
        assert_eq!(r.kind, ActionKind::ReviewChanges);
        assert!(r.evidence[0].starts_with("verified:"));
        let behind = Verified { behind: 4, upstream: Some("origin/main".into()), ..clean.clone() };
        assert_eq!(recommend(&behind, &saved(vec![]), none).kind, ActionKind::Pull);

        assert_eq!(recommend(&clean, &saved(vec![task("r", "review", None)]), none).label, "Review “r”");
        let backlog = saved(vec![task("low", "backlog", Some("low")), task("urgent", "backlog", Some("urgent")), task("ready", "ready", Some("low"))]);
        assert_eq!(recommend(&clean, &backlog, none).task_id.as_deref(), Some("ready"), "ready before backlog");
        assert_eq!(recommend(&clean, &saved(vec![]), none).kind, ActionKind::Launch);
    }
}

#[cfg(test)]
mod integration {
    use super::*;
    use crate::claude::profile::ProfileInput;
    use crate::knowledge::service::{Author, CreateInput};
    use crate::session::NewSession;

    fn git(repo: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid"])
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(ok.success());
    }

    #[test]
    fn summary_separates_verified_git_state_from_saved_history() {
        let t = tempfile::tempdir().unwrap();
        let root = dunce::canonicalize(t.path()).unwrap();
        let conn = crate::db::open_in_memory().unwrap();
        let data = root.join("data");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "first"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "second"]);
        std::fs::write(repo.join("wip.txt"), "x").unwrap();
        let ws = workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
        let p = project::register(&conn, &ws.id, repo.to_str().unwrap(), Some("repo")).unwrap();
        let mk = |kind: &str, title: &str, status: Option<&str>, branch: Option<&str>, commit: Option<String>| {
            knowledge::create(&conn, &data, &ws, &CreateInput {
                kind: kind.into(), title: title.into(), project_id: Some(p.id.clone()), status: status.map(Into::into),
                branch: branch.map(Into::into), commit, body: "- [ ] step\n".into(), ..Default::default()
            }, Author::Human).unwrap()
        };
        mk("task", "Backlog item", None, None, None);
        let wip = mk("task", "Checkout flow", Some("in-progress"), None, None);
        mk("note", "Main-only note", None, Some("main"), None);
        mk("note", "Feature note", None, Some("feature/x"), Some("f".repeat(40)));
        mk("decision", "Use Stripe", Some("accepted"), None, None);

        let s = summarize(&conn, &data, &root.join("home"), &p.id).unwrap();
        let v = &s.verified;
        assert!(v.repo && v.branch.as_deref() == Some("main") && v.untracked == 1);
        assert_eq!(v.recent_commits.iter().map(|c| c.summary.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        assert_eq!(s.saved.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["Checkout flow", "Backlog item"]);
        assert_eq!(s.saved.notes.iter().map(|n| (n.title.as_str(), n.relevance)).collect::<Vec<_>>(), [("Main-only note", Relevance::ThisBranch)]);
        assert_eq!(s.saved.hidden_other_branch, 1, "the other branch's note is hidden, Git decides");
        assert_eq!(s.saved.decisions[0].title, "Use Stripe");
        assert_eq!((s.next.kind, s.next.task_id.as_deref()), (ActionKind::LaunchTask, wip.id.as_deref()), "no conversation to resume yet");

        // A resumable conversation for the task makes "resume" the recommendation.
        let home = root.join("home");
        let cfg = root.join("cfg");
        let prof = profile::create_managed(&conn, &ProfileInput { name: "P".into(), config_dir: Some(cfg.to_string_lossy().into_owned()), ..Default::default() }, &home).unwrap();
        let sess = session::insert_starting(&conn, NewSession {
            title: "Checkout flow".into(), cwd: repo.clone(), program: "claude".into(), workspace_id: Some(ws.id.clone()),
            project_id: Some(p.id.clone()), profile_id: Some(prof.id.clone()), claude_session_id: Some("c0ffee00-0000-4000-8000-000000000000".into()),
            task_id: wip.id.clone(), branch: Some("feature/old".into()), ..Default::default()
        }).unwrap();
        session::mark_exited(&conn, &sess.id, Some(0)).unwrap();
        checkpoint::add(&conn, &sess.id, checkpoint::CheckpointKind::Progress, "Cart page done", checkpoint::Source::Claude).unwrap();
        std::fs::create_dir_all(cfg.join("projects").join("repo")).unwrap();
        std::fs::write(cfg.join("projects").join("repo").join("c0ffee00-0000-4000-8000-000000000000.jsonl"), "{}\n").unwrap();
        let s = summarize(&conn, &data, &home, &p.id).unwrap();
        assert_eq!((s.next.kind, s.next.session_id.as_deref()), (ActionKind::ResumeSession, Some(sess.id.as_str())));
        assert!(s.saved.checkpoints[0].other_branch, "recorded on feature/old while main is checked out");
        assert!(s.warnings.iter().any(|w| w.contains("another branch")));
        assert!(s.next.evidence.iter().all(|e| e.starts_with("saved: ") || e.starts_with("verified: ")), "every fact says where it comes from");
    }
}
