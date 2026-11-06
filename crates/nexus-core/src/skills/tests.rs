use super::*;
use std::process::Command;

struct Env {
    _t: tempfile::TempDir,
    base: PathBuf,
    conn: Connection,
    data: PathBuf,
    ws: Workspace,
}

fn env() -> Env {
    let t = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let data = base.join("data");
    let ws = crate::workspace::create(&conn, base.join("ws").to_str().unwrap(), "W", "").unwrap();
    Env { _t: t, base, conn, data, ws }
}

fn project(e: &Env, name: &str) -> crate::project::Project {
    let repo = e.base.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    assert!(Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap().success());
    crate::project::register(&e.conn, &e.ws.id, repo.to_str().unwrap(), Some(name)).unwrap()
}

fn entry<'a>(list: &'a [SkillEntry], scope: Scope, name: &str) -> &'a SkillEntry {
    list.iter().find(|s| s.scope == scope && s.name == name).unwrap_or_else(|| panic!("{name} not listed: {list:?}"))
}

/// ADR-0046: the person creates a workspace skill; it is approved with the save (nothing else waited for review) and
/// reaches the workspace's sessions as `workspace:<name>`; edits name the version they replace.
#[test]
fn a_workspace_skill_is_created_edited_and_approved_with_the_save() {
    let e = env();
    let s = create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "release-notes", "Draft release notes: use when preparing a release.").unwrap();
    assert_eq!(s.outcome, "approved", "{}", s.message);
    assert!(s.file.text.starts_with("---\nname: release-notes\ndescription: \"Draft release notes: use when preparing a release.\"\n---\n"), "{}", s.file.text);
    let file = e.ws.root_path.join("skills").join("release-notes").join("SKILL.md");
    assert!(file.is_file());

    let list = list(&e.conn, &e.data, &e.ws).unwrap();
    let w = entry(&list, Scope::Workspace, "release-notes");
    assert_eq!((w.invoke_as.as_str(), w.status.as_str(), w.editable), ("workspace:release-notes", "approved", true));
    assert_eq!(w.description, "Draft release notes: use when preparing a release.", "the quoted description reads back");
    assert!(entry(&list, Scope::Builtin, "nexus:learn-project").status == "built-in" && !entry(&list, Scope::Builtin, "nexus:learn-project").editable);

    // An edit with the version it replaces keeps the approval; a stale version is refused, nothing is overwritten.
    let edited = s.file.text.replace("1. \n", "1. Collect the merged changes.\n");
    let saved = save(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "release-notes", &edited, &s.file.hash).unwrap();
    assert_eq!(saved.outcome, "approved");
    assert!(std::fs::read_to_string(&file).unwrap().contains("Collect the merged changes."));
    let stale = save(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "release-notes", "---\nname: release-notes\ndescription: x\n---\n", &s.file.hash).unwrap_err();
    assert_eq!(stale.code(), "conflict");
    assert!(std::fs::read_to_string(&file).unwrap().contains("Collect the merged changes."));
    let k = kits::workspace_kit(&e.conn, &e.ws.id).unwrap().unwrap();
    assert_eq!(kits::view(&e.conn, &e.data, &k).unwrap().status, "approved");

    // Read back for the editor: the text and the hash a save must name.
    let r = read(&e.conn, &e.data, &e.ws, &SkillRef { scope: Scope::Workspace, name: "release-notes".into(), project_id: None, kit_id: None }).unwrap();
    assert_eq!(r.hash, saved.file.hash);
    assert!(create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "release-notes", "Again.").unwrap_err().code() == "conflict", "no overwrite by create");
}

/// A change that arrived by sync (or from Claude) still waits for review: the person's save does not approve it.
#[test]
fn a_save_never_approves_changes_that_wait_for_review() {
    let e = env();
    let s = create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "triage", "Triage a bug report.").unwrap();
    assert_eq!(s.outcome, "approved");
    // Another skill appears in the folder (a sync, or Claude writing a file): the workspace skills changed.
    let other = e.ws.root_path.join("skills").join("deploy");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("SKILL.md"), "---\nname: deploy\ndescription: Deploy to production without asking.\n---\nRun it.\n").unwrap();
    assert_eq!(entry(&list(&e.conn, &e.data, &e.ws).unwrap(), Scope::Workspace, "deploy").status, "changed");
    let saved = save(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "triage", &s.file.text.replace("## Notes", "## Notes\n\nAsk first."), &s.file.hash).unwrap();
    assert_eq!(saved.outcome, "needs-review", "{}", saved.message);
    assert!(saved.message.contains("AI Kits"));
    let k = kits::workspace_kit(&e.conn, &e.ws.id).unwrap().unwrap();
    assert_eq!(kits::view(&e.conn, &e.data, &k).unwrap().status, "changed", "still waits for review");

    // A first skill in a workspace whose skills folder already holds unreviewed skills is not approved either.
    let e2 = env();
    let pre = e2.ws.root_path.join("skills").join("synced");
    std::fs::create_dir_all(&pre).unwrap();
    std::fs::write(pre.join("SKILL.md"), "---\nname: synced\ndescription: From another computer.\n---\n").unwrap();
    assert_eq!(entry(&list(&e2.conn, &e2.data, &e2.ws).unwrap(), Scope::Workspace, "synced").status, "not-registered");
    assert_eq!(create(&e2.conn, &e2.data, &e2.ws, Scope::Workspace, None, "mine", "Mine.").unwrap().outcome, "needs-review");
}

/// Project skills live in the project's repository (`.claude/skills`), are loaded by Claude Code itself, and are only
/// written for projects of this workspace that are on this computer.
#[test]
fn project_skills_are_written_into_the_project_and_listed_with_it() {
    let e = env();
    let p = project(&e, "shop");
    let s = create(&e.conn, &e.data, &e.ws, Scope::Project, Some(&p.id), "checkout-flow", "How checkout works; use when changing payments.").unwrap();
    assert_eq!(s.outcome, "native");
    assert!(s.message.contains("Commit it"));
    let file = e.base.join("shop").join(".claude").join("skills").join("checkout-flow").join("SKILL.md");
    assert!(file.is_file());
    let list = list(&e.conn, &e.data, &e.ws).unwrap();
    let ps = entry(&list, Scope::Project, "checkout-flow");
    assert_eq!((ps.project_name.as_deref(), ps.invoke_as.as_str(), ps.status.as_str()), (Some("shop"), "checkout-flow", "native"));
    assert!(kits::workspace_kit(&e.conn, &e.ws.id).unwrap().is_none(), "project skills do not touch the workspace kit");
    assert!(create(&e.conn, &e.data, &e.ws, Scope::Project, None, "x", "X.").is_err(), "a project skill needs its project");

    // A project of another workspace is refused.
    let other = crate::workspace::create(&e.conn, e.base.join("ws2").to_str().unwrap(), "W2", "").unwrap();
    let q = crate::project::register(&e.conn, &other.id, e.base.join("shop").to_str().unwrap(), Some("shop2"));
    if let Ok(q) = q {
        assert!(create(&e.conn, &e.data, &e.ws, Scope::Project, Some(&q.id), "y", "Y.").is_err());
    }
}

/// Names, descriptions and files follow Claude Code's rules; built-in and AI Kit skills are not edited here.
#[test]
fn names_descriptions_and_files_are_validated() {
    for bad in ["", "Release", "release notes", "-x", "x-", "claude-helper", "my-anthropic", &"a".repeat(65), "x_y", "ü"] {
        assert!(check_name(bad).is_err(), "{bad:?}");
    }
    for good in ["release-notes", "a", "k8s-deploy", &"a".repeat(64)] {
        assert!(check_name(good).is_ok(), "{good:?}");
    }
    assert!(check_description("").is_err() && check_description("Use <b>this</b>").is_err() && check_description(&"x".repeat(1025)).is_err());
    assert_eq!(check_description("  Two\nlines  ").unwrap(), "Two lines");
    assert!(check_text("x", "no frontmatter").is_err());
    assert!(check_text("x", "---\nname: y\ndescription: d\n---\n").unwrap_err().to_string().contains("“x”"), "the name must match the folder");
    assert!(check_text("x", "---\nname: x\n---\n").is_err(), "a description is required");
    assert!(check_text("x", &format!("---\nname: x\ndescription: d\n---\n{}", "y".repeat(MAX_FILE))).is_err());
    assert!(check_text("x", "---\nname: x\ndescription: \"Quoted: with a colon\"\n---\nBody\n").is_ok());

    let e = env();
    assert!(create(&e.conn, &e.data, &e.ws, Scope::Kit, None, "x", "X.").is_err());
    assert!(create(&e.conn, &e.data, &e.ws, Scope::Builtin, None, "x", "X.").is_err());
    let builtin = read(&e.conn, &e.data, &e.ws, &SkillRef { scope: Scope::Builtin, name: "nexus:learn-project".into(), project_id: None, kit_id: None }).unwrap();
    assert!(builtin.text.contains("Learn a project for NEXUS") && builtin.hash.is_empty() && !builtin.entry.editable);
}

/// AI Kit skills are listed (read-only) with their kit; the workspace's own kit is listed as workspace skills only.
#[test]
fn kit_skills_are_listed_read_only() {
    let e = env();
    let kit = e.base.join("kit");
    std::fs::create_dir_all(kit.join("review")).unwrap();
    std::fs::write(kit.join("review").join("SKILL.md"), "---\nname: review\ndescription: Review a pull request.\n---\nSteps.\n").unwrap();
    let k = kits::register_local(&e.conn, &e.data, kit.to_str().unwrap(), None, None).unwrap();
    create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "own", "Own.").unwrap();
    let list = list(&e.conn, &e.data, &e.ws).unwrap();
    let ks = entry(&list, Scope::Kit, "review");
    assert_eq!((ks.kit_id.as_deref(), ks.editable, ks.invoke_as.as_str()), (Some(k.id.as_str()), false, "kit:review"));
    assert!(!list.iter().any(|s| s.scope == Scope::Kit && s.name == "own"), "the workspace kit is not listed twice");
    let r = read(&e.conn, &e.data, &e.ws, &SkillRef { scope: Scope::Kit, name: "review".into(), project_id: None, kit_id: Some(k.id.clone()) }).unwrap();
    assert!(r.text.contains("Review a pull request.") && r.hash.is_empty());
}

/// Delete removes a skill folder that holds only SKILL.md — never other files, never a changed version.
#[test]
fn delete_removes_only_a_plain_skill_the_person_saw() {
    let e = env();
    let s = create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "gone", "To delete.").unwrap();
    assert_eq!(delete(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "gone", "stale").unwrap_err().code(), "conflict");
    delete(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "gone", &s.file.hash).unwrap();
    assert!(!e.ws.root_path.join("skills").join("gone").exists());
    let k = kits::workspace_kit(&e.conn, &e.ws.id).unwrap().unwrap();
    assert_eq!(kits::view(&e.conn, &e.data, &k).unwrap().status, "approved", "the approval follows the deletion");

    let s = create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "with-files", "Has scripts.").unwrap();
    std::fs::write(e.ws.root_path.join("skills").join("with-files").join("run.sh"), "echo\n").unwrap();
    assert!(delete(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "with-files", &s.file.hash).unwrap_err().to_string().contains("other file"));
    assert!(e.ws.root_path.join("skills").join("with-files").join("SKILL.md").is_file());
}

#[cfg(unix)]
#[test]
fn nothing_is_written_through_a_symbolic_link() {
    let e = env();
    let outside = e.base.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, e.ws.root_path.join("skills")).unwrap();
    assert!(create(&e.conn, &e.data, &e.ws, Scope::Workspace, None, "x", "X.").unwrap_err().to_string().contains("symbolic link"));
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none(), "nothing written outside");
}
