use std::collections::{HashMap, HashSet};

use super::*;
use crate::knowledge::constitution;

const NOW: &str = "2026-11-05T12:00:00Z";
/// Stack names NEXUS detects (`project::detect`) — a stack rule set for anything else would never apply.
const STACKS: &[&str] = &[
    "angular", "vue", "react", "typescript", "node", "laravel", "php", "python", "rust", "flutter", "dart", "sql", "docker", "nextjs", "nuxt",
    "svelte", "nestjs", "express", "django", "fastapi", "flask", "tauri", "electron", "go", "dotnet", "jvm", "ruby",
];

fn rules(body: &str) -> Vec<constitution::Rule> {
    let (s, _) = constitution::parse(body);
    s.articles.into_iter().flat_map(|a| a.sections).flat_map(|sec| sec.rules).collect()
}

/// Content quality gate for the built-in library: every file installs as a valid document of its kind, ids and
/// prefixes are unique, workflows only use skills the template ships, integrations follow the manifest schema, and only
/// the small core constitution is always on (progressive loading).
#[test]
fn every_template_file_is_valid_and_consistent() {
    assert_eq!(ids(), ["minimal", "developer", "ai-engineering-pro"]);
    for id in ids() {
        let comps = components(&id).unwrap();
        let skills: HashSet<String> = comps.iter().filter(|c| c.path.ends_with("/SKILL.md")).map(|c| c.path.split('/').nth(1).unwrap().to_string()).collect();
        let mut prefixes: HashMap<String, String> = HashMap::new();
        let mut rule_ids = HashSet::new();
        for c in &comps {
            let text = render(c, None, NOW);
            validate_component(c, &text).unwrap_or_else(|e| panic!("{id}: {e}"));
            if c.kind.is_document() {
                assert!(!text.contains("{{"), "{id}/{}: unrendered placeholder", c.path);
            }
            assert!(knowledge::check_portable(&Default::default(), &text).is_ok(), "{id}/{}: credential-shaped text", c.path);
            match c.kind {
                ComponentKind::Constitution => {
                    let doc = Doc::parse(&text).unwrap();
                    let prefix = doc.fm.str("prefix").unwrap_or_else(|| panic!("{}: prefix", c.path)).to_string();
                    assert!(prefixes.insert(prefix.clone(), c.path.clone()).is_none(), "{}: prefix {prefix} used twice", c.path);
                    let (_, problems) = constitution::parse(&doc.body);
                    assert!(problems.is_empty(), "{}: {problems:?}", c.path);
                    let rs = rules(&doc.body);
                    assert!(rs.len() >= 6 && rs.len() <= 18, "{}: {} rules", c.path, rs.len());
                    for (i, r) in rs.iter().enumerate() {
                        assert_eq!(r.id, format!("{prefix}-{}", i + 1), "{}: rule ids are numbered in order", c.path);
                        assert!(rule_ids.insert(r.id.clone()), "{}: rule id {} used twice", c.path, r.id);
                        assert!(!r.text.trim().is_empty() && r.text.chars().count() <= 400, "{}: {} text has {} characters", c.path, r.id, r.text.chars().count());
                        assert!(r.title.chars().count() <= 80, "{}: {} title too long", c.path, r.id);
                        if let Some(g) = &r.check {
                            assert!(["test", "lint", "typecheck", "build"].contains(&g.as_str()), "{}: {} check={g}", c.path, r.id);
                        }
                    }
                    assert_eq!(doc.fm.str("applies") == Some("always"), c.path == "constitutions/core.md", "{}: only the core is always on", c.path);
                    for s in doc.fm.list("stacks") {
                        assert!(STACKS.contains(&s.as_str()), "{}: unknown stack {s}", c.path);
                    }
                    if c.path.starts_with("constitutions/stacks/") {
                        assert!(!doc.fm.list("stacks").is_empty(), "{}: a stack rule set names its stacks", c.path);
                    }
                }
                ComponentKind::Workflow => {
                    let doc = Doc::parse(&text).unwrap();
                    let (d, problems) = crate::workflow::parse(&doc.body);
                    assert!(problems.is_empty(), "{}: {problems:?}", c.path);
                    assert!(crate::workflow::order(&d).is_some(), "{}: steps form an order", c.path);
                    for s in doc.fm.list("skills") {
                        let (kit, name) = s.split_once(':').unwrap();
                        assert_eq!(kit, crate::kits::WORKSPACE_KIT, "{}: {s}", c.path);
                        assert!(skills.contains(name), "{id}/{}: skill {name} is not in the template", c.path);
                    }
                }
                ComponentKind::Skill if c.path.ends_with("/SKILL.md") => {
                    let folder = c.path.split('/').nth(1).unwrap();
                    let header: Vec<&str> = text.lines().skip(1).take_while(|l| l.trim() != "---").collect();
                    let field = |k: &str| header.iter().find_map(|l| l.strip_prefix(&format!("{k}:"))).map(|v| v.trim().trim_matches('"').to_string());
                    assert_eq!(field("name").as_deref(), Some(folder), "{}: name equals the folder", c.path);
                    let d = field("description").unwrap_or_default();
                    assert!(!d.is_empty() && d.chars().count() <= 160, "{}: description has {} characters", c.path, d.chars().count());
                    assert!(text.lines().count() <= 160, "{}: SKILL.md stays focused ({} lines)", c.path, text.lines().count());
                }
                ComponentKind::Integration => {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                    for key in ["nexus", "id", "name", "kind", "summary", "homepage", "license", "verifiedAt", "enabled", "requires", "provides", "install", "nexusDelivery", "dataFlow", "security", "usage"] {
                        assert!(v.get(key).is_some(), "{}: missing {key}", c.path);
                    }
                    assert_eq!(v["enabled"], false, "{}: integrations start disabled", c.path);
                    assert!(["mcp-server", "cli-tool", "claude-skill"].contains(&v["kind"].as_str().unwrap()), "{}", c.path);
                    for t in v["requires"].as_array().unwrap() {
                        assert!(crate::toolchain::tool(t.as_str().unwrap()).is_some(), "{}: requires {t}", c.path);
                    }
                    if let Some(p) = v["provides"].as_str() {
                        assert!(crate::toolchain::tool(p).is_some(), "{}: provides {p}", c.path);
                    }
                    assert_eq!(format!("integrations/{}.json", v["id"].as_str().unwrap()), c.path);
                }
                _ => {}
            }
        }
    }
    let pro = summary("ai-engineering-pro").unwrap();
    assert!(pro.always_on_chars > 500 && pro.always_on_chars < 1_000, "the always-on core stays small: {} characters", pro.always_on_chars);
    assert_eq!((pro.skills, pro.workflows), (11, 7));
}

fn workspace_fixture() -> (tempfile::TempDir, Connection, Workspace, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let ws = workspace::create(&conn, t.path().join("ws").to_str().unwrap(), "W", "").unwrap();
    let data = t.path().join("data");
    (t, conn, ws, data)
}

#[test]
fn installing_the_pro_template_seeds_an_approved_progressive_library() {
    let (_t, conn, ws, data) = workspace_fixture();
    let p = plan(&ws.root_path, "ai-engineering-pro").unwrap();
    assert!(p.items.iter().all(|i| i.action == Action::Add));
    let done = apply(&conn, &data, &ws, "ai-engineering-pro").unwrap();
    assert_eq!(done.written.len(), p.items.len());
    let m = workspace::read_manifest(&ws.root_path).unwrap();
    assert_eq!(m.template.as_ref().map(|t| t.id.as_str()), Some("ai-engineering-pro"));
    assert!(m.has_feature("development-records") && m.has_feature("integrations"));
    let lock = read_lock(&ws.root_path).unwrap().unwrap();
    assert_eq!(lock.components.len(), done.written.len());

    // Every document is indexed as valid and approved on this machine.
    let report = knowledge::service::refresh(&conn, &data, &ws).unwrap();
    assert_eq!(report.invalid, 0);
    let items = knowledge::index::list(&conn, &ws.id, &Default::default()).unwrap();
    assert!(items.len() >= 30);
    for i in &items {
        assert!(knowledge::index::is_approved(&conn, i).unwrap(), "{} is approved", i.title);
    }
    // The skills reach this workspace's sessions as the `workspace` kit.
    let kit = crate::kits::workspace_kit(&conn, &ws.id).unwrap().unwrap();
    assert_eq!((kit.name.as_str(), kit.approved()), (crate::kits::WORKSPACE_KIT, true));
    let v = crate::kits::view(&conn, &data, &kit).unwrap();
    assert_eq!(v.inventory.skills.len(), 11);

    // Progressive delivery for a Vue + TypeScript project: only the core text, everything else by document.
    let vue = vec!["vue".to_string(), "typescript".to_string(), "node".to_string()];
    let target = constitution::Target { project: Some(("p", vue.as_slice())), task_id: None, include_pending: false };
    let e = constitution::effective_indexed(&conn, &ws.id, &target).unwrap();
    assert!(e.rules.iter().any(|r| r.id.starts_with("VUE-")) && !e.rules.iter().any(|r| r.id.starts_with("NG-")), "stack rules follow the project");
    let d = constitution::delivery(&e, 1_500, true, crate::context::ConstitutionDelivery::Rules);
    let text = d.text.unwrap();
    assert!(text.chars().count() <= 1_500, "{} characters", text.chars().count());
    assert!(text.contains("CORE-1") && text.contains("VUE-*") && text.contains("Testing TST-*") && text.contains("Security SEC-*"), "{text}");
    // Mandatory policies first, then the project's stack documents before workspace-wide guidance.
    let at = |s: &str| text.find(s).unwrap_or(usize::MAX);
    assert!(at("SEC-*") < at("VUE-*") && at("VUE-*") < at("CQ-*"), "{text}");
    assert!(!text.contains("TST-1"), "on-demand rules are listed by document, not one by one: {text}");
}

#[test]
fn updates_never_overwrite_changes_and_failed_installs_change_nothing() {
    let (_t, conn, ws, data) = workspace_fixture();
    apply(&conn, &data, &ws, "developer").unwrap();
    let root = &ws.root_path;
    let again = plan(root, "developer").unwrap();
    assert!(again.is_noop(), "a second install changes nothing: {:?}", again.items.iter().filter(|i| i.action != Action::Unchanged).collect::<Vec<_>>());

    // The person changes one file, deletes another, and an unchanged one has an older text (simulated).
    let testing = root.join("constitutions").join("testing.md");
    let custom = std::fs::read_to_string(&testing).unwrap().replace("## ", "## Our ");
    std::fs::write(&testing, &custom).unwrap();
    std::fs::remove_file(root.join("constitutions").join("git.md")).unwrap();
    let docs = root.join("constitutions").join("documentation.md");
    let old = std::fs::read_to_string(&docs).unwrap().replace("## ", "## Old ");
    std::fs::write(&docs, &old).unwrap();
    let mut lock = read_lock(root).unwrap().unwrap();
    lock.components.get_mut("constitutions/documentation.md").unwrap().sha256 = sha256(old.as_bytes());
    std::fs::write(root.join(LOCK_FILE), serde_json::to_string(&lock).unwrap()).unwrap();

    let p = plan(root, "ai-engineering-pro").unwrap();
    let action = |path: &str| p.items.iter().find(|i| i.path == path).unwrap().action;
    assert_eq!(action("constitutions/testing.md"), Action::KeepCustomized);
    assert_eq!(action("constitutions/git.md"), Action::KeepRemoved);
    assert_eq!(action("constitutions/documentation.md"), Action::Update);
    assert_eq!(action("constitutions/security.md"), Action::Add);
    let id_before = Doc::parse(&old).unwrap().id().unwrap().to_string();
    let done = apply(&conn, &data, &ws, "ai-engineering-pro").unwrap();
    assert_eq!(std::fs::read_to_string(&testing).unwrap(), custom, "the person's version is kept");
    assert!(done.saved_for_review.contains(&format!("{UPDATES_DIR}/constitutions/testing.md")));
    assert!(!root.join("constitutions").join("git.md").exists(), "a deleted file is not added again");
    let updated = std::fs::read_to_string(&docs).unwrap();
    assert!(!updated.contains("## Old ") && Doc::parse(&updated).unwrap().id() == Some(id_before.as_str()), "updated in place with the same id");
    let gi = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(gi.lines().any(|l| l == ".nexus-local/"), "review copies stay local");

    // A write that fails half-way leaves everything as it was.
    let (_t2, conn2, ws2, data2) = workspace_fixture();
    std::fs::remove_dir_all(ws2.root_path.join("workflows")).unwrap();
    std::fs::write(ws2.root_path.join("workflows"), "a file where the workflows folder must go").unwrap();
    let before = std::fs::read_to_string(ws2.root_path.join(workspace::MANIFEST_FILE)).unwrap();
    let err = apply(&conn2, &data2, &ws2, "developer").unwrap_err();
    assert!(err.to_string().contains("nothing was changed"), "{err}");
    assert!(!ws2.root_path.join("constitutions").join("core.md").exists(), "files written before the failure are removed again");
    assert!(!ws2.root_path.join(LOCK_FILE).exists());
    assert_eq!(std::fs::read_to_string(ws2.root_path.join(workspace::MANIFEST_FILE)).unwrap(), before);
}

#[test]
fn a_read_only_workspace_refuses_templates_and_a_users_own_file_is_never_replaced() {
    let (_t, conn, ws, data) = workspace_fixture();
    std::fs::create_dir_all(ws.root_path.join("constitutions")).unwrap();
    std::fs::write(ws.root_path.join("constitutions").join("core.md"), "mine").unwrap();
    let p = plan(&ws.root_path, "minimal").unwrap();
    assert_eq!(p.items[0].action, Action::SkipExisting);
    apply(&conn, &data, &ws, "minimal").unwrap();
    assert_eq!(std::fs::read_to_string(ws.root_path.join("constitutions").join("core.md")).unwrap(), "mine");

    let mut m = workspace::read_manifest(&ws.root_path).unwrap();
    m.schema_version = 9;
    std::fs::write(ws.root_path.join(workspace::MANIFEST_FILE), serde_json::to_string(&m).unwrap()).unwrap();
    assert_eq!(apply(&conn, &data, &ws, "developer").unwrap_err().code(), "unsupported");
}
