//! Reading Claude Code's session transcripts for usage (ADR-0030) — read-only, incremental, numbers only.
//!
//! A conversation `<id>` of a profile lives in `<config dir>/projects/<encoded cwd>/<id>.jsonl`; subagents write
//! `<…>/<id>/subagents/*.jsonl`. Lines are JSON records. Two kinds matter:
//!
//! * `type: assistant` with `message.usage` — one API response, as reported by the Claude API (input, output,
//!   cache-creation and cache-read tokens). Claude Code may write one line per content block of the same response
//!   (same `message.id`), so responses are deduplicated by id. Messages Claude Code generates locally (model
//!   `<synthetic>`, `isApiErrorMessage`, e.g. "Not logged in") made no API request and are only counted as errors.
//! * `type: cost-state` — Claude Code's running cost snapshot (`totalCostUSD`, process `startTime`): an estimate
//!   from its own price table, never a bill.
//!
//! Everything else (prompts, responses, tool input and output) is skipped by the parser and never stored.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Claude Code versions (major.minor) whose transcript usage records NEXUS has verified against the real CLI — the
/// compatibility layer of ADR-0010. Other versions are still read (the fields are stable so far) but reported as
/// unverified.
pub const VERIFIED_FORMATS: &[&str] = &["2.1"];

pub fn format_verified(version: &str) -> bool {
    let mut parts = version.split(['.', ' ']);
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => VERIFIED_FORMATS.contains(&format!("{major}.{minor}").as_str()),
        _ => false,
    }
}

/// Longest line considered (larger ones are skipped as `oversized`).
pub const MAX_LINE: usize = 8 << 20;
/// Subagent files considered per conversation.
pub const MAX_SUBAGENT_FILES: usize = 200;

#[derive(Debug, Clone, PartialEq)]
pub struct Located {
    /// Relative to the config dir, with forward slashes (stable key across machines and separators).
    pub rel: String,
    pub path: PathBuf,
    pub subagent: bool,
}

/// The transcript files of conversation `claude_session_id` under `config_dir` — main file first. Only regular files
/// inside `projects/` are returned; the id must be a UUID (it becomes part of a path).
pub fn locate(config_dir: &Path, claude_session_id: &str) -> Vec<Located> {
    if uuid::Uuid::parse_str(claude_session_id).is_err() {
        return Vec::new();
    }
    let projects = config_dir.join("projects");
    let Ok(rd) = std::fs::read_dir(&projects) else {
        return Vec::new();
    };
    let regular = |p: &Path| std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_file());
    let mut main = Vec::new();
    let mut subs = Vec::new();
    let mut dirs: Vec<_> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).collect();
    dirs.sort_by_key(|e| e.file_name());
    for d in dirs {
        let dir_name = d.file_name().to_string_lossy().into_owned();
        let file = d.path().join(format!("{claude_session_id}.jsonl"));
        if regular(&file) {
            main.push(Located { rel: format!("projects/{dir_name}/{claude_session_id}.jsonl"), path: file, subagent: false });
        }
        let sub_dir = d.path().join(claude_session_id).join("subagents");
        if let Ok(rd) = std::fs::read_dir(&sub_dir) {
            let mut files: Vec<_> = rd
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().ends_with(".jsonl") && regular(&e.path()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            files.sort();
            for f in files.into_iter().take(MAX_SUBAGENT_FILES.saturating_sub(subs.len())) {
                subs.push(Located { rel: format!("projects/{dir_name}/{claude_session_id}/subagents/{f}"), path: sub_dir.join(&f), subagent: true });
            }
        }
    }
    main.extend(subs);
    main
}

/// One API response (tokens as reported by the Claude API).
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub ts: i64,
    pub model: String,
    pub input: i64,
    pub output: i64,
    pub cache_write: i64,
    pub cache_read: i64,
    pub web_searches: i64,
    pub subagent: bool,
}

/// A Claude Code cost-state snapshot (cumulative for its process).
#[derive(Debug, Clone, PartialEq)]
pub struct Cost {
    pub total_usd: f64,
    pub start_time: Option<i64>,
    pub unknown_model: bool,
    /// Time of the last API response before this snapshot, if any.
    pub after_ts: Option<i64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Chunk {
    pub messages: Vec<Message>,
    pub costs: Vec<Cost>,
    /// Bytes consumed (up to and including the last complete line); the next read starts here.
    pub consumed: u64,
    pub lines: i64,
    pub skipped: i64,
    pub local_errors: i64,
    pub version: Option<String>,
}

#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "isSidechain")]
    sidechain: Option<bool>,
    #[serde(rename = "isApiErrorMessage")]
    api_error: Option<bool>,
    uuid: Option<String>,
    version: Option<String>,
    message: Option<RawMessage>,
    #[serde(rename = "totalCostUSD")]
    total_cost: Option<f64>,
    #[serde(rename = "startTime")]
    start_time: Option<i64>,
    #[serde(rename = "hasUnknownModelCost")]
    unknown_model: Option<bool>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize, Default)]
struct RawUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    server_tool_use: Option<ServerTools>,
}

#[derive(Deserialize, Default)]
struct ServerTools {
    web_search_requests: Option<u64>,
}

fn n(v: Option<u64>) -> i64 {
    i64::try_from(v.unwrap_or(0)).unwrap_or(i64::MAX)
}

/// Printable, bounded identifier (model names and ids come from a file; they are shown and used as keys).
fn ident(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_graphic()).take(120).collect()
}

/// Read complete lines from `offset` (at most `max_bytes`) and extract usage records.
pub fn read_from(path: &Path, offset: u64, max_bytes: u64, subagent: bool) -> std::io::Result<Chunk> {
    let mut f = File::open(path)?;
    f.seek(SeekFrom::Start(offset))?;
    let mut buf = Vec::new();
    f.take(max_bytes).read_to_end(&mut buf)?;
    let mut chunk = Chunk::default();
    let end = match buf.iter().rposition(|b| *b == b'\n') {
        Some(i) => i + 1,
        // No complete line: a line longer than the budget is skipped as a whole; otherwise wait for more.
        None if buf.len() as u64 >= max_bytes && !buf.is_empty() => {
            chunk.skipped = 1;
            chunk.consumed = buf.len() as u64;
            return Ok(chunk);
        }
        None => return Ok(chunk),
    };
    chunk.consumed = end as u64;
    let mut last_ts: Option<i64> = None;
    for line in buf[..end].split(|b| *b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        chunk.lines += 1;
        if line.len() > MAX_LINE {
            chunk.skipped += 1;
            continue;
        }
        // Cheap prefilter: only assistant responses and cost snapshots are parsed.
        let interesting = contains(line, b"\"assistant\"") || contains(line, b"\"cost-state\"");
        if !interesting {
            continue;
        }
        let Ok(raw) = serde_json::from_slice::<RawLine>(line) else {
            chunk.skipped += 1;
            continue;
        };
        if let Some(v) = raw.version.as_deref() {
            chunk.version = Some(ident(v));
        }
        match raw.kind.as_deref() {
            Some("assistant") => {
                let Some(m) = raw.message else { continue };
                let model = m.model.as_deref().map(ident).unwrap_or_default();
                if model == "<synthetic>" || model.is_empty() || raw.api_error == Some(true) {
                    chunk.local_errors += 1;
                    continue;
                }
                let Some(u) = m.usage else { continue };
                let Some(ts) = raw.timestamp.as_deref().and_then(parse_ts) else {
                    chunk.skipped += 1;
                    continue;
                };
                let Some(id) = m.id.or(raw.uuid).map(|i| ident(&i)).filter(|i| !i.is_empty()) else {
                    chunk.skipped += 1;
                    continue;
                };
                last_ts = Some(ts);
                chunk.messages.push(Message {
                    id,
                    ts,
                    model,
                    input: n(u.input_tokens),
                    output: n(u.output_tokens),
                    cache_write: n(u.cache_creation_input_tokens),
                    cache_read: n(u.cache_read_input_tokens),
                    web_searches: n(u.server_tool_use.unwrap_or_default().web_search_requests),
                    subagent: subagent || raw.sidechain == Some(true),
                });
            }
            Some("cost-state") => {
                if let Some(total) = raw.total_cost.filter(|c| c.is_finite() && *c >= 0.0) {
                    chunk.costs.push(Cost { total_usd: total, start_time: raw.start_time, unknown_model: raw.unknown_model == Some(true), after_ts: last_ts });
                }
            }
            _ => {}
        }
    }
    Ok(chunk)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// RFC 3339 timestamp (`2026-11-04T23:34:19.016Z`, offsets allowed) → milliseconds since the epoch.
pub fn parse_ts(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (num(0..4)?, num(5..7)?, num(8..10)?, num(11..13)?, num(14..16)?, num(17..19)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let mut i = 19;
    let mut ms = 0i64;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let frac = s.get(start..i)?;
        let digits: String = frac.chars().chain(std::iter::repeat('0')).take(3).collect();
        ms = digits.parse().ok()?;
    }
    let offset_min = match b.get(i)? {
        b'Z' | b'z' => 0,
        sign @ (b'+' | b'-') => {
            let oh = num(i + 1..i + 3)?;
            let om = num(i + 4..i + 6)?;
            let o = oh * 60 + om;
            if *sign == b'+' { o } else { -o }
        }
        _ => return None,
    };
    // Days from civil (Howard Hinnant's algorithm).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 86_400 + h * 3600 + mi * 60 + se - offset_min * 60) * 1000) + ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_versions() {
        assert!(format_verified("2.1.311") && format_verified("2.1.0"));
        assert!(!format_verified("2.2.0") && !format_verified("1.0.88") && !format_verified("garbage"));
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2026-11-04T23:34:19.016Z"), Some(1_793_835_259_016));
        assert_eq!(parse_ts("2026-11-05T00:34:19.016+01:00"), Some(1_793_835_259_016));
        assert_eq!(parse_ts("2000-02-29T12:00:00.5Z"), Some(951_825_600_500));
        assert_eq!(parse_ts("garbage"), None);
        assert_eq!(parse_ts("2026-13-01T00:00:00Z"), None);
    }
}
