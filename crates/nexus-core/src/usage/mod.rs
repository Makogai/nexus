//! Local token usage analytics (ADR-0030).
//!
//! Source: the session transcripts Claude Code writes into each profile's config dir — read-only, incrementally,
//! numbers only. Every figure is one of:
//!
//! * **parsed** (ADR-0010) — token counts per API response as reported by the Claude API and recorded by Claude Code
//!   (input, output, cache writes, cache reads), deduplicated by message id. They are measured by the API but read
//!   from an undocumented local format, so the transcript's Claude Code version is checked against the formats NEXUS
//!   has verified ([`transcript::format_verified`]) and the values are labelled accordingly;
//! * **estimated** — Claude Code's own cost estimate (its `cost-state`, API list prices, never a bill), shown only for
//!   profiles billed per API use — subscription usage is never converted into spend — and NEXUS's estimate of the
//!   context it added itself (characters ÷ 4);
//! * **unavailable** — with a reason: no transcript, unreadable, not yet written, or not attributable.
//!
//! Nothing here is *measured* in ADR-0010's sense (a documented interface): OpenTelemetry to a local receiver for
//! opt-in personal profiles is the planned upgrade (ADR-0030).
//!
//! NEXUS never changes telemetry: it neither sets nor forwards `OTEL_*` / `CLAUDE_CODE_ENABLE_TELEMETRY` (they are
//! removed from child environments, ADR-0003) and organisation-managed telemetry is only reported, not touched.
//!
//! Attribution: a conversation's responses belong to the NEXUS session that joined that conversation most recently
//! before the response (`session_conversations`; a resumed session continues the same conversation, `/clear`
//! starts a new one). Responses from before any NEXUS session are kept apart as "outside NEXUS".

pub mod report;
pub mod transcript;

#[cfg(test)]
mod tests;

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use crate::session::{self, Kind, Session};
use crate::{Result, now_ms};

/// Bytes read per transcript file and scan (a large transcript is caught up over several scans, so one scan never
/// holds the database for long).
pub const SCAN_BYTES: u64 = 8 << 20;
/// Responses up to this long before a session joined a conversation still count as that session's (clocks,
/// process start).
const JOIN_TOLERANCE_MS: i64 = 120_000;

/// What a scan did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Scanned {
    pub sessions: usize,
    pub messages: usize,
    /// Sessions that are not live any more (their final scan, or catching up).
    pub finished: usize,
}

#[derive(Debug, Default)]
struct Source {
    read_offset: u64,
    last_cost: Option<f64>,
    last_start: Option<i64>,
    cost_seq: i64,
}

/// The sessions linked to a conversation with the time each joined it, oldest first.
fn joined(conn: &Connection, claude_session_id: &str) -> Result<Vec<(String, i64)>> {
    Ok(conn
        .prepare("SELECT session_id, since FROM session_conversations WHERE claude_session_id = ?1 ORDER BY since, session_id")?
        .query_map([claude_session_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?)
}

fn attribute(joined: &[(String, i64)], ts: i64) -> Option<String> {
    joined.iter().rev().find(|(_, since)| *since <= ts + JOIN_TOLERANCE_MS).map(|(id, _)| id.clone())
}

/// Record that a session works in its current conversation (sessions created by any host).
fn link_current(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO session_conversations (session_id, claude_session_id, since)
         SELECT id, claude_session_id, started_at FROM sessions WHERE kind = 'claude' AND claude_session_id IS NOT NULL",
        [],
    )?;
    Ok(())
}

/// What one conversation scan found.
#[derive(Default)]
struct ConversationScan {
    files: usize,
    read: usize,
    /// Unread bytes remain in some file.
    behind: bool,
}

/// Read what is new in one conversation's transcript files.
fn scan_conversation(conn: &Connection, profile_id: &str, config_dir: &Path, claude_session_id: &str) -> Result<ConversationScan> {
    let files = transcript::locate(config_dir, claude_session_id);
    let joined = joined(conn, claude_session_id)?;
    let mut read = 0;
    let mut behind = false;
    for f in &files {
        let src: Source = conn
            .query_row(
                "SELECT read_offset, last_cost, last_start, cost_seq FROM usage_sources WHERE profile_id = ?1 AND claude_session_id = ?2 AND file = ?3",
                params![profile_id, claude_session_id, f.rel],
                |r| Ok(Source { read_offset: r.get::<_, i64>(0)?.max(0) as u64, last_cost: r.get(1)?, last_start: r.get(2)?, cost_seq: r.get(3)? }),
            )
            .optional()?
            .unwrap_or_default();
        let len = std::fs::metadata(&f.path).map(|m| m.len()).unwrap_or(0);
        // A shorter file was rewritten: read it again (responses are deduplicated by id).
        let offset = if len < src.read_offset { 0 } else { src.read_offset };
        if len == offset && len > 0 {
            continue;
        }
        let chunk = transcript::read_from(&f.path, offset, SCAN_BYTES, f.subagent).map_err(|source| crate::CoreError::Io { action: "read", path: f.path.clone(), source })?;
        behind |= offset + chunk.consumed < len && chunk.consumed > 0;
        let tx = conn.unchecked_transaction()?;
        for m in &chunk.messages {
            tx.execute(
                "INSERT INTO usage_messages (profile_id, claude_session_id, message_id, session_id, ts, model, input_tokens, output_tokens,
                     cache_write_tokens, cache_read_tokens, web_searches, subagent)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (profile_id, claude_session_id, message_id) DO UPDATE SET
                     input_tokens = MAX(input_tokens, excluded.input_tokens),
                     output_tokens = MAX(output_tokens, excluded.output_tokens),
                     cache_write_tokens = MAX(cache_write_tokens, excluded.cache_write_tokens),
                     cache_read_tokens = MAX(cache_read_tokens, excluded.cache_read_tokens),
                     web_searches = MAX(web_searches, excluded.web_searches),
                     ts = MIN(ts, excluded.ts),
                     session_id = COALESCE(session_id, excluded.session_id)",
                params![
                    profile_id,
                    claude_session_id,
                    m.id,
                    attribute(&joined, m.ts),
                    m.ts,
                    m.model,
                    m.input,
                    m.output,
                    m.cache_write,
                    m.cache_read,
                    m.web_searches,
                    m.subagent as i64
                ],
            )?;
        }
        // Cost snapshots are cumulative per Claude process; a lower total starts a new series.
        let (mut last_cost, mut last_start, mut seq) = (src.last_cost, src.last_start, src.cost_seq);
        for c in &chunk.costs {
            let delta = match last_cost {
                Some(prev) if c.total_usd >= prev => c.total_usd - prev,
                _ => c.total_usd,
            };
            last_cost = Some(c.total_usd);
            last_start = c.start_time.or(last_start);
            if delta <= 0.0 && !c.unknown_model {
                continue;
            }
            seq += 1;
            let at = c.start_time.or(c.after_ts).unwrap_or_else(now_ms);
            tx.execute(
                "INSERT OR REPLACE INTO usage_costs (profile_id, claude_session_id, file, seq, session_id, ts, cost_usd, unknown_model)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![profile_id, claude_session_id, f.rel, seq, attribute(&joined, at), c.after_ts.unwrap_or(at), delta.max(0.0), c.unknown_model as i64],
            )?;
        }
        tx.execute(
            "INSERT INTO usage_sources (profile_id, claude_session_id, file, read_offset, lines, skipped, local_errors, claude_version, last_cost, last_start, cost_seq, scanned_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT (profile_id, claude_session_id, file) DO UPDATE SET
                 read_offset = excluded.read_offset, lines = lines + excluded.lines, skipped = skipped + excluded.skipped,
                 local_errors = local_errors + excluded.local_errors, claude_version = COALESCE(excluded.claude_version, claude_version),
                 last_cost = excluded.last_cost, last_start = excluded.last_start, cost_seq = excluded.cost_seq, scanned_at = excluded.scanned_at",
            params![
                profile_id,
                claude_session_id,
                f.rel,
                (offset + chunk.consumed) as i64,
                chunk.lines,
                chunk.skipped,
                chunk.local_errors,
                chunk.version,
                last_cost,
                last_start,
                seq,
                now_ms()
            ],
        )?;
        tx.commit()?;
        read += chunk.messages.len();
    }
    Ok(ConversationScan { files: files.len(), read, behind })
}

fn set_state(conn: &Connection, session_id: &str, status: &str, detail: Option<&str>, files: usize, behind: bool) -> Result<()> {
    conn.execute(
        "INSERT INTO usage_state (session_id, status, detail, files, behind, scanned_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (session_id) DO UPDATE SET status = excluded.status, detail = excluded.detail, files = excluded.files,
             behind = excluded.behind, scanned_at = excluded.scanned_at",
        params![session_id, status, detail, files as i64, behind as i64, now_ms()],
    )?;
    Ok(())
}

/// Scan every conversation of a Claude session (and refresh the coverage state of all sessions sharing them).
pub fn scan_session(conn: &Connection, home: &Path, s: &Session) -> Result<usize> {
    if s.kind != Kind::Claude {
        return Ok(0);
    }
    link_current(conn)?;
    let Some(profile) = s.profile_id.as_deref().and_then(|id| crate::claude::profile::get(conn, id).ok()) else {
        set_state(conn, &s.id, "unreadable", Some("the session's Claude profile no longer exists in NEXUS"), 0, false)?;
        return Ok(0);
    };
    let config_dir = profile.effective_config_dir(home);
    let conversations: Vec<String> = conn
        .prepare("SELECT claude_session_id FROM session_conversations WHERE session_id = ?1 ORDER BY since")?
        .query_map([&s.id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut files = 0;
    let mut read = 0;
    let mut behind = false;
    let mut sharing = std::collections::BTreeSet::new();
    for c in &conversations {
        match scan_conversation(conn, &profile.id, &config_dir, c) {
            Ok(scan) => {
                files += scan.files;
                read += scan.read;
                behind |= scan.behind;
            }
            Err(e) => {
                // The reason, never the path or content.
                let kind = match &e {
                    crate::CoreError::Io { source, .. } => format!("{:?}", source.kind()),
                    _ => "error".into(),
                };
                set_state(conn, &s.id, "unreadable", Some(&format!("the transcript could not be read ({kind})")), files, false)?;
                return Ok(read);
            }
        }
        for (id, _) in joined(conn, c)? {
            sharing.insert(id);
        }
    }
    sharing.insert(s.id.clone());
    for id in sharing {
        let other = if id == s.id { s.clone() } else { session::get(conn, &id)? };
        refresh_state(conn, &other, &conversations, files, behind)?;
    }
    Ok(read)
}

fn refresh_state(conn: &Connection, s: &Session, conversations: &[String], files: usize, behind: bool) -> Result<()> {
    let responses: i64 = conn.query_row("SELECT COUNT(*) FROM usage_messages WHERE session_id = ?1", [&s.id], |r| r.get(0))?;
    let live = s.state.is_live();
    let local_errors: i64 = if conversations.is_empty() {
        0
    } else {
        let marks = vec!["?"; conversations.len()].join(",");
        conn.query_row(
            &format!("SELECT COALESCE(SUM(local_errors), 0) FROM usage_sources WHERE claude_session_id IN ({marks})"),
            rusqlite::params_from_iter(conversations.iter()),
            |r| r.get(0),
        )?
    };
    let (status, detail): (&str, Option<String>) = if responses > 0 {
        ("measured", None)
    } else if files == 0 && live {
        ("pending", Some("Claude has not written a transcript for this session yet".into()))
    } else if files == 0 {
        ("missing", Some("no transcript in the profile's config dir — deleted, removed by Claude's cleanup, or Claude never started".into()))
    } else if live {
        ("pending", Some("no API response recorded yet".into()))
    } else if local_errors > 0 {
        ("no-requests", Some(format!("no API request was made — {local_errors} local error message(s), e.g. not signed in")))
    } else {
        ("no-requests", Some("the transcript records no API response".into()))
    };
    set_state(conn, &s.id, status, detail.as_deref(), files, behind)
}

/// Scan the sessions that need it: live ones, ended ones not scanned since they ended, never-scanned ones.
/// Bounded per call (`limit` sessions); the desktop runner and the CLI call it.
pub fn scan_due(conn: &Connection, home: &Path, limit: usize) -> Result<Scanned> {
    link_current(conn)?;
    let ids: Vec<String> = conn
        .prepare(
            "SELECT s.id FROM sessions s LEFT JOIN usage_state u ON u.session_id = s.id
             WHERE s.kind = 'claude' AND s.claude_session_id IS NOT NULL AND s.profile_id IS NOT NULL
               AND (u.session_id IS NULL OR s.state IN ('starting', 'running') OR u.behind = 1 OR u.scanned_at < COALESCE(s.ended_at, 0))
             ORDER BY (s.state IN ('starting', 'running')) DESC, s.started_at DESC LIMIT ?1",
        )?
        .query_map([limit as i64], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut out = Scanned::default();
    for id in ids {
        let s = session::get(conn, &id)?;
        out.messages += scan_session(conn, home, &s)?;
        out.sessions += 1;
        out.finished += usize::from(!s.state.is_live());
    }
    Ok(out)
}

/// How a profile is billed, from `claude auth status` (no identity is kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Billing {
    /// A Claude subscription (Pro, Max, Team, Enterprise): usage is never shown as spend.
    Subscription,
    /// Billed per API use (API key, Console, cloud providers).
    Api,
    Unknown,
}

impl Billing {
    pub fn as_str(self) -> &'static str {
        match self {
            Billing::Subscription => "subscription",
            Billing::Api => "api",
            Billing::Unknown => "unknown",
        }
    }
}

pub fn billing_of(a: &crate::claude::auth::AuthStatus) -> Billing {
    if !a.logged_in {
        return Billing::Unknown;
    }
    let method = a.auth_method.as_deref().unwrap_or("").to_ascii_lowercase();
    let provider = a.api_provider.as_deref().unwrap_or("").to_ascii_lowercase();
    let subscription = a.subscription_type.as_deref().map(str::trim).is_some_and(|t| !t.is_empty() && !t.eq_ignore_ascii_case("none"));
    if method == "claude.ai" || subscription {
        Billing::Subscription
    } else if (method.contains("api") && method.contains("key")) || method == "console" || matches!(provider.as_str(), "bedrock" | "vertex" | "foundry") {
        Billing::Api
    } else {
        Billing::Unknown
    }
}

/// Remember how a profile is billed (after an auth status check).
pub fn record_billing(conn: &Connection, profile_id: &str, a: &crate::claude::auth::AuthStatus) -> Result<Billing> {
    let b = billing_of(a);
    conn.execute(
        "INSERT INTO profile_billing (profile_id, kind, checked_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (profile_id) DO UPDATE SET kind = excluded.kind, checked_at = excluded.checked_at",
        params![profile_id, b.as_str(), now_ms()],
    )?;
    Ok(b)
}
