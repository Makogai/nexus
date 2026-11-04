//! Output helpers: aligned tables for humans, pretty JSON for scripts (`--json`).

use serde::Serialize;

pub fn json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("error: cannot serialize output: {e}"),
    }
}

/// Print rows as aligned columns. The last column is never padded.
pub fn table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if let Some(w) = widths.get_mut(i) {
                *w = (*w).max(cell.chars().count());
            }
        }
    }
    let line = |cells: Vec<&str>| {
        let last = cells.len().saturating_sub(1);
        let mut out = String::new();
        for (i, c) in cells.iter().enumerate() {
            out.push_str(c);
            if i < last {
                out.push_str(&" ".repeat(widths[i] - c.chars().count() + 2));
            }
        }
        println!("{}", out.trim_end());
    };
    line(headers.to_vec());
    for row in rows {
        line(row.iter().map(String::as_str).collect());
    }
}

pub fn ago(ms: Option<i64>) -> String {
    let Some(ms) = ms else { return "never".into() };
    let s = (nexus_core::now_ms() - ms).max(0) / 1000;
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} days ago", s / 86_400),
    }
}

pub fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
