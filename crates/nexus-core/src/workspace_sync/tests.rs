use super::*;
use super::ops::{self, Side, Sync};
use crate::knowledge::service::{self, Author, CreateInput};
use std::path::PathBuf;

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "core.autocrlf=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

#[test]
fn sensitive_names() {
    for n in [".env", "config/.env.production", "nexus.db", "x.db-wal", "id_rsa", "keys/server.pem", ".credentials.json", ".npmrc"] {
        assert!(sensitive_reason(n).is_some(), "{n}");
    }
    for n in ["README.md", "tasks/env-setup.md", "rules/keys.md", "nexus-workspace.json"] {
        assert!(sensitive_reason(n).is_none(), "{n}");
    }
}

#[test]
fn readiness_reports_what_would_travel_and_what_must_not() {
    let t = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
    let data = root.join("data");
    std::fs::create_dir_all(&data).unwrap();
    // Not a repository yet: guidance, not ready.
    let r = readiness(&conn, &ws, &data).unwrap();
    assert!(!r.repo && !r.ready && r.issues[0].message.contains("not a Git repository"));

    git(&ws.root_path, &["init", "-q"]);
    git(&ws.root_path, &["add", "-A"]);
    git(&ws.root_path, &["commit", "-qm", "init"]);
    std::fs::create_dir_all(ws.root_path.join("tasks")).unwrap();
    std::fs::write(ws.root_path.join("tasks").join("a.md"), "---\nnexus: 1\n---\n").unwrap();
    std::fs::write(ws.root_path.join("server.pem"), "not read").unwrap();
    std::fs::write(ws.root_path.join(".env"), "IGNORED=by-the-generated-gitignore").unwrap();
    let r = readiness(&conn, &ws, &data).unwrap();
    assert!(r.repo && r.branch.is_some());
    assert_eq!(r.changed, vec!["tasks/a.md".to_string()], "the new task would travel; .env is ignored by the generated .gitignore");
    assert_eq!(r.sensitive, vec![SensitiveFile { path: "server.pem".into(), tracked: false, reason: "private key or keystore" }]);
    assert!(r.ready, "warnings only: {:?}", r.issues);
    assert!(r.issues.iter().any(|i| i.severity == "warning" && i.message.contains("not ignored")));
    assert!(r.issues.iter().any(|i| i.message.contains("no remote")));

    // A committed key blocks; so does a data dir inside the workspace.
    git(&ws.root_path, &["add", "server.pem"]);
    git(&ws.root_path, &["commit", "-qm", "oops"]);
    let r = readiness(&conn, &ws, &ws.root_path.join(".nexus-data")).unwrap();
    assert!(!r.ready && r.data_dir_inside);
    assert!(r.sensitive.iter().any(|s| s.tracked && s.path == "server.pem"));
    assert!(r.issues.iter().any(|i| i.severity == "error" && i.message.contains("committed")));

    // Ahead of an upstream (as of the last fetch); no network is used by the check.
    let remote = root.join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "-q", "--bare"]);
    git(&ws.root_path, &["remote", "add", "origin", &format!("file://{}", remote.display())]);
    git(&ws.root_path, &["push", "-q", "-u", "origin", "HEAD"]);
    git(&ws.root_path, &["add", "-A"]);
    git(&ws.root_path, &["commit", "-qm", "task"]);
    let r = readiness(&conn, &ws, &data).unwrap();
    assert_eq!((r.ahead, r.behind, r.upstream.is_some(), r.remote.is_some()), (1, 0, true, true));
}

// ---- M29: two machines sharing a local bare remote ----------------------------------------------------------------

struct Machine {
    conn: Connection,
    data: PathBuf,
    home: PathBuf,
    host: String,
    ws: Workspace,
}

impl Machine {
    fn sync(&self) -> Sync<'_> {
        Sync { conn: &self.conn, ws: &self.ws, data_dir: &self.data, home: &self.home, host_id: &self.host }
    }
    fn root(&self) -> &Path {
        &self.ws.root_path
    }
    fn commit_all(&self, msg: &str) -> String {
        let r = ops::review_commit(&self.sync(), None).unwrap();
        assert!(!r.inspection.blocked, "{:?}", r.inspection.findings);
        ops::commit(&self.sync(), msg, None, &r.token).unwrap().oid
    }
    fn push(&self) -> ops::Pushed {
        let r = ops::review_push(&self.sync()).unwrap();
        ops::push(&self.sync(), &r.token).unwrap()
    }
}

fn host(conn: &Connection) -> String {
    crate::host::register(conn, crate::host::HostKind::Cli).unwrap().id
}

fn set_identity(dir: &Path) {
    git(dir, &["config", "user.name", "T"]);
    git(dir, &["config", "user.email", "t@example.invalid"]);
}

/// Machine A with a workspace repository pushed to a fresh bare remote; returns (A, remote URL, temp root).
fn origin() -> (Machine, String, tempfile::TempDir) {
    let t = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let (data, home) = (root.join("a-data"), root.join("home"));
    std::fs::create_dir_all(&data).unwrap();
    std::fs::create_dir_all(&home).unwrap();
    let ws = crate::workspace::create(&conn, root.join("a-ws").to_str().unwrap(), "Team", "").unwrap();
    git(&ws.root_path, &["init", "-q"]);
    set_identity(&ws.root_path);
    let remote = root.join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git(&remote, &["init", "-q", "--bare"]);
    let url = format!("file://{}", remote.display()).replace('\\', "/");
    git(&ws.root_path, &["remote", "add", "origin", &url]);
    let a = Machine { host: host(&conn), conn, data, home, ws };
    (a, url, t)
}

fn second_machine(t: &tempfile::TempDir, url: &str, name: &str) -> (Machine, ops::Restore) {
    let root = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let data = root.join(format!("{name}-data"));
    std::fs::create_dir_all(&data).unwrap();
    let h = host(&conn);
    let (ws, restore) = ops::clone_workspace(&conn, &data, &h, url, root.join(format!("{name}-ws")).to_str().unwrap()).unwrap();
    set_identity(&ws.root_path);
    (Machine { conn, data, home: root.join("home"), host: h, ws }, restore)
}

fn rule(title: &str, body: &str) -> CreateInput {
    CreateInput { kind: "rule".into(), title: title.into(), body: body.into(), applies: Some("always".into()), ..Default::default() }
}

#[test]
fn first_push_then_onboarding_by_clone_restores_portable_configuration() {
    let (a, url, t) = origin();
    service::create(&a.conn, &a.data, &a.ws, &rule("Small commits", "Keep commits small."), Author::Human).unwrap();
    let kit = a.root().join("kits").join("team").join(".claude-plugin");
    std::fs::create_dir_all(&kit).unwrap();
    std::fs::write(kit.join("plugin.json"), r#"{"name":"team","version":"1.0.0"}"#).unwrap();
    let wf = crate::workflow::create(&a.conn, &a.data, &a.ws, &crate::workflow::NewWorkflow { title: "Ship".into(), category: "feature".into(), ..Default::default() }).unwrap();
    crate::workflow::save_raw(&a.conn, &a.data, &a.ws, &wf.summary.id, "## Build\n<!-- step: id=build kind=claude gates=test -->\n", Some(&wf.summary.hash)).unwrap();

    // Before anything is pushed: the whole history travels, inspected.
    a.commit_all("Workspace");
    let review = ops::review_push(&a.sync()).unwrap();
    assert!(review.first_push && review.commits.len() == 1 && !review.inspection.blocked);
    assert_eq!(a.push().commits, 1);
    let st = status(&a.conn, &a.ws, &a.data).unwrap();
    assert_eq!(st.state, "up-to-date", "{st:?}");
    assert!(st.history.iter().any(|h| h.op == "push" && h.status == "ok") && st.history.iter().any(|h| h.op == "commit" && h.status == "ok"));

    // Machine B onboards from the repository: portable configuration comes along, machine-local state does not.
    let (b, restore) = second_machine(&t, &url, "b");
    assert_eq!(b.ws.id, a.ws.id, "the same workspace");
    assert!(restore.pending_approvals.iter().any(|d| d.title == "Small commits"), "a synced rule is inactive until approved here: {restore:?}");
    assert!(restore.pending_approvals.iter().any(|d| d.kind == "workflow"));
    assert_eq!(restore.kit_folders, vec![("kits/team".to_string(), false)]);
    assert_eq!(restore.gates_needed, vec!["test".to_string()]);
    assert!(restore.steps.iter().any(|s| s.contains("machine-local and never synced")));
    assert!(!b.root().join(".nexus-local").exists() && !b.data.join("nexus.db").exists(), "nothing machine-local arrived");
    let h = history::list(&b.conn, &b.ws.id, 5).unwrap();
    assert_eq!((h[0].op.as_str(), h[0].status.as_str()), ("clone", "ok"));
    // On machine A the workspace is already open: a second clone is refused and leaves nothing behind.
    let again = dunce::canonicalize(t.path()).unwrap().join("a-again");
    let e = ops::clone_workspace(&a.conn, &a.data, &a.host, &url, again.to_str().unwrap()).unwrap_err().to_string();
    assert!(e.contains("already open on this machine"), "{e}");
    assert!(!again.exists());
    assert_eq!(crate::workspace::get(&a.conn, &a.ws.id).unwrap().root_path, a.ws.root_path, "the registration did not move");
}

#[test]
fn fetch_shows_incoming_and_pull_fast_forwards_with_approvals_required_again() {
    let (a, url, t) = origin();
    let r = service::create(&a.conn, &a.data, &a.ws, &rule("Tests", "Every change ships with a test."), Author::Human).unwrap();
    a.commit_all("Rule");
    a.push();
    let (b, _) = second_machine(&t, &url, "b");
    service::approve(&b.conn, &b.data, &b.ws, r.id.as_deref().unwrap()).unwrap();
    // A changes the rule and an AI Kit folder; B has approved the old versions.
    let kit = a.root().join("kits").join("team");
    std::fs::create_dir_all(kit.join(".claude-plugin")).unwrap();
    std::fs::write(kit.join(".claude-plugin").join("plugin.json"), r#"{"name":"team","version":"1.0.0"}"#).unwrap();
    a.commit_all("Kit");
    a.push();
    ops::fetch(&b.sync()).unwrap();
    ops::pull(&b.sync()).unwrap();
    let kit_b = crate::kits::register_local(&b.conn, &b.data, b.root().join("kits").join("team").to_str().unwrap(), None, None).unwrap();
    crate::kits::approve(&b.conn, &b.data, &kit_b.id).unwrap();
    let path = r.path.strip_prefix(a.root()).unwrap().to_path_buf();
    let mut text = std::fs::read_to_string(a.root().join(&path)).unwrap();
    text.push_str("\nAlso run the linter.\n");
    std::fs::write(a.root().join(&path), text).unwrap();
    std::fs::write(kit.join(".claude-plugin").join("plugin.json"), r#"{"name":"team","version":"1.1.0"}"#).unwrap();
    a.commit_all("Rule and kit changed");
    a.push();

    let f = ops::fetch(&b.sync()).unwrap();
    assert_eq!(f.incoming, 1);
    let st = status(&b.conn, &b.ws, &b.data).unwrap();
    assert_eq!((st.state, st.incoming.len(), st.incoming[0].summary.as_str()), ("behind", 1, "Rule and kit changed"));
    let rel = path.to_string_lossy().replace('\\', "/");
    assert!(st.incoming_files.iter().any(|e| e.path == rel));
    let d = ops::file_diff(&b.ws, Side::Incoming, &rel).unwrap();
    assert!(d.text.contains("+Also run the linter."));
    let p = ops::pull(&b.sync()).unwrap();
    assert_eq!(p.commits, 1);
    assert!(p.needs_approval.iter().any(|n| n.title == "Tests"), "the changed rule needs approval again: {p:?}");
    assert_eq!(p.kits_changed, vec!["team".to_string()], "the changed kit needs a new review");
    assert_eq!(crate::kits::view(&b.conn, &b.data, &crate::kits::get(&b.conn, &kit_b.id).unwrap()).unwrap().status, "changed");
    let notes = crate::notify::list(&b.conn, &crate::notify::Filter { include_dismissed: true, ..Default::default() }).unwrap();
    assert!(notes.iter().any(|n| n.category == "review-required" && n.title.contains("Synced changes")));
    assert_eq!(status(&b.conn, &b.ws, &b.data).unwrap().state, "up-to-date");
}

#[test]
fn diverged_dirty_unsafe_and_busy_pulls_are_refused_without_changes() {
    let (a, url, t) = origin();
    std::fs::create_dir_all(a.root().join("knowledge").join("notes")).unwrap();
    std::fs::write(a.root().join("knowledge").join("notes").join("x.md"), "one\n").unwrap();
    a.commit_all("Notes");
    a.push();
    let (b, _) = second_machine(&t, &url, "b");

    // Local changes to a file that also changes upstream: refused, nothing touched.
    std::fs::write(a.root().join("knowledge").join("notes").join("x.md"), "two\n").unwrap();
    a.commit_all("Upstream edit");
    a.push();
    std::fs::write(b.root().join("knowledge").join("notes").join("x.md"), "local edit\n").unwrap();
    ops::fetch(&b.sync()).unwrap();
    let head = git::head(b.root());
    let e = ops::pull(&b.sync()).unwrap_err().to_string();
    assert!(e.contains("local changes to 1 incoming file"), "{e}");
    assert_eq!(git::head(b.root()), head);
    assert_eq!(std::fs::read_to_string(b.root().join("knowledge").join("notes").join("x.md")).unwrap(), "local edit\n");

    // Diverged: B commits locally, A pushed more — NEXUS only fast-forwards.
    let r = ops::review_commit(&b.sync(), None).unwrap();
    ops::commit(&b.sync(), "Local", None, &r.token).unwrap();
    let e = ops::pull(&b.sync()).unwrap_err().to_string();
    assert!(e.contains("diverged"), "{e}");
    let st = status(&b.conn, &b.ws, &b.data).unwrap();
    assert_eq!(st.state, "diverged");
    // …and pushing is refused instead of forced.
    let review = ops::review_push(&b.sync()).unwrap();
    let e = ops::push(&b.sync(), &review.token).unwrap_err().to_string();
    assert!(e.contains("never force-pushes"), "{e}");

    // Unsafe incoming: a teammate's commit with a symbolic link and a Windows device name (made with plumbing).
    let (c, _) = second_machine(&t, &url, "c");
    let blob = String::from_utf8(std::process::Command::new("git").args(["hash-object", "-w", "--stdin"]).current_dir(a.root()).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().and_then(|mut ch| {
        use std::io::Write;
        ch.stdin.take().unwrap().write_all(b"../../outside")?;
        ch.wait_with_output()
    }).unwrap().stdout).unwrap();
    git(a.root(), &["update-index", "--add", "--cacheinfo", &format!("120000,{},tools/escape", blob.trim())]);
    git(a.root(), &["commit", "-qm", "link"]);
    git(a.root(), &["push", "-q", "origin", "HEAD"]);
    ops::fetch(&c.sync()).unwrap();
    let head = git::head(c.root());
    let e = ops::pull(&c.sync()).unwrap_err().to_string();
    assert!(e.contains("cannot be checked out safely") && e.contains("symbolic link"), "{e}");
    assert_eq!(git::head(c.root()), head);

    // One operation at a time; an interrupted one is reconciled.
    let running = history::begin(&c.conn, &c.ws.id, "fetch", &c.host, None, None).unwrap();
    assert!(ops::fetch(&c.sync()).unwrap_err().to_string().contains("another sync operation"));
    history::finish(&c.conn, &running, "ok", None, 0, 0, None, None).unwrap();
    history::begin(&c.conn, &c.ws.id, "pull", "dead-host", None, None).unwrap();
    assert_eq!(history::reconcile(&c.conn, crate::now_ms()).unwrap(), 1);
    assert!(history::list(&c.conn, &c.ws.id, 5).unwrap().iter().any(|h| h.status == "interrupted"));
    // A Git lock left behind is reported and stops a pull.
    std::fs::write(git::git_dir(c.root()).unwrap().join("index.lock"), "").unwrap();
    assert!(status(&c.conn, &c.ws, &c.data).unwrap().in_progress.unwrap().contains("index.lock"));
    assert!(ops::pull(&c.sync()).unwrap_err().to_string().contains("index.lock"));
}

#[test]
fn commit_inspection_blocks_secrets_sensitive_files_and_private_knowledge() {
    let (a, _, _t) = origin();
    a.commit_all("init");
    let token = format!("{}{}", "ghp_", "Z".repeat(36));
    std::fs::write(a.root().join("notes.md"), format!("deploy with {token}\n")).unwrap();
    std::fs::write(a.root().join("deploy.pem"), "not read").unwrap();
    std::fs::create_dir_all(a.root().join(".nexus-local").join("knowledge")).unwrap();
    std::fs::write(a.root().join(".nexus-local").join("knowledge").join("mine.md"), "private").unwrap();
    std::fs::write(a.root().join("paths.md"), format!("log: {}\n", a.home.join("x.log").display())).unwrap();
    let r = ops::review_commit(&a.sync(), None).unwrap();
    assert!(r.inspection.blocked);
    let kinds: Vec<(&str, &str)> = r.inspection.findings.iter().map(|f| (f.path.as_str(), f.kind)).collect();
    for want in [("notes.md", "secret"), ("deploy.pem", "sensitive-file"), (".nexus-local/knowledge/mine.md", "local-only-knowledge"), ("paths.md", "machine-path")] {
        assert!(kinds.contains(&want), "{want:?} in {kinds:?}");
    }
    assert!(r.inspection.findings.iter().all(|f| !f.message.contains("ZZZZ")), "values are never reported");
    let e = ops::commit(&a.sync(), "Everything", None, &r.token).unwrap_err().to_string();
    assert!(e.contains("security inspection blocks"), "{e}");
    // The person can commit the harmless file alone; a stale review is refused.
    let only = vec!["paths.md".to_string()];
    let r2 = ops::review_commit(&a.sync(), Some(&only)).unwrap();
    assert!(!r2.inspection.blocked && r2.inspection.findings.iter().any(|f| f.kind == "machine-path"), "a warning, not a block");
    std::fs::write(a.root().join("paths.md"), "log: relative/x.log\n").unwrap();
    assert!(ops::commit(&a.sync(), "Paths", Some(&only), &r2.token).unwrap_err().to_string().contains("review them again"));
    let r3 = ops::review_commit(&a.sync(), Some(&only)).unwrap();
    ops::commit(&a.sync(), "Paths", Some(&only), &r3.token).unwrap();
    assert!(git::changed_paths(a.root()).unwrap().iter().any(|(_, p)| p == "notes.md"), "the blocked files stay uncommitted");
    // A secret committed outside NEXUS is caught before a push.
    git(a.root(), &["add", "notes.md"]);
    git(a.root(), &["commit", "-qm", "outside NEXUS"]);
    let p = ops::review_push(&a.sync()).unwrap();
    assert!(p.inspection.blocked && p.inspection.findings.iter().any(|f| f.path == "notes.md" && f.kind == "secret"));
    assert!(ops::push(&a.sync(), &p.token).unwrap_err().to_string().contains("blocks this push"));
}

#[test]
fn failed_commits_roll_back_staging_and_remote_failures_are_explained() {
    let (a, _, t) = origin();
    a.commit_all("init");
    let hooks = git::git_dir(a.root()).unwrap().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho refused by the team hook >&2\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(hooks.join("pre-commit"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(a.root().join("new.md"), "hello\n").unwrap();
    let head = git::head(a.root());
    let r = ops::review_commit(&a.sync(), None).unwrap();
    let e = ops::commit(&a.sync(), "New", None, &r.token).unwrap_err().to_string();
    assert!(e.contains("nothing was committed") && e.contains("undid its staging"), "{e}");
    assert_eq!(git::head(a.root()), head);
    assert!(git::changed_paths(a.root()).unwrap().contains(&("??".into(), "new.md".into())), "back to untracked");
    std::fs::remove_file(hooks.join("pre-commit")).unwrap();

    // Remote failures: a missing remote and an unreachable host — explained, recorded, nothing changes locally.
    let root = dunce::canonicalize(t.path()).unwrap();
    git(a.root(), &["remote", "set-url", "origin", &format!("file://{}", root.join("missing.git").display()).replace('\\', "/")]);
    let e = ops::fetch(&a.sync()).unwrap_err().to_string();
    assert!(e.contains("not found"), "{e}");
    git(a.root(), &["remote", "set-url", "origin", "https://127.0.0.1:9/team.git"]);
    let e = ops::fetch(&a.sync()).unwrap_err().to_string();
    assert!(e.contains("cannot be reached") || e.contains("did not answer"), "{e}");
    let h = history::list(&a.conn, &a.ws.id, 10).unwrap();
    let kinds: Vec<&str> = h.iter().filter(|e| e.op == "fetch").filter_map(|e| e.error_kind.as_deref()).collect();
    assert!(kinds.contains(&"remote-missing") && (kinds.contains(&"offline") || kinds.contains(&"timeout")), "{kinds:?}");
    assert!(status(&a.conn, &a.ws, &a.data).unwrap().readiness.repo, "offline: the local view still works");

    // Onboarding refuses repositories that are not workspaces, and leaves no folder behind.
    let plain = root.join("plain.git");
    std::fs::create_dir_all(&plain).unwrap();
    git(&plain, &["init", "-q", "--bare"]);
    let conn = crate::db::open_in_memory().unwrap();
    let h = host(&conn);
    let dest = root.join("not-a-ws");
    let e = ops::clone_workspace(&conn, &root.join("x-data"), &h, &format!("file://{}", plain.display()).replace('\\', "/"), dest.to_str().unwrap()).unwrap_err().to_string();
    assert!(e.contains("not a NEXUS workspace") || e.contains("Git reported"), "{e}");
    assert!(!dest.exists(), "the folder NEXUS created is removed again");
    std::fs::create_dir_all(root.join("busy")).unwrap();
    std::fs::write(root.join("busy").join("x"), "").unwrap();
    assert!(ops::clone_workspace(&conn, &root.join("x-data"), &h, "file:///x", root.join("busy").to_str().unwrap()).unwrap_err().to_string().contains("new or empty folder"));
    assert!(ops::clone_workspace(&conn, &root.join("x-data"), &h, "ext::sh -c touch% /tmp/pwned", root.join("y").to_str().unwrap()).is_err(), "command transports are refused");
}
