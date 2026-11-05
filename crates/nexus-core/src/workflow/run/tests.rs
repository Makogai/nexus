use super::*;
use crate::workflow::{self, NewWorkflow, gates::GateInput};

struct Fx {
    _t: tempfile::TempDir,
    conn: Connection,
    data: std::path::PathBuf,
    ws: Workspace,
    project: crate::project::Project,
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .args(["-c", "user.name=T", "-c", "user.email=t@example.invalid", "-c", "core.autocrlf=false"])
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// Define and approve a gate (the person reviewed the command).
fn gate(conn: &Connection, project_id: &str, g: &GateInput) -> gates::Gate {
    let d = gates::upsert(conn, project_id, g).unwrap();
    gates::approve(conn, project_id, &d.name, &d.command_hash).unwrap()
}

fn fx() -> Fx {
    let t = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let ws = crate::workspace::create(&conn, root.join("ws").to_str().unwrap(), "W", "").unwrap();
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    let project = crate::project::register(&conn, &ws.id, repo.to_str().unwrap(), None).unwrap();
    let ok = |name: &str| GateInput { name: name.into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() };
    for g in ["test", "lint"] {
        gate(&conn, &project.id, &ok(g));
    }
    gate(&conn, &project.id, &GateInput { name: "broken".into(), program: "git".into(), args: vec!["rev-parse".into(), "--verify".into(), "no-such-ref".into()], ..Default::default() });
    Fx { data: root.join("data"), _t: t, conn, ws, project }
}

const BODY: &str = "## Plan
<!-- step: id=plan kind=claude -->
Plan it.
### Acceptance
- Plan recorded

## Implement
<!-- step: id=implement kind=claude needs=plan gates=test -->
Build it.

## Static checks
<!-- step: id=checks kind=check needs=implement gates=lint -->

## Docs
<!-- step: id=docs kind=claude needs=implement rigor=professional -->

## Review
<!-- step: id=review kind=review needs=checks,docs -->
";

fn workflow(f: &Fx, body: &str) -> String {
    let v = workflow::create(&f.conn, &f.data, &f.ws, &NewWorkflow { title: "Ship".into(), category: "feature".into(), ..Default::default() }).unwrap();
    workflow::save_raw(&f.conn, &f.data, &f.ws, &v.summary.id, body, Some(&v.summary.hash)).unwrap().summary.id
}

fn start(f: &Fx, wf: &str, rigor: &str) -> RunView {
    run_start(f, wf, rigor, None)
}

fn run_start(f: &Fx, wf: &str, rigor: &str, task: Option<String>) -> RunView {
    super::start(&f.conn, &f.data, &f.ws, &StartRun { workflow_id: wf.into(), project_id: Some(f.project.id.clone()), task_id: task, goal: "Add <checkout>".into(), rigor: Some(rigor.into()), ..Default::default() }).unwrap()
}

fn status(v: &RunView, step: &str) -> String {
    v.steps.iter().find(|s| s.state.step_id == step).unwrap().state.status.clone()
}

fn verify(f: &Fx) -> usize {
    verify_pending(|g: &mut dyn FnMut(&Connection) -> Result<()>| g(&f.conn), &f.data, "host-1", &crate::env::current()).unwrap()
}

fn categories(f: &Fx, run: &str) -> Vec<String> {
    notify::list(&f.conn, &notify::Filter { include_dismissed: true, ..Default::default() }).unwrap().into_iter().filter(|n| n.workflow_id.as_deref() == Some(run)).map(|n| n.category).collect()
}

#[test]
fn a_run_goes_from_ai_reported_to_verified_to_signed_off() {
    let f = fx();
    let wf = workflow(&f, BODY);
    let v = start(&f, &wf, "balanced");
    let id = v.run.id.clone();
    assert_eq!((status(&v, "plan"), status(&v, "docs")), ("ready".into(), "skipped".into()), "docs only at professional rigor");
    let task = tasks::view(&f.conn, &f.ws, &v.run.task_id, 4000).unwrap();
    assert_eq!(task.title, "Ship: Add <checkout>");
    assert_eq!(task.checklist.len(), 4, "one checklist item per included step");

    // AI-reported: no gate on `plan`.
    report(&f.conn, &f.data, &f.ws, &id, "plan", &StepReport { status: Reported::Done, summary: "Plan <b>recorded</b>", criteria: &[1, 9] }).unwrap();
    let v = view(&f.conn, &id).unwrap();
    let plan = &v.steps[0].state;
    assert_eq!((plan.status.as_str(), plan.verification.as_deref(), plan.criteria_met.clone()), ("passed", Some("ai-reported"), vec![1]));
    assert_eq!(plan.report.as_deref(), Some("Plan ‹b›recorded‹/b›"), "Claude's text is escaped data");
    // Verified: `implement` has a gate; Claude's report only moves it to verifying.
    report(&f.conn, &f.data, &f.ws, &id, "implement", &StepReport { status: Reported::Done, summary: "Built", criteria: &[] }).unwrap();
    assert_eq!(status(&view(&f.conn, &id).unwrap(), "implement"), "verifying");
    assert!(report(&f.conn, &f.data, &f.ws, &id, "checks", &StepReport { status: Reported::Done, summary: "x", criteria: &[] }).is_err(), "Claude cannot report a check step");
    assert_eq!(verify(&f), 1);
    let v = view(&f.conn, &id).unwrap();
    let imp = v.steps.iter().find(|s| s.state.step_id == "implement").unwrap();
    assert_eq!((imp.state.status.as_str(), imp.state.verification.as_deref()), ("passed", Some("verified")));
    assert_eq!((imp.results[0].status.as_str(), imp.results[0].exit_code), ("passed", Some(0)));
    assert_eq!(status(&v, "checks"), "verifying", "a check step runs on its own");
    assert_eq!(verify(&f), 1);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!((status(&v, "review"), v.run.status.as_str()), ("awaiting-review".into(), "waiting"));
    assert!(categories(&f, &id).contains(&"review-required".to_string()));
    let v = super::review(&f.conn, &f.data, &f.ws, &id, "review", true, "LGTM").unwrap();
    assert_eq!(v.run.status, "completed");
    assert_eq!((v.progress.verified, v.progress.ai_reported, v.progress.human, v.progress.skipped), (2, 1, 1, 1));
    assert!(categories(&f, &id).contains(&"workflow-completed".to_string()));
    let task = tasks::view(&f.conn, &f.ws, &v.run.task_id, 4000).unwrap();
    assert_eq!(task.status, "done");
    assert_eq!(task.checked(), 4);
    assert!(task.recent_log.iter().any(|l| l.contains("verified by test")) && task.recent_log.iter().any(|l| l.contains("AI-reported")));
}

#[test]
fn failing_gates_go_back_to_claude_then_fail_and_can_be_retried() {
    let f = fx();
    let wf = workflow(&f, "## Implement\n<!-- step: id=implement kind=claude gates=broken -->\nBuild.\n");
    let id = start(&f, &wf, "professional").run.id; // professional: 2 attempts
    for attempt in 1..=2 {
        report(&f.conn, &f.data, &f.ws, &id, "implement", &StepReport { status: Reported::Done, summary: "done", criteria: &[] }).unwrap();
        verify(&f);
        let v = view(&f.conn, &id).unwrap();
        let s = &v.steps[0].state;
        assert_eq!(s.attempts, attempt);
        assert!(s.last_failure.as_deref().unwrap().contains("gate broken failed"), "{s:?}");
        if attempt == 1 {
            assert_eq!(s.status, "ready", "back to Claude with the failure");
        } else {
            assert_eq!((s.status.as_str(), v.run.status.as_str()), ("failed", "failed"));
        }
    }
    assert!(categories(&f, &id).contains(&"workflow-failed".to_string()));
    let task = tasks::view(&f.conn, &f.ws, &view(&f.conn, &id).unwrap().run.task_id, 100).unwrap();
    assert_eq!(task.status, "blocked");
    // A person retries: one more attempt, the run is running again.
    let v = retry(&f.conn, &f.data, &f.ws, &id, "implement").unwrap();
    assert_eq!((v.run.status.as_str(), v.steps[0].state.status.as_str(), v.steps[0].state.max_attempts), ("running", "ready", 3));
    gate(&f.conn, &f.project.id, &GateInput { name: "broken".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() });
    report(&f.conn, &f.data, &f.ws, &id, "implement", &StepReport { status: Reported::Done, summary: "fixed", criteria: &[] }).unwrap();
    verify(&f);
    assert_eq!(view(&f.conn, &id).unwrap().run.status, "completed");
}

#[test]
fn rigor_selects_steps_and_critical_needs_sign_off_without_gates() {
    let f = fx();
    let wf = workflow(&f, BODY);
    let v = start(&f, &wf, "professional");
    assert_eq!(status(&v, "docs"), "pending", "included at professional");
    let wf2 = workflow(&f, "## Plan\n<!-- step: id=plan kind=claude -->\nPlan.\n");
    let id = start(&f, &wf2, "critical").run.id;
    report(&f.conn, &f.data, &f.ws, &id, "plan", &StepReport { status: Reported::Done, summary: "planned", criteria: &[] }).unwrap();
    let v = view(&f.conn, &id).unwrap();
    assert_eq!(status(&v, "plan"), "awaiting-review", "critical: an AI-reported step without gates needs a person");
    let v = super::review(&f.conn, &f.data, &f.ws, &id, "plan", true, "").unwrap();
    assert_eq!((v.run.status.as_str(), v.steps[0].state.verification.as_deref()), ("completed", Some("human")));
}

#[test]
fn rejected_review_reopens_claude_steps_missing_gates_block_and_input_requests_notify() {
    let f = fx();
    let wf = workflow(&f, "## Build\n<!-- step: id=build kind=claude -->\nB.\n\n## Review\n<!-- step: id=review kind=review needs=build -->\n");
    let id = start(&f, &wf, "balanced").run.id;
    report(&f.conn, &f.data, &f.ws, &id, "build", &StepReport { status: Reported::NeedsInput, summary: "Which API version?", criteria: &[] }).unwrap();
    assert!(categories(&f, &id).contains(&"input-required".to_string()));
    report(&f.conn, &f.data, &f.ws, &id, "build", &StepReport { status: Reported::Done, summary: "built", criteria: &[] }).unwrap();
    let v = super::review(&f.conn, &f.data, &f.ws, &id, "review", false, "Missing error handling").unwrap();
    assert_eq!((status(&v, "build"), status(&v, "review")), ("ready".into(), "pending".into()));
    assert!(v.steps[0].state.last_failure.as_deref().unwrap().contains("Missing error handling"));

    let wf = workflow(&f, "## Check\n<!-- step: id=check kind=check gates=typecheck -->\n");
    let id = start(&f, &wf, "balanced").run.id;
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!(status(&v, "check"), "blocked");
    assert!(v.steps[0].state.last_failure.as_deref().unwrap().contains("typecheck are not defined"));
    assert!(categories(&f, &id).contains(&"input-required".to_string()));
    gate(&f.conn, &f.project.id, &GateInput { name: "typecheck".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() });
    retry(&f.conn, &f.data, &f.ws, &id, "check").unwrap();
    verify(&f);
    assert_eq!(view(&f.conn, &id).unwrap().run.status, "completed");
}

#[test]
fn cancellation_crash_recovery_and_one_active_run_per_task() {
    let f = fx();
    let wf = workflow(&f, "## Check\n<!-- step: id=check kind=check gates=test -->\n");
    let v = start(&f, &wf, "balanced");
    let id = v.run.id.clone();
    assert!(super::start(&f.conn, &f.data, &f.ws, &StartRun { workflow_id: wf.clone(), task_id: Some(v.run.task_id.clone()), ..Default::default() }).is_err(), "one active run per task");
    // Crash: the host claimed the step and died while the gate was running.
    let p = pending_verifications(&f.conn).unwrap();
    assert_eq!(p.len(), 1);
    let c = claim(&f.conn, &p[0], "dead-host").unwrap().unwrap();
    assert!(pending_verifications(&f.conn).unwrap().is_empty(), "claimed: not run twice");
    assert_eq!(reconcile(&f.conn, now_ms()).unwrap(), 1);
    assert_eq!(gates::get_result(&f.conn, &c.gates[0].1).unwrap().status, "interrupted");
    assert_eq!(pending_verifications(&f.conn).unwrap().len(), 1, "verified again after the crash");
    let v = cancel(&f.conn, &f.data, &f.ws, &id).unwrap();
    assert_eq!((v.run.status.as_str(), status(&v, "check")), ("cancelled", "cancelled".into()));
    assert!(pending_verifications(&f.conn).unwrap().is_empty());
    assert!(retry(&f.conn, &f.data, &f.ws, &id, "check").is_err());
    // Brief for Claude is bounded data.
    let b = brief(&view(&f.conn, &id).unwrap(), 2000);
    assert!(b.starts_with("<nexus-workflow-run") && b.contains("[check]"));
    // A run on an existing task (from a task card) leaves its checklist alone.
    let task = service::create(&f.conn, &f.data, &f.ws, &CreateInput { kind: "task".into(), title: "Existing".into(), body: "- [ ] mine\n".into(), ..Default::default() }, Author::Human).unwrap();
    let v = run_start(&f, &wf, "balanced", task.id.clone());
    assert_eq!(v.run.task_id, task.id.clone().unwrap());
    assert_eq!(tasks::view(&f.conn, &f.ws, &v.run.task_id, 100).unwrap().checklist.len(), 1);
}

#[test]
fn constitution_rules_linked_to_gates_are_verified_others_stay_instructed() {
    use crate::knowledge::constitution::{self, NewConstitution, Target};
    let f = fx();
    let c = constitution::create(&f.conn, &f.data, &f.ws, &NewConstitution { title: "Team".into(), scope: "workspace".into(), ..Default::default() }).unwrap();
    let body = "## Quality\n#### Lint is clean\n<!-- rule: id=Q-1 enforcement=mandatory check=lint -->\nNo lint errors.\n#### Small commits\n<!-- rule: id=Q-2 enforcement=guideline -->\nKeep commits small.\n";
    constitution::save_raw(&f.conn, &f.data, &f.ws, &c.summary.id, body, Some(&c.summary.hash)).unwrap();
    let stacks = f.project.stack.stacks.clone();
    let comp = |f: &Fx| {
        let e = constitution::effective(&f.conn, &f.data, &f.ws, &Target { project: Some((&f.project.id, &stacks)), ..Default::default() }).unwrap();
        constitution::compliance(&f.conn, Some(&f.project.id), &e).unwrap().into_iter().map(|c| (c.rule_id, c.status)).collect::<Vec<_>>()
    };
    assert_eq!(comp(&f), [("Q-1".into(), "not-run"), ("Q-2".into(), "instructed")], "applying a rule is not verifying it");
    let wf = workflow(&f, "## Constitution checks\n<!-- step: id=rules kind=check gates=constitution -->\n");
    let id = start(&f, &wf, "balanced").run.id;
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!((v.run.status.as_str(), v.steps[0].results[0].gate_name.as_str()), ("completed", "lint"), "a constitution step runs the linked gates");
    assert_eq!(comp(&f)[0], ("Q-1".into(), "passed"));
    gates::upsert(&f.conn, &f.project.id, &GateInput { name: "lint".into(), program: "git".into(), args: vec!["status".into()], ..Default::default() }).unwrap();
    assert_eq!(comp(&f)[0].1, "not-approved", "a changed gate command is not evidence until approved");
    gates::remove(&f.conn, &f.project.id, "lint").unwrap();
    assert_eq!(comp(&f)[0].1, "gate-missing");
}

#[test]
fn gate_commands_run_only_in_their_approved_version() {
    let f = fx();
    let wf = workflow(&f, "## Check
<!-- step: id=check kind=check gates=audit -->
");
    let g = gates::upsert(&f.conn, &f.project.id, &GateInput { name: "audit".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() }).unwrap();
    let id = start(&f, &wf, "balanced").run.id;
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!(status(&v, "check"), "blocked", "a new gate does not run before approval");
    assert!(v.steps[0].state.last_failure.as_deref().unwrap().contains("audit (version 1) are not approved"));
    assert!(v.steps[0].results.is_empty(), "nothing was executed");
    assert!(categories(&f, &id).contains(&"input-required".to_string()));
    assert_eq!(approval_impact(&f.conn, &f.project.id, "audit").unwrap(), ApprovalImpact { steps: 1, runs: 1, still_waiting: 0 });
    gates::approve(&f.conn, &f.project.id, "audit", &g.command_hash).unwrap();
    assert_eq!(resume_gate_waits(&f.conn, &f.project.id).unwrap().steps, 1, "approval resumes the waiting step");
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!((v.run.status.as_str(), v.steps[0].results[0].gate_version), ("completed", Some(1)));
    // The command changes: version 2 needs approval again before a new run can use it.
    gates::upsert(&f.conn, &f.project.id, &GateInput { name: "audit".into(), program: "git".into(), args: vec!["status".into()], ..Default::default() }).unwrap();
    let id2 = start(&f, &wf, "balanced").run.id;
    verify(&f);
    let v = view(&f.conn, &id2).unwrap();
    assert!(v.steps[0].state.last_failure.as_deref().unwrap().contains("audit (version 2) are not approved"));
}

#[test]
fn reported_work_waiting_for_gate_approval_resumes_without_a_new_report() {
    let f = fx();
    let wf = workflow(&f, "## Build
<!-- step: id=build kind=claude gates=unit -->
");
    let g = gates::upsert(&f.conn, &f.project.id, &GateInput { name: "unit".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() }).unwrap();
    let id = start(&f, &wf, "balanced").run.id;
    report(&f.conn, &f.data, &f.ws, &id, "build", &StepReport { status: Reported::Done, summary: "Built", criteria: &[] }).unwrap();
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!((v.steps[0].state.status.as_str(), v.steps[0].state.blocked_on.as_deref(), v.run.status.as_str()), ("blocked", Some("gates"), "waiting"));
    // Approving the command resumes verification; Claude does not have to report again.
    gates::approve(&f.conn, &f.project.id, "unit", &g.command_hash).unwrap();
    assert_eq!(resume_gate_waits(&f.conn, &f.project.id).unwrap(), ApprovalImpact { steps: 1, runs: 1, still_waiting: 0 });
    verify(&f);
    let v = view(&f.conn, &id).unwrap();
    assert_eq!((v.run.status.as_str(), v.steps[0].state.verification.as_deref(), v.steps[0].state.blocked_on.as_deref()), ("completed", Some("verified"), None));
    assert_eq!(v.steps[0].state.attempts, 1, "still the first attempt");
    let open: Vec<_> = notify::list(&f.conn, &notify::Filter { include_dismissed: true, ..Default::default() })
        .unwrap()
        .into_iter()
        .filter(|n| n.workflow_id.as_deref() == Some(id.as_str()) && n.category == "input-required" && n.resolved_at.is_none())
        .collect();
    assert!(open.is_empty(), "the approval request is resolved");
}

#[test]
fn approval_preview_counts_exactly_the_steps_and_runs_that_resume() {
    let f = fx();
    let wf = workflow(&f, "## Build
<!-- step: id=build kind=claude gates=unit -->

## Docs
<!-- step: id=docs kind=claude gates=unit,e2e -->
");
    let unit = gates::upsert(&f.conn, &f.project.id, &GateInput { name: "unit".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() }).unwrap();
    let lint = gates::upsert(&f.conn, &f.project.id, &GateInput { name: "e2e".into(), program: "git".into(), args: vec!["--version".into()], ..Default::default() }).unwrap();
    let mut ids = Vec::new();
    for _ in 0..2 {
        let id = start(&f, &wf, "balanced").run.id; // each run gets its own task
        for step in ["build", "docs"] {
            report(&f.conn, &f.data, &f.ws, &id, step, &StepReport { status: Reported::Done, summary: "done", criteria: &[] }).unwrap();
        }
        ids.push(id);
    }
    verify(&f);
    // Four steps wait: two need only `unit`, two need `unit` and `e2e`.
    assert_eq!(gate_waits(&f.conn, &f.project.id).unwrap().len(), 4);
    assert_eq!(approval_impact(&f.conn, &f.project.id, "unit").unwrap(), ApprovalImpact { steps: 2, runs: 2, still_waiting: 2 });
    assert_eq!(approval_impact(&f.conn, &f.project.id, "e2e").unwrap(), ApprovalImpact { steps: 0, runs: 0, still_waiting: 2 });
    // The preview is what happens: approving `unit` resumes exactly those two; the others keep waiting (no re-block).
    gates::approve(&f.conn, &f.project.id, "unit", &unit.command_hash).unwrap();
    assert_eq!(resume_gate_waits(&f.conn, &f.project.id).unwrap(), ApprovalImpact { steps: 2, runs: 2, still_waiting: 2 });
    assert_eq!(approval_impact(&f.conn, &f.project.id, "e2e").unwrap(), ApprovalImpact { steps: 2, runs: 2, still_waiting: 0 });
    gates::approve(&f.conn, &f.project.id, "e2e", &lint.command_hash).unwrap();
    assert_eq!(resume_gate_waits(&f.conn, &f.project.id).unwrap().steps, 2);
    verify(&f);
    for id in &ids {
        assert_eq!(view(&f.conn, id).unwrap().run.status, "completed");
    }
}
