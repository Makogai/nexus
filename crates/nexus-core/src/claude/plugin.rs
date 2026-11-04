//! Read-only inventory of a Claude Code plugin directory (or a plain skills folder) — shared by profile
//! diagnostics (installed plugins, ADR-0023) and the AI Kit manager (ADR-0024).
//!
//! Layout (verified against Claude Code 2.1.311): `.claude-plugin/plugin.json`, `skills/<name>/SKILL.md`,
//! `agents/*.md`, `commands/**/*.md`, `hooks/hooks.json`, `.mcp.json`, `output-styles/*.md`; the manifest may point
//! to other component paths. Claude namespaces plugin content as `<plugin>:<name>` (MCP: `plugin:<plugin>:<server>`).
//!
//! Secrets never leave this module: hook commands, MCP arguments, environment values, headers and URL paths are not
//! recorded — only names, counts, transport kinds, the executable's file name and a URL's host.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

const MAX_FILES: usize = 2000;
const MAX_BYTES: u64 = 20 * 1024 * 1024;
const MAX_TEXT_FILE: u64 = 512 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    pub description: String,
    /// Characters of the listing Claude keeps in context at session start (name + description).
    pub listing_chars: usize,
    /// Where the component lives inside the plugin, and where it belongs in the standard layout (overlays).
    #[serde(skip)]
    pub src: PathBuf,
    #[serde(skip)]
    pub dest: PathBuf,
}


#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HookGroup {
    pub event: String,
    pub handlers: usize,
    /// Handler types (`command`, `http`, `prompt`, `agent`, `mcp_tool`).
    pub types: Vec<String>,
    /// File names of the programs command handlers start (never arguments).
    pub programs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpServer {
    pub name: String,
    /// `stdio`, `http`, `sse`.
    pub transport: String,
    /// Executable file name (stdio) or URL host (http/sse) — never arguments, query strings or headers.
    pub target: String,
    pub env_vars: usize,
    pub headers: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    /// Plugin name (manifest `name`, else the folder name).
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
    /// `plugin` (has a manifest) or `skills` (a plain folder of skills).
    pub kind: String,
    pub skills: Vec<Entry>,
    pub agents: Vec<Entry>,
    pub commands: Vec<Entry>,
    pub output_styles: Vec<String>,
    pub hooks: Vec<HookGroup>,
    pub mcp_servers: Vec<McpServer>,
    pub files: usize,
    pub bytes: u64,
    /// Things that make the folder unsafe or unusable (reported, never "fixed").
    pub problems: Vec<String>,
}

impl Inventory {
    /// Estimated tokens Claude spends at session start on this plugin's listings (≈ chars / 4).
    pub fn listing_tokens(&self) -> usize {
        self.skills.iter().chain(&self.agents).chain(&self.commands).map(|e| e.listing_chars).sum::<usize>().div_ceil(4)
    }
    pub fn runs_code(&self) -> bool {
        !self.hooks.is_empty() || !self.mcp_servers.is_empty()
    }
}

/// Executable file name only (`C:\tools\node.exe` → `node.exe`, `npx` → `npx`).
pub fn program_name(cmd: &str) -> String {
    let first = cmd.trim().trim_matches('"');
    let first = if cmd.trim().starts_with('"') { first.split('"').next().unwrap_or(first) } else { first.split_whitespace().next().unwrap_or("") };
    Path::new(first).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// `https://user:pw@mcp.example.com:8443/path?key=…` → `mcp.example.com`.
pub fn url_host(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    host.split(':').next().unwrap_or("").to_string()
}

pub fn mcp_from_value(name: &str, v: &Value) -> McpServer {
    let ty = v.get("type").and_then(Value::as_str).unwrap_or(if v.get("url").is_some() { "http" } else { "stdio" });
    let target = match (v.get("command").and_then(Value::as_str), v.get("url").and_then(Value::as_str)) {
        (Some(c), _) => program_name(c),
        (None, Some(u)) => url_host(u),
        _ => String::new(),
    };
    McpServer {
        name: name.chars().take(100).collect(),
        transport: ty.chars().take(10).collect(),
        target,
        env_vars: v.get("env").and_then(Value::as_object).map(|m| m.len()).unwrap_or(0),
        headers: v.get("headers").and_then(Value::as_object).map(|m| m.len()).unwrap_or(0),
    }
}

/// Hook groups from a settings-style `hooks` object (`{Event: [{matcher, hooks: [{type, command}]}]}`).
pub fn hooks_from_value(v: &Value) -> Vec<HookGroup> {
    let Some(map) = v.as_object() else { return Vec::new() };
    let mut out = Vec::new();
    for (event, groups) in map {
        let mut g = HookGroup { event: event.chars().take(60).collect(), handlers: 0, types: Vec::new(), programs: Vec::new() };
        for group in groups.as_array().into_iter().flatten() {
            for h in group.get("hooks").and_then(Value::as_array).into_iter().flatten() {
                g.handlers += 1;
                let ty = h.get("type").and_then(Value::as_str).unwrap_or("command").to_string();
                if !g.types.contains(&ty) {
                    g.types.push(ty);
                }
                if let Some(c) = h.get("command").and_then(Value::as_str) {
                    let p = program_name(c);
                    if !p.is_empty() && !g.programs.contains(&p) {
                        g.programs.push(p);
                    }
                }
            }
        }
        if g.handlers > 0 {
            out.push(g);
        }
    }
    out
}

/// `name` and `description` from YAML-ish frontmatter (only top-level scalars; nothing is executed).
pub fn front(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut desc) = (None, None);
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with([' ', '\t']) {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches(['"', '\'']).to_string();
            match k.trim() {
                "name" if !v.is_empty() => name = Some(v),
                "description" if !v.is_empty() && !v.starts_with(['|', '>']) => desc = Some(v),
                _ => {}
            }
        }
    }
    (name, desc)
}

fn clip(s: &str, n: usize) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>()) } else { s }
}

fn read_small(p: &Path) -> Option<String> {
    let m = std::fs::symlink_metadata(p).ok()?;
    if !m.is_file() || m.len() > MAX_TEXT_FILE {
        return None;
    }
    std::fs::read_to_string(p).ok()
}

fn entry(name: String, description: Option<String>, src: PathBuf, dest: PathBuf) -> Entry {
    let description = clip(&description.unwrap_or_default(), 300);
    Entry { listing_chars: name.chars().count() + description.chars().count() + 2, name: clip(&name, 100), description, src, dest }
}

/// A relative manifest path that stays inside the plugin (no `..`, not absolute).
pub fn confined(base: &Path, rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_) | Component::RootDir)) {
        return None;
    }
    Some(base.join(p))
}

fn manifest_paths(m: &Value, key: &str, base: &Path, default: &str, problems: &mut Vec<String>) -> Vec<PathBuf> {
    let mut out = vec![base.join(default)];
    let extra: Vec<&str> = match m.get(key) {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    for rel in extra {
        match confined(base, rel) {
            Some(p) => out.push(p),
            None => problems.push(format!("manifest `{key}` points outside the plugin: {rel}")),
        }
    }
    out
}

/// Inventory of the plugin (or skills folder) at `dir`.
pub fn inventory(dir: &Path) -> Inventory {
    let mut inv = Inventory { kind: "skills".into(), ..Default::default() };
    let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "plugin".into());
    let manifest_path = dir.join(".claude-plugin").join("plugin.json");
    let manifest: Value = match read_small(&manifest_path) {
        Some(t) => match serde_json::from_str(&t) {
            Ok(v) => {
                inv.kind = "plugin".into();
                v
            }
            Err(e) => {
                inv.problems.push(format!("plugin.json is not valid JSON: {e}"));
                Value::Null
            }
        },
        None => Value::Null,
    };
    inv.name = manifest.get("name").and_then(Value::as_str).map(|s| clip(s, 100)).unwrap_or(folder);
    if !valid_plugin_name(&inv.name) {
        inv.problems.push(format!("plugin name “{}” must use letters, digits, '-' or '_'", inv.name));
    }
    inv.version = manifest.get("version").and_then(Value::as_str).map(|s| clip(s, 40));
    inv.description = manifest.get("description").and_then(Value::as_str).map(|s| clip(s, 300));

    let (files, bytes, symlinks) = tree_size(dir);
    (inv.files, inv.bytes) = (files, bytes);
    if symlinks > 0 {
        inv.problems.push(format!("{symlinks} symbolic link(s) — not followed (they could point outside the kit)"));
    }
    if files >= MAX_FILES || bytes > MAX_BYTES {
        inv.problems.push("larger than 2,000 files / 20 MB".into());
    }

    for skills_dir in manifest_paths(&manifest, "skills", dir, "skills", &mut inv.problems) {
        for e in sorted_dirs(&skills_dir) {
            if let Some(t) = read_small(&e.join("SKILL.md")) {
                let (n, d) = front(&t);
                let dest = Path::new("skills").join(file_name(&e));
                inv.skills.push(entry(n.unwrap_or_else(|| file_name(&e)), d, e.clone(), dest));
            }
        }
    }
    for agents_dir in manifest_paths(&manifest, "agents", dir, "agents", &mut inv.problems) {
        for f in sorted_md(&agents_dir, false) {
            if let Some(t) = read_small(&f) {
                let (n, d) = front(&t);
                let dest = Path::new("agents").join(file_name(&f));
                inv.agents.push(entry(n.unwrap_or_else(|| stem(&f)), d, f.clone(), dest));
            }
        }
    }
    for commands_dir in manifest_paths(&manifest, "commands", dir, "commands", &mut inv.problems) {
        for f in sorted_md(&commands_dir, true) {
            if let Some(t) = read_small(&f) {
                let rel = f.strip_prefix(&commands_dir).unwrap_or(&f).to_path_buf();
                let name = rel.with_extension("").to_string_lossy().replace(['\\', '/'], ":");
                inv.commands.push(entry(name, front(&t).1, f.clone(), Path::new("commands").join(&rel)));
            }
        }
    }
    inv.output_styles = sorted_md(&dir.join("output-styles"), false).iter().map(|f| stem(f)).collect();

    // Hooks: hooks/hooks.json, or manifest `hooks` (path or inline object).
    let mut hook_docs: Vec<Value> = Vec::new();
    if let Some(t) = read_small(&dir.join("hooks").join("hooks.json")) {
        match serde_json::from_str::<Value>(&t) {
            Ok(v) => hook_docs.push(v),
            Err(_) => inv.problems.push("hooks/hooks.json is not valid JSON".into()),
        }
    }
    match manifest.get("hooks") {
        Some(Value::String(rel)) => match confined(dir, rel).and_then(|p| read_small(&p)).and_then(|t| serde_json::from_str(&t).ok()) {
            Some(v) => hook_docs.push(v),
            None => inv.problems.push(format!("manifest `hooks` cannot be read inside the plugin: {rel}")),
        },
        Some(v @ Value::Object(_)) => hook_docs.push(v.clone()),
        _ => {}
    }
    for d in hook_docs {
        inv.hooks.extend(hooks_from_value(d.get("hooks").unwrap_or(&d)));
    }
    // MCP: .mcp.json, or manifest `mcpServers` (path or inline).
    let mut mcp_docs: Vec<Value> = Vec::new();
    if let Some(t) = read_small(&dir.join(".mcp.json")) {
        match serde_json::from_str::<Value>(&t) {
            Ok(v) => mcp_docs.push(v),
            Err(_) => inv.problems.push(".mcp.json is not valid JSON".into()),
        }
    }
    match manifest.get("mcpServers") {
        Some(Value::String(rel)) => {
            if let Some(v) = confined(dir, rel).and_then(|p| read_small(&p)).and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
                mcp_docs.push(v);
            }
        }
        Some(v @ Value::Object(_)) => mcp_docs.push(serde_json::json!({ "mcpServers": v })),
        _ => {}
    }
    for d in mcp_docs {
        for (name, v) in d.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
            inv.mcp_servers.push(mcp_from_value(name, v));
        }
    }
    if inv.kind == "skills" && inv.skills.is_empty() {
        inv.problems.push("neither a plugin (.claude-plugin/plugin.json) nor a folder with skills/<name>/SKILL.md".into());
    }
    inv
}

pub fn valid_plugin_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 64 && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn sorted_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    v.sort();
    v.truncate(500);
    v
}

fn sorted_md(dir: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let p = e.path();
            if ft.is_dir() && recursive && depth < 4 {
                stack.push((p, depth + 1));
            } else if ft.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out.truncate(500);
    out
}

/// (files, bytes, symlinks) under `dir`, bounded; symlinks are counted, never followed.
pub fn tree_size(dir: &Path) -> (usize, u64, usize) {
    let (mut files, mut bytes, mut links) = (0, 0u64, 0);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                links += 1;
            } else if ft.is_dir() {
                if e.file_name() != ".git" {
                    stack.push(e.path());
                }
            } else if ft.is_file() {
                files += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
            if files >= MAX_FILES {
                return (files, bytes, links);
            }
        }
    }
    (files, bytes, links)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(p: &Path, t: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }

    #[test]
    fn inventories_a_plugin_without_recording_secrets() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("kit");
        w(&d.join(".claude-plugin/plugin.json"), r#"{"name":"probe-kit","version":"1.2.3","description":"Probe","commands":["./extra"],"agents":"../escape"}"#);
        w(&d.join("skills/alpha/SKILL.md"), "---\nname: alpha\ndescription: Alpha skill\n---\nbody");
        w(&d.join("skills/beta/SKILL.md"), "---\ndescription: Beta\n---\n");
        w(&d.join("agents/helper.md"), "---\nname: helper\ndescription: Helps\n---\n");
        w(&d.join("commands/hello.md"), "---\ndescription: Say hello\n---\n");
        w(&d.join("extra/frontend/comp.md"), "---\ndescription: Component\n---\n");
        w(&d.join("hooks/hooks.json"), r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"command","command":"C:\\tools\\node.exe run.js --token SECRET-HOOK"}]}]}}"#);
        w(&d.join(".mcp.json"), r#"{"mcpServers":{"db":{"command":"uvx","args":["--key","SECRET-ARG"],"env":{"TOKEN":"SECRET-ENV"}},"web":{"type":"http","url":"https://user:SECRET-PW@mcp.example.com:8443/v1?key=SECRET-Q","headers":{"Authorization":"Bearer SECRET-H"}}}}"#);
        let inv = inventory(&d);
        assert_eq!((inv.name.as_str(), inv.version.as_deref(), inv.kind.as_str()), ("probe-kit", Some("1.2.3"), "plugin"));
        assert_eq!(inv.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["alpha", "beta"]);
        assert_eq!(inv.commands.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["hello", "frontend:comp"]);
        assert_eq!(inv.hooks[0].programs, ["node.exe"]);
        assert_eq!((inv.mcp_servers[0].target.as_str(), inv.mcp_servers[0].env_vars), ("uvx", 1));
        assert_eq!((inv.mcp_servers[1].transport.as_str(), inv.mcp_servers[1].target.as_str(), inv.mcp_servers[1].headers), ("http", "mcp.example.com", 1));
        assert!(inv.problems.iter().any(|p| p.contains("outside the plugin")), "{:?}", inv.problems);
        assert!(inv.runs_code() && inv.listing_tokens() > 0);
        let dump = serde_json::to_string(&inv).unwrap();
        assert!(!dump.contains("SECRET"), "{dump}");
    }

    #[test]
    fn a_plain_skills_folder_is_a_kit_too() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("team-skills");
        w(&d.join("skills/review/SKILL.md"), "---\nname: review\ndescription: Code review\n---\n");
        let inv = inventory(&d);
        assert_eq!((inv.name.as_str(), inv.kind.as_str(), inv.skills.len()), ("team-skills", "skills", 1));
        assert!(inventory(&t.path().join("missing")).problems.iter().any(|p| p.contains("neither a plugin")));
        assert_eq!(program_name("\"C:\\Program Files\\node.exe\" x.js"), "node.exe");
        assert_eq!(url_host("http://localhost:3000/mcp"), "localhost");
    }
}
