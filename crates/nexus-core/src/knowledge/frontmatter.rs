//! Strict YAML-subset frontmatter (ADR-0013, ADR-0017).
//!
//! Accepted between `---` fences at the top of a Markdown file:
//! * `key: value` — plain or quoted (`"…"` with `\"`, `\\`, `\n` escapes, or `'…'` with `''`) scalars;
//!   plain `true`/`false` are booleans, plain integers are numbers, `null`/`~`/empty are null;
//! * `key: [a, "b c"]` — flow lists of scalars;
//! * block lists (`key:` followed by `  - item` lines);
//! * `#` comment lines and blank lines.
//!
//! Everything else (anchors, aliases, tags, nested maps, multi-line blocks, duplicate keys) is rejected with a line
//! number, so a file is reported invalid instead of being misread. Entries keep their order; unknown keys survive a
//! rewrite unchanged in meaning.

use std::fmt::Write as _;

pub const MAX_FRONTMATTER_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    List(Vec<String>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_list(&self) -> Option<&[String]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Frontmatter {
    pub entries: Vec<(String, Value)>,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }
    pub fn list(&self, key: &str) -> Vec<String> {
        match self.get(key) {
            Some(Value::List(l)) => l.clone(),
            Some(Value::Str(s)) => vec![s.clone()],
            _ => Vec::new(),
        }
    }
    /// Set a key, keeping its position if it exists, appending otherwise.
    pub fn set(&mut self, key: &str, value: Value) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some(e) => e.1 = value,
            None => self.entries.push((key.to_string(), value)),
        }
    }
    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(k, _)| k != key);
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("frontmatter line {line}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

fn err(line: usize, message: impl Into<String>) -> ParseError {
    ParseError { line, message: message.into() }
}

/// Split a document into frontmatter and body. A document without a leading `---` fence has no frontmatter.
pub fn split(text: &str) -> Result<(Option<Frontmatter>, &str), ParseError> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n")) else {
        return Ok((None, text));
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" {
            let fm_text = &rest[..offset];
            if fm_text.len() > MAX_FRONTMATTER_BYTES {
                return Err(err(1, "frontmatter is too large"));
            }
            let body = &rest[offset + line.len()..];
            return Ok((Some(parse(fm_text)?), body));
        }
        offset += line.len();
    }
    Err(err(1, "missing closing `---`"))
}

pub fn parse(text: &str) -> Result<Frontmatter, ParseError> {
    let mut fm = Frontmatter::default();
    let mut pending_list: Option<(String, Vec<String>, usize)> = None;
    for (i, raw) in text.lines().enumerate() {
        let n = i + 2; // line 1 is the opening fence
        let line = raw.trim_end();
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(item) = line.strip_prefix("  - ").or_else(|| line.strip_prefix("- ")).or_else(|| line.strip_prefix("\t- ")) {
            let Some((_, items, _)) = pending_list.as_mut() else {
                return Err(err(n, "list item without a key"));
            };
            items.push(scalar_string(item.trim(), n)?);
            continue;
        }
        if let Some((key, items, at)) = pending_list.take() {
            // `key:` with no items is null; with items it is a list.
            let v = if items.is_empty() { Value::Null } else { Value::List(items) };
            push(&mut fm, key, v, at)?;
        }
        if line.starts_with([' ', '\t']) {
            return Err(err(n, "nested values are not supported"));
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(err(n, "expected `key: value`"));
        };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(err(n, format!("invalid key `{key}`")));
        }
        let value = value.trim();
        if value.is_empty() {
            pending_list = Some((key.to_string(), Vec::new(), n));
            continue;
        }
        push(&mut fm, key.to_string(), parse_value(value, n)?, n)?;
    }
    if let Some((key, items, at)) = pending_list {
        // `key:` with no items is null; with items it is a list.
        let v = if items.is_empty() { Value::Null } else { Value::List(items) };
        push(&mut fm, key, v, at)?;
    }
    Ok(fm)
}

fn push(fm: &mut Frontmatter, key: String, value: Value, line: usize) -> Result<(), ParseError> {
    if fm.get(&key).is_some() {
        return Err(err(line, format!("duplicate key `{key}`")));
    }
    fm.entries.push((key, value));
    Ok(())
}

fn parse_value(v: &str, n: usize) -> Result<Value, ParseError> {
    if let Some(inner) = v.strip_prefix('[') {
        let inner = inner.strip_suffix(']').ok_or_else(|| err(n, "unterminated list"))?;
        if inner.trim().is_empty() {
            return Ok(Value::List(Vec::new()));
        }
        return split_flow(inner, n)?.iter().map(|item| scalar_string(item.trim(), n)).collect::<Result<_, _>>().map(Value::List);
    }
    if v.starts_with('"') || v.starts_with('\'') {
        return Ok(Value::Str(quoted(v, n)?));
    }
    reject_special(v, n)?;
    let plain = strip_comment(v);
    Ok(match plain {
        "" | "null" | "~" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => match plain.parse::<i64>() {
            Ok(i) if !plain.starts_with('+') && !(plain.len() > 1 && plain.starts_with('0')) => Value::Int(i),
            _ => Value::Str(plain.to_string()),
        },
    })
}

fn strip_comment(v: &str) -> &str {
    match v.find(" #") {
        Some(i) => v[..i].trim_end(),
        None => v,
    }
}

fn reject_special(v: &str, n: usize) -> Result<(), ParseError> {
    match v.chars().next() {
        Some('&' | '*' | '!' | '|' | '>' | '{' | '@' | '`' | '%') => Err(err(n, format!("unsupported YAML syntax `{}`", &v[..1]))),
        _ => Ok(()),
    }
}

fn scalar_string(v: &str, n: usize) -> Result<String, ParseError> {
    if v.starts_with('"') || v.starts_with('\'') {
        return quoted(v, n);
    }
    reject_special(v, n)?;
    if v.starts_with('[') {
        return Err(err(n, "nested lists are not supported"));
    }
    Ok(strip_comment(v).to_string())
}

fn quoted(v: &str, n: usize) -> Result<String, ParseError> {
    let q = v.chars().next().unwrap_or('"');
    let mut out = String::new();
    let mut chars = v[1..].chars().peekable();
    loop {
        match chars.next() {
            None => return Err(err(n, "unterminated string")),
            Some(c) if c == q => {
                if q == '\'' && chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                    continue;
                }
                let rest: String = chars.collect();
                let rest = rest.trim();
                if !(rest.is_empty() || rest.starts_with('#')) {
                    return Err(err(n, "unexpected text after string"));
                }
                return Ok(out);
            }
            Some('\\') if q == '"' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                other => return Err(err(n, format!("unsupported escape `\\{}`", other.map(String::from).unwrap_or_default()))),
            },
            Some(c) => out.push(c),
        }
    }
}

/// Split a flow list on commas outside quotes.
fn split_flow(inner: &str, n: usize) -> Result<Vec<String>, ParseError> {
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in inner.chars() {
        match quote {
            Some(q) => {
                cur.push(c);
                if escaped {
                    escaped = false;
                } else if c == '\\' && q == '"' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                cur.push(c);
            }
            None if c == ',' => items.push(std::mem::take(&mut cur)),
            None => cur.push(c),
        }
    }
    if quote.is_some() {
        return Err(err(n, "unterminated string in list"));
    }
    items.push(cur);
    Ok(items)
}

fn needs_quotes(s: &str) -> bool {
    s.is_empty()
        || s != s.trim()
        || matches!(s, "true" | "false" | "null" | "~")
        || s.parse::<i64>().is_ok()
        || s.starts_with(['&', '*', '!', '|', '>', '{', '[', '@', '`', '%', '"', '\'', '#', '-', '?', ':'])
        || s.contains([':', '#', ',', '[', ']', '\n', '\t', '"', '\\'])
}

fn render_scalar(s: &str) -> String {
    if !needs_quotes(s) {
        return s.to_string();
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Render frontmatter (without fences). Lists are written in flow style; parse(render(x)) == x.
pub fn render(fm: &Frontmatter) -> String {
    let mut out = String::new();
    for (k, v) in &fm.entries {
        let value = match v {
            Value::Null => "null".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Str(s) => render_scalar(s),
            Value::List(items) => format!("[{}]", items.iter().map(|i| render_scalar(i)).collect::<Vec<_>>().join(", ")),
        };
        let _ = writeln!(out, "{k}: {value}");
    }
    out
}

/// Render a complete document: fenced frontmatter followed by the body.
pub fn render_document(fm: &Frontmatter, body: &str) -> String {
    let body = body.trim_start_matches(['\r', '\n']);
    format!("---\n{}---\n\n{}{}", render(fm), body, if body.ends_with('\n') || body.is_empty() { "" } else { "\n" })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_supported_subset() {
        let text = "---\n# comment\nnexus: 1\nid: \"0192f2a4-0000-7000-8000-000000000001\"\ntitle: Fix: login 'redirect'\nlabels: [ui, \"needs review\", 'it''s']\nacceptance:\n  - Redirects to /home\n  - \"Keeps #anchors\"\nempty:\ndone: false\npriority: high # trailing comment\n---\n\nBody text\n";
        let (fm, body) = split(text).unwrap();
        let fm = fm.unwrap();
        assert_eq!(fm.get("nexus"), Some(&Value::Int(1)));
        assert_eq!(fm.str("title"), Some("Fix: login 'redirect'"));
        assert_eq!(fm.list("labels"), ["ui", "needs review", "it's"]);
        assert_eq!(fm.list("acceptance"), ["Redirects to /home", "Keeps #anchors"]);
        assert_eq!(fm.get("empty"), Some(&Value::Null));
        assert_eq!(fm.get("done"), Some(&Value::Bool(false)));
        assert_eq!(fm.str("priority"), Some("high"));
        assert_eq!(body, "\nBody text\n");
    }

    #[test]
    fn rejects_everything_outside_the_subset() {
        for bad in [
            "a: &anchor x",
            "a: *alias",
            "a: !!python/object x",
            "a: |\n  multi",
            "a: {b: c}",
            "a:\n  b: c",
            "a: 1\na: 2",
            "a: \"unterminated",
            "a: \"x\" trailing",
            "a: [x, [y]]",
            "- orphan",
            "no colon here",
            "bad key: x",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
        assert!(split("---\nid: x\n").is_err(), "missing closing fence");
        let huge = format!("---\nbody: \"{}\"\n---\n", "x".repeat(MAX_FRONTMATTER_BYTES));
        assert!(split(&huge).is_err());
    }

    #[test]
    fn round_trip_preserves_values_order_and_unknown_keys() {
        let mut fm = Frontmatter::default();
        for (k, v) in [
            ("nexus", Value::Int(1)),
            ("title", Value::Str("Tricky: \"quotes\", #hash, \\slash\nnewline".into())),
            ("labels", Value::List(vec!["a,b".into(), "true".into(), " padded ".into(), String::new()])),
            ("future_field", Value::Str("007".into())),
            ("flag", Value::Bool(true)),
            ("none", Value::Null),
        ] {
            fm.set(k, v);
        }
        let doc = render_document(&fm, "# Heading\n\ntext");
        let (back, body) = split(&doc).unwrap();
        assert_eq!(back.unwrap(), fm);
        assert_eq!(body.trim(), "# Heading\n\ntext");
    }

    #[test]
    fn documents_without_frontmatter_and_crlf() {
        assert_eq!(split("# Just markdown").unwrap(), (None, "# Just markdown"));
        let (fm, body) = split("---\r\ntitle: x\r\n---\r\nbody").unwrap();
        assert_eq!(fm.unwrap().str("title"), Some("x"));
        assert_eq!(body, "body");
    }
}
