use super::*;

fn fixture() -> (tempfile::TempDir, Connection, Workspace, std::path::PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let ws = workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
    let data = t.path().join("data");
    (t, conn, ws, data)
}

fn check(conn: &Connection, data: &Path, ws: &Workspace) -> Report {
    let home = data.parent().unwrap().to_path_buf();
    run(conn, data, &workspace::get(conn, &ws.id).unwrap(), &Options { home: &home, probe_tools: false }).unwrap()
}

fn find<'a>(r: &'a Report, check: &str) -> Option<&'a Finding> {
    r.findings.iter().find(|f| f.check == check)
}

#[test]
fn a_new_workspace_is_healthy_and_says_what_is_optional() {
    let (_t, conn, ws, data) = fixture();
    let r = check(&conn, &data, &ws);
    assert_eq!(r.counts.error, 0, "{:?}", r.findings);
    assert_eq!(r.counts.warn, 0, "{:?}", r.findings);
    assert_eq!(find(&r, "schema").unwrap().status, Status::Ok);
    assert_eq!(find(&r, "template").unwrap().status, Status::Info, "no template yet is information, not a problem");
    assert_eq!(find(&r, "sync").unwrap().navigate, Some("sync"));
}

#[test]
fn problems_come_with_recommendations_and_reviewable_repairs() {
    let (_t, conn, ws, data) = fixture();
    let root = ws.root_path.clone();
    std::fs::remove_file(root.join("README.md")).unwrap();
    std::fs::write(root.join(".gitignore"), "# mine\n").unwrap();
    std::fs::create_dir_all(root.join("rules")).unwrap();
    std::fs::write(root.join("rules").join("broken.md"), "---\nnexus: 1\nkind: rule\n---\nno id\n").unwrap();
    std::fs::create_dir_all(root.join("skills").join("deploy")).unwrap();
    std::fs::write(root.join("skills").join("deploy").join("SKILL.md"), "---\nname: deploy\ndescription: Deploys\n---\nSteps\n").unwrap();
    std::fs::create_dir_all(root.join("integrations")).unwrap();
    std::fs::write(root.join("integrations").join("odd.json"), r#"{"id":"odd","enabled":true,"requires":["definitely-not-a-tool"],"provides":null}"#).unwrap();
    std::fs::write(root.join("integrations").join("broken.json"), "{").unwrap();
    crate::workflow::create(
        &conn,
        &data,
        &ws,
        &crate::workflow::NewWorkflow { title: "Ship".into(), template: Some("feature".into()), skills: vec!["workspace:missing-skill".into()], ..Default::default() },
    )
    .unwrap();

    let r = check(&conn, &data, &ws);
    let readme = find(&r, "readme").unwrap();
    assert_eq!(readme.repair.as_ref().unwrap().id, "readme");
    assert_eq!(find(&r, "gitignore").unwrap().detail, ["projects/", "repos/", ".nexus-local/"]);
    assert!(find(&r, "knowledge-invalid").unwrap().detail[0].starts_with("rules/broken.md"));
    let skills = find(&r, "workspace-skills").unwrap();
    assert_eq!((skills.status, skills.repair.as_ref().unwrap().id.as_str()), (Status::Warn, "register-workspace-skills"));
    assert!(find(&r, "workflow-skills").unwrap().detail[0].contains("workspace:missing-skill"));
    let integ: Vec<&str> = r.findings.iter().filter(|f| f.check == "integrations").map(|f| f.summary.as_str()).collect();
    assert!(integ.iter().any(|s| s.contains("broken") && s.contains("not valid JSON")), "{integ:?}");
    assert!(integ.iter().any(|s| s.contains("unknown tool")), "a manifest cannot name a program: {integ:?}");
    assert_eq!(r.findings.first().unwrap().status, Status::Warn, "most severe first");

    // Each repair shows its plan, then applies exactly that.
    for id in ["readme", "gitignore", "register-workspace-skills"] {
        let p = repair_plan(&conn, &data, &ws, id).unwrap();
        assert!(!p.changes.is_empty(), "{id}");
        repair(&conn, &data, &ws, id).unwrap();
    }
    assert!(root.join("README.md").is_file());
    assert!(std::fs::read_to_string(root.join(".gitignore")).unwrap().starts_with("# mine\n"));
    let r = check(&conn, &data, &ws);
    assert!(find(&r, "readme").is_none() && find(&r, "gitignore").is_none());
    let skills = find(&r, "workspace-skills").unwrap();
    assert!(skills.summary.contains("wait for review") && skills.navigate == Some("kits"), "registering is not approving: {skills:?}");
    assert!(repair_plan(&conn, &data, &ws, "rm -rf").is_err());
}

#[test]
fn old_schemas_and_older_templates_are_repaired_on_request() {
    let (_t, conn, ws, data) = fixture();
    let root = ws.root_path.clone();
    templates::apply(&conn, &data, &ws, "developer").unwrap();
    // An older schema and an older template version (as a workspace from an earlier NEXUS would have).
    let mut m = workspace::read_manifest(&root).unwrap();
    m.schema_version = 1;
    m.template.as_mut().unwrap().version = "0.9.0".into();
    std::fs::write(root.join(workspace::MANIFEST_FILE), serde_json::to_string(&m).unwrap()).unwrap();

    let r = check(&conn, &data, &ws);
    let schema = find(&r, "schema").unwrap();
    assert_eq!(schema.repair.as_ref().unwrap().id, "migrate-schema");
    let tpl = find(&r, "template").unwrap();
    assert_eq!(tpl.repair.as_ref().unwrap().id, "update-template:developer");
    let p = repair_plan(&conn, &data, &ws, "migrate-schema").unwrap();
    assert!(p.changes.iter().any(|c| c.contains("nexus-workspace.json")));
    repair(&conn, &data, &ws, "migrate-schema").unwrap();
    repair(&conn, &data, &ws, "update-template:developer").unwrap();
    let r = check(&conn, &data, &ws);
    assert_eq!(find(&r, "schema").unwrap().status, Status::Ok);
    assert_eq!(find(&r, "template").unwrap().status, Status::Ok, "{:?}", find(&r, "template"));
    assert_eq!(find(&r, "workspace-skills").unwrap().status, Status::Ok, "the template's skills are approved");
}
