use std::process::Command;

use serde_json::json;

use super::*;
use crate::activity::{self, HookEvent};
use crate::checkpoint::Source;
use crate::session::NewSession;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "init.defaultBranch=main", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
}

struct Fx {
    _t: tempfile::TempDir,
    conn: Connection,
    data: PathBuf,
    ws: workspace::Workspace,
    repo: PathBuf,
    project: project::Project,
}

fn fixture() -> Fx {
    let t = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let data = base.join("data");
    let ws = workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
    let repo = base.join("shop");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("package.json"), r#"{"dependencies":{"vue":"^3"}}"#).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "init"]);
    let project = project::register_with_context(&conn, &data, &ws.id, repo.to_str().unwrap(), None).unwrap();
    Fx { _t: t, conn, data, ws, repo, project }
}

fn start(fx: &Fx, title: &str) -> Session {
    session::insert_starting(
        &fx.conn,
        NewSession {
            claude_session_id: Some(uuid::Uuid::new_v4().to_string()),
            title: title.into(),
            cwd: fx.repo.clone(),
            program: "claude".into(),
            workspace_id: Some(fx.ws.id.clone()),
            project_id: Some(fx.project.id.clone()),
            ..Default::default()
        },
    )
    .unwrap()
}

fn hook(fx: &Fx, s: &Session, event: HookEvent, payload: serde_json::Value) {
    let mut p = payload;
    p["session_id"] = json!(s.claude_session_id);
    activity::handle(&fx.conn, &s.id, event, &p).unwrap();
}

#[test]
fn a_meaningful_session_leaves_a_layered_record_and_a_chat_leaves_none() {
    let fx = fixture();
    let s = start(&fx, "Checkout");
    assert!(fx.conn.query_row("SELECT head_start FROM sessions WHERE id = ?1", [&s.id], |r| r.get::<_, Option<String>>(0)).unwrap().is_some(), "the starting commit is recorded");
    // What Claude Code's hooks report, a checkpoint, a real commit and a gate NEXUS ran.
    hook(&fx, &s, HookEvent::PostToolUse, json!({ "tool_name": "Edit", "tool_input": { "file_path": fx.repo.join("src").join("cart.ts") } }));
    hook(&fx, &s, HookEvent::PostToolUse, json!({ "tool_name": "Bash", "tool_input": { "command": "npm test" } }));
    checkpoint::add(&fx.conn, &s.id, CheckpointKind::Decision, "Use the hosted payment page", Source::Claude).unwrap();
    checkpoint::add(&fx.conn, &s.id, CheckpointKind::Progress, "Cart totals done; next: shipping costs", Source::Claude).unwrap();
    std::fs::write(fx.repo.join("src").join("cart.ts"), "export const total = 1\n").unwrap();
    git(&fx.repo, &["add", "-A"]);
    git(&fx.repo, &["commit", "-q", "-m", "Add cart totals"]);
    fx.conn
        .execute(
            "INSERT INTO gate_results(id, project_id, gate_name, command, status, exit_code, started_at, ended_at) VALUES ('g1', ?1, 'test', 'npm test', 'passed', 0, ?2, ?2)",
            params![fx.project.id, crate::now_ms()],
        )
        .unwrap();
    // A compaction leaves a durable NEXUS checkpoint.
    hook(&fx, &s, HookEvent::PreCompact, json!({ "trigger": "auto" }));
    assert!(checkpoint::for_session(&fx.conn, &s.id, 10).unwrap().iter().any(|c| c.source == "nexus" && c.summary.contains("Context compacted after 1 file edit")));
    session::mark_exited(&fx.conn, &s.id, Some(0)).unwrap();

    let path = finalize(&fx.conn, &fx.data, &s.id).unwrap().expect("a record");
    let ctx = project::context::context_dir(&fx.ws, &fx.project.id).unwrap();
    assert!(path.starts_with(ctx.join("development")), "{}", path.display());
    let (doc, _) = knowledge::store::read(&path).unwrap();
    assert_eq!(doc.validate(Privacy::Shared), Ok(knowledge::Kind::Note));
    assert_eq!((doc.fm.str("topic"), doc.fm.str("verification")), (Some(TOPIC), Some("unverified")));
    assert_eq!(doc.fm.str("branch"), Some("main"));
    let b = &doc.body;
    let verified = &b[b.find("## Verified").unwrap()..b.find("## Observed").unwrap()];
    assert!(verified.contains("Add cart totals") && verified.contains("test: passed"), "{verified}");
    let observed = &b[b.find("## Observed").unwrap()..b.find("## Reported").unwrap()];
    assert!(observed.contains("`src/cart.ts`") && observed.contains("test ×1") && observed.contains("results are not observed"), "{observed}");
    let reported = &b[b.find("## Reported by Claude (unverified)").unwrap()..];
    assert!(reported.contains("decision: Use the hosted payment page") && reported.contains("Last reported state (progress): Cart totals done"), "{reported}");
    assert!(finalize(&fx.conn, &fx.data, &s.id).unwrap().is_none(), "once per session");
    let state = std::fs::read_to_string(ctx.join("current-state.md")).unwrap();
    assert!(state.contains("Add cart totals"), "the current state follows the record: {state}");

    // Chatting only leaves no record.
    let chat = start(&fx, "Questions");
    hook(&fx, &chat, HookEvent::UserPrompt, json!({}));
    session::mark_exited(&fx.conn, &chat.id, Some(0)).unwrap();
    assert!(finalize(&fx.conn, &fx.data, &chat.id).unwrap().is_none());
    assert_eq!(std::fs::read_dir(ctx.join("development")).unwrap().count(), 1);
}

#[test]
fn an_abrupt_end_is_recorded_from_what_was_saved_and_history_shows_it() {
    let fx = fixture();
    let s = start(&fx, "Refactor");
    hook(&fx, &s, HookEvent::PostToolUse, json!({ "tool_name": "Write", "tool_input": { "file_path": fx.repo.join("src").join("a.ts") } }));
    checkpoint::add(&fx.conn, &s.id, CheckpointKind::Blocker, "Waiting for the API contract", Source::Claude).unwrap();
    // The host dies: reconciliation marks the session interrupted; the next host writes the record.
    fx.conn.execute("UPDATE sessions SET state = 'interrupted', ended_at = ?2 WHERE id = ?1", params![s.id, crate::now_ms()]).unwrap();
    assert_eq!(finalize_pending(&fx.conn, &fx.data).unwrap(), 1);
    assert_eq!(finalize_pending(&fx.conn, &fx.data).unwrap(), 0, "nothing pending twice");
    let h = history(&fx.conn, &fx.data, &fx.ws, &fx.project.id).unwrap();
    assert_eq!(h.records.len(), 1);
    assert_eq!(h.records[0].sync, "no-repository", "the workspace is not a Git repository yet");
    let (doc, _) = knowledge::store::read(&h.records[0].path).unwrap();
    assert!(doc.body.contains("ended abruptly") && doc.body.contains("Open blockers reported: 1"), "{}", doc.body);
    assert!(h.sessions.iter().any(|x| x.id == s.id && x.record.is_some()));
    assert_eq!(h.blockers.len(), 1);
    assert!(!h.commits.is_empty(), "the project's commits are listed");

    // In a workspace repository the record is "local" until it is committed.
    git(&fx.ws.root_path, &["init", "-q"]);
    let h = history(&fx.conn, &fx.data, &fx.ws, &fx.project.id).unwrap();
    assert_eq!(h.records[0].sync, "local");
    git(&fx.ws.root_path, &["add", "-A"]);
    git(&fx.ws.root_path, &["commit", "-q", "-m", "workspace"]);
    assert_eq!(history(&fx.conn, &fx.data, &fx.ws, &fx.project.id).unwrap().records[0].sync, "committed", "committed, no remote to push to yet");
}

#[test]
fn workspaces_that_turn_records_off_get_none() {
    let fx = fixture();
    let mut m = workspace::read_manifest(&fx.ws.root_path).unwrap();
    m.features.retain(|f| f != FEATURE);
    workspace::write_manifest(&fx.ws.root_path, &m).unwrap();
    let s = start(&fx, "Work");
    hook(&fx, &s, HookEvent::PostToolUse, json!({ "tool_name": "Edit", "tool_input": { "file_path": fx.repo.join("x.ts") } }));
    session::mark_exited(&fx.conn, &s.id, Some(0)).unwrap();
    assert!(finalize(&fx.conn, &fx.data, &s.id).unwrap().is_none());
}
