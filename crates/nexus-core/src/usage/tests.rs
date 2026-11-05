use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};

use super::report::{self, Dimension, Filter};
use super::*;
use crate::claude::profile::{self, ProfileInput};
use crate::session::NewSession;

const CSID: &str = "0b7c2a3e-1111-4222-8333-944455556666";
const T0: i64 = 1_793_000_000_000; // a fixed day in 2026

struct Fx {
    conn: Connection,
    _t: tempfile::TempDir,
    home: PathBuf,
    cfg: PathBuf,
    profile: String,
}

fn fx() -> Fx {
    let t = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(t.path()).unwrap();
    let conn = crate::db::open_in_memory().unwrap();
    let (home, cfg) = (root.join("home"), root.join("cfg"));
    std::fs::create_dir_all(&home).unwrap();
    let p = profile::create_managed(&conn, &ProfileInput { name: "Work".into(), config_dir: Some(cfg.to_string_lossy().into_owned()), ..Default::default() }, &home).unwrap();
    // Billed per API use (an API key), so Claude Code's cost estimate may be shown.
    record_billing(&conn, &p.id, &api_key()).unwrap();
    Fx { conn, _t: t, home, cfg, profile: p.id }
}

fn api_key() -> crate::claude::auth::AuthStatus {
    crate::claude::auth::AuthStatus { logged_in: true, auth_method: Some("api_key".into()), ..Default::default() }
}

fn iso(ms: i64) -> String {
    // Civil from days (inverse of transcript::parse_ts), UTC.
    let (secs, millis) = (ms.div_euclid(1000), ms.rem_euclid(1000));
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{millis:03}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// One API response as Claude Code writes it: one line per content block, same message id and usage.
fn response(id: &str, ts: i64, model: &str, (input, output, cw, cr): (i64, i64, i64, i64), blocks: usize) -> String {
    let usage = format!(
        r#"{{"input_tokens":{input},"output_tokens":{output},"cache_creation_input_tokens":{cw},"cache_read_input_tokens":{cr},"server_tool_use":{{"web_search_requests":0}},"service_tier":"standard"}}"#
    );
    (0..blocks)
        .map(|b| {
            format!(
                r#"{{"type":"assistant","isSidechain":false,"sessionId":"{CSID}","version":"2.1.311","timestamp":"{}","requestId":"req_{id}","uuid":"u-{id}-{b}","message":{{"id":"msg_{id}","type":"message","role":"assistant","model":"{model}","content":[{{"type":"text","text":"SECRET-RESPONSE-TEXT"}}],"usage":{usage}}}}}"#,
                iso(ts)
            ) + "\n"
        })
        .collect()
}

fn user(ts: i64) -> String {
    format!(r#"{{"type":"user","sessionId":"{CSID}","timestamp":"{}","message":{{"role":"user","content":"SECRET-PROMPT-TEXT"}}}}"#, iso(ts)) + "\n"
}

fn synthetic(ts: i64) -> String {
    format!(
        r#"{{"type":"assistant","isApiErrorMessage":true,"timestamp":"{}","message":{{"id":"x-{ts}","model":"<synthetic>","usage":{{"input_tokens":0,"output_tokens":0}},"content":[{{"type":"text","text":"Not logged in"}}]}}}}"#,
        iso(ts)
    ) + "\n"
}

fn cost(total: f64, start: i64) -> String {
    format!(r#"{{"type":"cost-state","sessionId":"{CSID}","totalCostUSD":{total},"startTime":{start},"modelUsage":{{}},"hasUnknownModelCost":false}}"#) + "\n"
}

fn transcript(cfg: &Path, csid: &str) -> PathBuf {
    let dir = cfg.join("projects").join("D--work-repo");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{csid}.jsonl"))
}

fn append(path: &Path, s: &str) {
    use std::io::Write;
    std::fs::OpenOptions::new().create(true).append(true).open(path).unwrap().write_all(s.as_bytes()).unwrap();
}

fn session(f: &Fx, csid: &str, started_at: i64, integrations: Option<&str>) -> crate::session::Session {
    let s = crate::session::insert_starting(
        &f.conn,
        NewSession { title: "s".into(), cwd: PathBuf::from("/w"), program: "claude".into(), profile_id: Some(f.profile.clone()), claude_session_id: Some(csid.into()), work_profile: Some("balanced".into()), ..Default::default() },
    )
    .unwrap();
    f.conn.execute("UPDATE sessions SET started_at = ?2, integrations = ?3 WHERE id = ?1", params![s.id, started_at, integrations]).unwrap();
    crate::session::get(&f.conn, &s.id).unwrap()
}

fn ended(f: &Fx, s: &crate::session::Session, at: i64) -> crate::session::Session {
    crate::session::mark_exited(&f.conn, &s.id, Some(0)).unwrap();
    f.conn.execute("UPDATE sessions SET ended_at = ?2 WHERE id = ?1", params![s.id, at]).unwrap();
    crate::session::get(&f.conn, &s.id).unwrap()
}

fn status(f: &Fx, id: &str) -> (String, Option<String>) {
    f.conn.query_row("SELECT status, detail FROM usage_state WHERE session_id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap()
}

#[test]
fn transcripts_are_read_incrementally_deduplicated_and_never_store_text() {
    let f = fx();
    let s = session(&f, CSID, T0, None);
    let path = transcript(&f.cfg, CSID);
    let sub = f.cfg.join("projects").join("D--work-repo").join(CSID).join("subagents");
    std::fs::create_dir_all(&sub).unwrap();
    append(&path, &(user(T0 + 1_000) + &response("a", T0 + 2_000, "claude-opus-5-5", (10, 200, 3_000, 0), 3) + &synthetic(T0 + 2_500) + &cost(0.25, T0)));
    // A partial line at the end is left for the next scan.
    append(&path, r#"{"type":"assistant","timestamp":"#);
    append(&sub.join("agent-1.jsonl"), &response("sub", T0 + 3_000, "claude-haiku-5-5", (5, 50, 0, 100), 1));
    assert_eq!(scan_session(&f.conn, &f.home, &s).unwrap(), 4, "3 content-block lines of `a` and 1 subagent response read");
    let r = report::report(&f.conn, &Filter::default(), Dimension::Model).unwrap();
    assert_eq!((r.totals.requests, r.totals.input, r.totals.output, r.totals.cache_write, r.totals.cache_read), (2, 15, 250, 3_000, 100), "deduplicated by message id");
    assert_eq!(r.totals.subagent_requests, 1);
    assert_eq!(r.groups.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), ["claude-opus-5-5", "claude-haiku-5-5"]);
    assert_eq!(r.cost.usd, Some(0.25));
    assert_eq!(status(&f, &s.id).0, "measured", "measured so far while the session is still live");
    let stored: String = f.conn.query_row("SELECT group_concat(model || message_id) FROM usage_messages", [], |r| r.get(0)).unwrap();
    assert!(!stored.contains("SECRET"), "numbers and ids only");

    // Appending completes the partial line and adds a response; the cost snapshot grows cumulatively.
    append(&path, &(String::from(r#""bad-line", truncated"#) + "\n" + &response("b", T0 + 60_000, "claude-opus-5-5", (1, 20, 0, 3_010), 2) + &cost(0.40, T0)));
    let s = ended(&f, &s, T0 + 120_000);
    assert_eq!(scan_session(&f.conn, &f.home, &s).unwrap(), 2);
    let r = report::report(&f.conn, &Filter::default(), Dimension::Project).unwrap();
    assert_eq!((r.totals.requests, r.totals.cache_read), (3, 3_110));
    assert!((r.cost.usd.unwrap() - 0.40).abs() < 1e-9, "snapshots become deltas: 0.25 + 0.15");
    assert_eq!(status(&f, &s.id).0, "measured");
    let (skipped, errors, version): (i64, i64, String) =
        f.conn.query_row("SELECT skipped, local_errors, claude_version FROM usage_sources WHERE file LIKE '%' || ?1 || '.jsonl'", [CSID], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    assert_eq!((skipped, errors, version.as_str()), (1, 1, "2.1.311"), "the broken line is skipped; the synthetic message is a local error");
    // Nothing new: nothing is read; a rewritten (shorter) file is read again without double counting.
    assert_eq!(scan_session(&f.conn, &f.home, &s).unwrap(), 0);
    std::fs::write(&path, response("a", T0 + 2_000, "claude-opus-5-5", (10, 200, 3_000, 0), 1)).unwrap();
    scan_session(&f.conn, &f.home, &s).unwrap();
    assert_eq!(report::report(&f.conn, &Filter::default(), Dimension::Project).unwrap().totals.requests, 3);
}

#[test]
fn resumed_and_cleared_conversations_are_attributed_by_time() {
    let f = fx();
    let path = transcript(&f.cfg, CSID);
    // Before NEXUS: a response from an earlier, external session of the same conversation.
    append(&path, &response("ext", T0 - 3_600_000, "m", (1, 1, 0, 0), 1));
    let a = session(&f, CSID, T0, None);
    append(&path, &response("a1", T0 + 1_000, "m", (10, 10, 0, 0), 1));
    let a = ended(&f, &a, T0 + 10_000);
    scan_due(&f.conn, &f.home, 10).unwrap();
    // Resumed in NEXUS later: same conversation, a new NEXUS session.
    let b = session(&f, CSID, T0 + 600_000, None);
    append(&path, &response("b1", T0 + 601_000, "m", (20, 20, 0, 0), 1));
    // /clear inside b: Claude continues in a new conversation.
    let csid2 = "0b7c2a3e-1111-4222-8333-000000000002";
    crate::activity::handle(&f.conn, &b.id, crate::activity::HookEvent::SessionStart, &serde_json::json!({ "source": "clear", "session_id": csid2 })).unwrap();
    append(&transcript(&f.cfg, csid2), &response("c1", now_ms(), "m", (30, 30, 0, 0), 1));
    let b = ended(&f, &crate::session::get(&f.conn, &b.id).unwrap(), now_ms() + 1_000);
    scan_due(&f.conn, &f.home, 10).unwrap();

    let rows = report::sessions(&f.conn, &Filter::default(), 10).unwrap();
    let of = |id: &str| rows.iter().find(|r| r.session_id == id).unwrap().tokens.requests;
    assert_eq!(of(&a.id), 1, "a: only its own response");
    assert_eq!(of(&b.id), 2, "b: its response in the resumed conversation and the one after /clear");
    let r = report::report(&f.conn, &Filter::default(), Dimension::Profile).unwrap();
    assert_eq!((r.totals.requests, r.coverage.outside_nexus_requests), (3, 1), "the external response is kept apart");
    assert_eq!((r.coverage.sessions, r.coverage.measured), (2, 2));
}

#[test]
fn unavailable_usage_is_explained_not_reported_as_zero() {
    let f = fx();
    let live = session(&f, "0b7c2a3e-1111-4222-8333-000000000010", T0, None);
    let gone = ended(&f, &session(&f, "0b7c2a3e-1111-4222-8333-000000000011", T0, None), T0 + 5_000);
    let signed_out = ended(&f, &session(&f, "0b7c2a3e-1111-4222-8333-000000000012", T0, None), T0 + 5_000);
    append(&transcript(&f.cfg, "0b7c2a3e-1111-4222-8333-000000000012"), &(user(T0 + 1_000) + &synthetic(T0 + 1_500) + &cost(0.0, T0)));
    assert_eq!(scan_due(&f.conn, &f.home, 10).unwrap().sessions, 3);
    assert_eq!(status(&f, &live.id).0, "pending");
    let (st, detail) = status(&f, &gone.id);
    assert!(st == "missing" && detail.unwrap().contains("no transcript"));
    let (st, detail) = status(&f, &signed_out.id);
    assert!(st == "no-requests" && detail.unwrap().contains("1 local error message"), "signed out: no API request");
    let r = report::report(&f.conn, &Filter::default(), Dimension::Project).unwrap();
    assert_eq!((r.totals.requests, r.cost.usd), (0, None), "no tokens and no cost are invented");
    assert_eq!((r.coverage.sessions, r.coverage.pending, r.coverage.missing, r.coverage.no_requests), (3, 1, 1, 1));
    // Due again: only the live one (ended sessions were scanned after they ended).
    assert_eq!(scan_due(&f.conn, &f.home, 10).unwrap().sessions, 1);
    // A removed profile: unreadable, with the reason.
    f.conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
    f.conn.execute("UPDATE sessions SET profile_id = 'gone' WHERE id = ?1", [&gone.id]).unwrap();
    scan_session(&f.conn, &f.home, &crate::session::get(&f.conn, &gone.id).unwrap()).unwrap();
    assert_eq!(status(&f, &gone.id).0, "unreadable");
}

#[test]
fn reports_group_filter_and_compare_efficiency_with_estimates_apart() {
    let f = fx();
    let lean = r#"{"mcp":{"state":"enabled","config":"/x"},"hooks":{"state":"enabled","config":"/y"},"context":{"strategy":"minimal","observeInstructions":false},"promptChars":400}"#;
    let deep = r#"{"mcp":{"state":"enabled","config":"/x"},"hooks":{"state":"enabled","config":"/y"},"context":{"strategy":"deep","observeInstructions":false},"promptChars":4000,"workflow":{"runId":"run-1","workflow":"Feature","rigor":"balanced","skills":[],"refused":[]}}"#;
    let ids = ["0b7c2a3e-1111-4222-8333-000000000020", "0b7c2a3e-1111-4222-8333-000000000021"];
    let s1 = session(&f, ids[0], T0, Some(lean));
    let s2 = session(&f, ids[1], T0 + 86_400_000, Some(deep));
    f.conn.execute("UPDATE sessions SET session_start_chars = 800 WHERE id = ?1", [&s2.id]).unwrap();
    append(&transcript(&f.cfg, ids[0]), &(response("l1", T0 + 1_000, "m", (100, 50, 1_000, 3_000), 1) + &response("l2", T0 + 2_000, "m", (100, 50, 0, 4_000), 1)));
    append(&transcript(&f.cfg, ids[1]), &(response("d1", T0 + 86_401_000, "m", (500, 100, 9_000, 0), 1) + &cost(1.5, T0 + 86_400_000)));
    for s in [&s1, &s2] {
        let s = ended(&f, s, T0 + 2 * 86_400_000);
        scan_session(&f.conn, &f.home, &s).unwrap();
    }
    let r = report::report(&f.conn, &Filter::default(), Dimension::Strategy).unwrap();
    assert_eq!(r.daily.len(), 2, "one bucket per local day");
    let (minimal, deep_g) = (r.groups.iter().find(|g| g.key == "minimal").unwrap(), r.groups.iter().find(|g| g.key == "deep").unwrap());
    assert_eq!((minimal.label.as_str(), minimal.tokens.requests, minimal.sessions), ("Minimal", 2, 1));
    assert_eq!(minimal.efficiency.context_per_request, Some(4_100.0), "(200 + 1000 + 7000) / 2");
    assert!((minimal.efficiency.cache_hit.unwrap() - 7_000.0 / 8_200.0).abs() < 1e-9);
    assert_eq!(minimal.cost.usd, None, "no cost snapshot → unavailable, not zero");
    assert_eq!(deep_g.cost.usd, Some(1.5));
    let bal = crate::workprofile::get("balanced").unwrap().prompt.chars().count() as f64;
    assert_eq!(deep_g.context_estimate.tokens_per_session, Some(((4_000.0 + bal + 800.0) / 4.0).ceil()));
    assert!(deep_g.context_estimate.share_of_context.unwrap() > minimal.context_estimate.share_of_context.unwrap() * 0.5);
    // Filters: workflow run, strategy, time; an unknown model filter hides costs (they are per session).
    let only = |flt: Filter| report::report(&f.conn, &flt, Dimension::Workflow).unwrap();
    assert_eq!(only(Filter { workflow_run_id: Some("run-1".into()), ..Default::default() }).totals.requests, 1);
    assert_eq!(only(Filter { strategy: Some("minimal".into()), ..Default::default() }).totals.requests, 2);
    assert_eq!(only(Filter { from: Some(T0 + 86_400_000), ..Default::default() }).totals.requests, 1);
    let by_model = only(Filter { model: Some("m".into()), ..Default::default() });
    assert_eq!((by_model.totals.requests, by_model.cost.usd), (3, None));
    let wf = only(Filter::default());
    assert_eq!(wf.groups.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(), ["run-1 (removed)", "No workflow"], "labels, also for removed runs");
    let rows = report::sessions(&f.conn, &Filter::default(), 10).unwrap();
    assert_eq!(rows.iter().map(|r| r.strategy.as_str()).collect::<Vec<_>>(), ["deep", "minimal"]);
    assert_eq!(rows[0].cost.usd, Some(1.5));
}

#[test]
fn subscription_usage_is_never_converted_into_spend() {
    use crate::claude::auth::AuthStatus;
    let sub = AuthStatus { logged_in: true, auth_method: Some("claude.ai".into()), subscription_type: Some("max".into()), ..Default::default() };
    assert_eq!(billing_of(&sub), Billing::Subscription);
    assert_eq!(billing_of(&api_key()), Billing::Api);
    assert_eq!(billing_of(&AuthStatus { logged_in: true, api_provider: Some("bedrock".into()), ..Default::default() }), Billing::Api);
    assert_eq!(billing_of(&AuthStatus { logged_in: true, auth_method: Some("fake".into()), ..Default::default() }), Billing::Unknown);
    assert_eq!(billing_of(&AuthStatus::default()), Billing::Unknown, "signed out");

    let f = fx();
    let s = session(&f, CSID, T0, None);
    append(&transcript(&f.cfg, CSID), &(response("a", T0 + 1_000, "m", (1, 1, 0, 0), 1) + &cost(2.5, T0)));
    let s = ended(&f, &s, T0 + 5_000);
    scan_session(&f.conn, &f.home, &s).unwrap();
    assert_eq!(report::report(&f.conn, &Filter::default(), Dimension::Project).unwrap().cost.usd, Some(2.5), "API-billed: Claude Code's estimate");
    record_billing(&f.conn, &f.profile, &sub).unwrap();
    let r = report::report(&f.conn, &Filter::default(), Dimension::Project).unwrap();
    assert_eq!((r.cost.usd, r.cost.withheld_subscription, r.totals.requests), (None, 1, 1), "tokens stay, spend is not shown");
    assert_eq!(report::sessions(&f.conn, &Filter::default(), 5).unwrap()[0].cost.withheld_subscription, 1);
    f.conn.execute("DELETE FROM profile_billing", []).unwrap();
    let r = report::report(&f.conn, &Filter::default(), Dimension::Profile).unwrap();
    assert_eq!((r.cost.usd, r.cost.withheld_unknown), (None, 1), "unknown billing is withheld too");
    assert_eq!((r.token_provenance, r.format_verified, r.claude_versions.clone()), ("parsed", true, vec!["2.1.311".to_string()]));
}
