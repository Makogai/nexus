//! Claude profile diagnostics (M14, ADR-0023): what a profile's configuration actually contains — executable,
//! organisation policy, user settings, memory, skills, agents, commands, MCP servers, hooks, plugins and marketplaces,
//! NEXUS kits — and how much listing context it costs every session.
//!
//! **Read-only and allowlisted.** Nothing is written; `.credentials.json` is never opened; from `.claude.json` only
//! `mcpServers` (user scope) and `projects.*.mcpServers` (local scope) are read. Values are reported only for known,
//! non-sensitive settings; everything else is reduced to key names, counts or presence flags. Environment values,
//! hook commands, MCP arguments, headers and URL paths never leave this module (see `claude::plugin`).
//!
//! File locations verified with Claude Code 2.1.311 (synthetic profiles): `<config>/settings.json`
//! (`enabledPlugins`, `extraKnownMarketplaces`, `hooks`, …), `<config>/plugins/installed_plugins.json` (v2),
//! `<config>/plugins/known_marketplaces.json`, `<config>/.claude.json` (for the default profile `~/.claude.json`).

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::plugin::{self, Entry, HookGroup, Inventory, McpServer};
use super::policy::{self, MachinePolicyLocations, PolicySummary};
use super::profile::{ClaudeProfile, default_config_dir};
use crate::paths;

const MAX_JSON: u64 = 8 * 1024 * 1024;
const LISTING_WARN_TOKENS: usize = 3000;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSummary {
    pub path: PathBuf,
    pub exists: bool,
    pub parse_error: Option<String>,
    pub model: Option<String>,
    pub default_permission_mode: Option<String>,
    pub output_style: Option<String>,
    pub permission_rules: (usize, usize, usize),
    pub additional_directories: usize,
    /// Environment variable *names* the settings set (values are never read out).
    pub env_names: Vec<String>,
    pub sensitive_env_names: Vec<String>,
    pub api_key_helper: bool,
    pub cloud_credential_helper: bool,
    pub status_line: bool,
    pub disable_all_hooks: bool,
    pub claude_md_excludes: usize,
    pub enabled_plugins: Vec<String>,
    pub disabled_plugins: Vec<String>,
    pub extra_marketplaces: Vec<String>,
    /// Names of other top-level keys (values not shown).
    pub other_keys: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// The name as Claude shows it (`alpha`, `probe-kit:alpha`).
    pub name: String,
    /// `user`, `plugin`, `kit`.
    pub source: &'static str,
    pub origin: Option<String>,
    pub description: String,
    pub listing_chars: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpItem {
    /// The name as Claude reports it (`db`, `plugin:probe-kit:db`); the server's own name is `name`.
    pub qualified_name: String,
    /// `user`, `local`, `plugin`, `managed`, `kit`.
    pub scope: &'static str,
    pub origin: Option<String>,
    #[serde(flatten)]
    pub server: McpServer,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HookItem {
    /// `user`, `plugin`, `kit`.
    pub source: &'static str,
    pub origin: Option<String>,
    #[serde(flatten)]
    pub group: HookGroup,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginItem {
    pub id: String,
    pub name: String,
    pub marketplace: Option<String>,
    pub version: Option<String>,
    pub scope: Option<String>,
    pub enabled: bool,
    pub installed: bool,
    pub install_path: Option<PathBuf>,
    pub skills: usize,
    pub agents: usize,
    pub commands: usize,
    pub hooks: usize,
    pub mcp_servers: usize,
    pub listing_tokens: usize,
    pub runs_code: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Marketplace {
    pub name: String,
    pub kind: String,
    pub location: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContextEstimate {
    pub skills_tokens: usize,
    pub agents_tokens: usize,
    pub commands_tokens: usize,
    pub memory_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummary {
    pub claude_md_lines: usize,
    pub claude_md_chars: usize,
    pub rules: usize,
    pub rules_chars: usize,
}

/// A NEXUS kit enabled for this profile (session-scoped `--plugin-dir`, ADR-0024).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KitUse {
    pub kit_id: String,
    pub inventory: Inventory,
    /// Why NEXUS will not pass it (`None` = it reaches this profile's NEXUS sessions).
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub profile_id: String,
    pub profile_name: String,
    pub config_dir: PathBuf,
    pub default_location: bool,
    pub config_exists: bool,
    pub managed_by_nexus: bool,
    pub isolate_instructions: bool,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub executable_error: Option<String>,
    pub policy: PolicySummary,
    pub policy_effects: Vec<String>,
    pub settings: SettingsSummary,
    pub memory: MemorySummary,
    pub skills: Vec<Item>,
    pub agents: Vec<Item>,
    pub commands: Vec<Item>,
    pub output_styles: Vec<String>,
    pub mcp_servers: Vec<McpItem>,
    pub hooks: Vec<HookItem>,
    pub plugins: Vec<PluginItem>,
    pub marketplaces: Vec<Marketplace>,
    pub kits: Vec<KitUse>,
    pub context: ContextEstimate,
    pub warnings: Vec<String>,
}

pub struct Input<'a> {
    pub profile: &'a ClaudeProfile,
    pub home: &'a Path,
    pub machine: &'a MachinePolicyLocations,
    pub executable: Option<PathBuf>,
    pub version: Option<String>,
    pub executable_error: Option<String>,
    pub kits: Vec<KitUse>,
}

fn read_json(p: &Path) -> Result<Option<Value>, String> {
    let Ok(meta) = std::fs::symlink_metadata(p) else { return Ok(None) };
    if !meta.is_file() {
        return Ok(None);
    }
    if meta.len() > MAX_JSON {
        return Err("larger than 8 MB".into());
    }
    let text = std::fs::read_to_string(p).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map(Some).map_err(|e| format!("not valid JSON ({e})"))
}

const KNOWN_KEYS: &[&str] = &[
    "model", "permissions", "outputStyle", "env", "apiKeyHelper", "awsAuthRefresh", "awsCredentialExport", "statusLine",
    "hooks", "disableAllHooks", "claudeMdExcludes", "enabledPlugins", "extraKnownMarketplaces", "$schema",
];

fn sensitive(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    ["KEY", "TOKEN", "SECRET", "PASSWORD", "PASSWD", "AUTH", "CREDENTIAL", "COOKIE", "SESSION"].iter().any(|m| u.contains(m))
}

fn short_str(v: Option<&Value>, max: usize) -> Option<String> {
    v.and_then(Value::as_str)
        .filter(|s| s.len() <= max && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:[]/@ ".contains(c)))
        .map(str::to_string)
}

fn settings(path: PathBuf) -> (SettingsSummary, Value) {
    let mut s = SettingsSummary { path: path.clone(), ..Default::default() };
    let v = match read_json(&path) {
        Ok(Some(v)) => v,
        Ok(None) => return (s, Value::Null),
        Err(e) => {
            s.exists = true;
            s.parse_error = Some(e);
            return (s, Value::Null);
        }
    };
    s.exists = true;
    s.model = short_str(v.get("model"), 100);
    s.output_style = short_str(v.get("outputStyle"), 60);
    if let Some(p) = v.get("permissions") {
        let n = |k: &str| p.get(k).and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        s.permission_rules = (n("allow"), n("ask"), n("deny"));
        s.default_permission_mode = short_str(p.get("defaultMode"), 40);
        s.additional_directories = n("additionalDirectories");
    }
    if let Some(env) = v.get("env").and_then(Value::as_object) {
        s.env_names = env.keys().take(100).cloned().collect();
        s.sensitive_env_names = s.env_names.iter().filter(|n| sensitive(n)).cloned().collect();
    }
    s.api_key_helper = v.get("apiKeyHelper").is_some();
    s.cloud_credential_helper = v.get("awsAuthRefresh").is_some() || v.get("awsCredentialExport").is_some();
    s.status_line = v.get("statusLine").is_some();
    s.disable_all_hooks = v.get("disableAllHooks").and_then(Value::as_bool).unwrap_or(false);
    s.claude_md_excludes = v.get("claudeMdExcludes").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    for (id, on) in v.get("enabledPlugins").and_then(Value::as_object).into_iter().flatten() {
        if on.as_bool().unwrap_or(false) { s.enabled_plugins.push(id.clone()) } else { s.disabled_plugins.push(id.clone()) }
    }
    s.extra_marketplaces = v.get("extraKnownMarketplaces").and_then(Value::as_object).map(|m| m.keys().cloned().collect()).unwrap_or_default();
    s.other_keys = v.as_object().map(|m| m.keys().filter(|k| !KNOWN_KEYS.contains(&k.as_str())).take(50).cloned().collect()).unwrap_or_default();
    (s, v)
}

fn items(entries: &[Entry], source: &'static str, origin: Option<&str>) -> Vec<Item> {
    entries
        .iter()
        .map(|e| Item {
            name: match origin {
                Some(o) if source != "user" => format!("{o}:{}", e.name),
                _ => e.name.clone(),
            },
            source,
            origin: origin.map(str::to_string),
            description: e.description.clone(),
            listing_chars: e.listing_chars,
        })
        .collect()
}

pub fn diagnose(i: &Input<'_>) -> Diagnostics {
    let p = i.profile;
    let config_dir = p.effective_config_dir(i.home);
    let default_location = p.config_dir.is_none();
    let pol = policy::evaluate(&config_dir, i.machine);
    let (settings, settings_doc) = settings(config_dir.join("settings.json"));
    let mut warnings = Vec::new();

    // User-level content (the profile's own config dir).
    let user = plugin::inventory(&config_dir);
    let mut skills = items(&user.skills, "user", None);
    let mut agents = items(&user.agents, "user", None);
    let mut commands = items(&user.commands, "user", None);
    let mut output_styles = user.output_styles.clone();
    let mut hooks: Vec<HookItem> = settings_doc
        .get("hooks")
        .map(plugin::hooks_from_value)
        .unwrap_or_default()
        .into_iter()
        .map(|group| HookItem { source: "user", origin: None, group })
        .collect();

    // MCP servers from .claude.json (only these keys are read).
    let claude_json = if default_location { i.home.join(".claude.json") } else { config_dir.join(".claude.json") };
    let mut mcp_servers = Vec::new();
    match read_json(&claude_json) {
        Ok(Some(doc)) => {
            for (name, v) in doc.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
                mcp_servers.push(McpItem { qualified_name: name.clone(), scope: "user", origin: None, server: plugin::mcp_from_value(name, v) });
            }
            for (proj, entry) in doc.get("projects").and_then(Value::as_object).into_iter().flatten() {
                let folder = Path::new(proj).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "project".into());
                for (name, v) in entry.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
                    mcp_servers.push(McpItem { qualified_name: name.clone(), scope: "local", origin: Some(folder.clone()), server: plugin::mcp_from_value(name, v) });
                }
            }
        }
        Ok(None) => {}
        Err(e) => warnings.push(format!("{} could not be read: {e}", claude_json.display())),
    }
    if let Some(dir) = i.machine.managed_dir()
        && let Ok(Some(doc)) = read_json(&dir.join("managed-mcp.json"))
    {
        for (name, v) in doc.get("mcpServers").and_then(Value::as_object).into_iter().flatten() {
            mcp_servers.push(McpItem { qualified_name: name.clone(), scope: "managed", origin: None, server: plugin::mcp_from_value(name, v) });
        }
    }

    // Installed plugins (registry v2) and marketplaces.
    let mut plugins = Vec::new();
    let registry = read_json(&config_dir.join("plugins").join("installed_plugins.json")).ok().flatten().unwrap_or(Value::Null);
    let installed = registry.get("plugins").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut ids: Vec<String> = installed.keys().cloned().collect();
    for id in settings.enabled_plugins.iter().chain(&settings.disabled_plugins) {
        if !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    for id in ids {
        let (name, marketplace) = match id.split_once('@') {
            Some((n, m)) => (n.to_string(), Some(m.to_string())),
            None => (id.clone(), None),
        };
        let entry = installed.get(&id).and_then(Value::as_array).and_then(|a| a.iter().find(|e| e.get("scope").and_then(Value::as_str) == Some("user")).or(a.first())).cloned();
        let install_path = entry.as_ref().and_then(|e| e.get("installPath")).and_then(Value::as_str).map(PathBuf::from);
        let enabled = settings.enabled_plugins.contains(&id);
        let inv = install_path.as_deref().filter(|p| p.is_dir()).map(plugin::inventory);
        if entry.is_some() && inv.is_none() {
            warnings.push(format!("plugin {id} is registered but its files are missing"));
        }
        if entry.is_none() && enabled {
            warnings.push(format!("plugin {id} is enabled in settings but not installed"));
        }
        if let Some(inv) = inv.as_ref().filter(|_| enabled) {
            skills.extend(items(&inv.skills, "plugin", Some(&name)));
            agents.extend(items(&inv.agents, "plugin", Some(&name)));
            commands.extend(items(&inv.commands, "plugin", Some(&name)));
            output_styles.extend(inv.output_styles.iter().map(|o| format!("{name}:{o}")));
            hooks.extend(inv.hooks.iter().cloned().map(|group| HookItem { source: "plugin", origin: Some(name.clone()), group }));
            mcp_servers.extend(inv.mcp_servers.iter().cloned().map(|server| McpItem { qualified_name: format!("plugin:{name}:{}", server.name), scope: "plugin", origin: Some(name.clone()), server }));
        }
        plugins.push(PluginItem {
            id: id.clone(),
            name,
            marketplace,
            version: entry.as_ref().and_then(|e| e.get("version")).and_then(Value::as_str).map(str::to_string),
            scope: entry.as_ref().and_then(|e| e.get("scope")).and_then(Value::as_str).map(str::to_string),
            enabled,
            installed: inv.is_some(),
            install_path,
            skills: inv.as_ref().map(|v| v.skills.len()).unwrap_or(0),
            agents: inv.as_ref().map(|v| v.agents.len()).unwrap_or(0),
            commands: inv.as_ref().map(|v| v.commands.len()).unwrap_or(0),
            hooks: inv.as_ref().map(|v| v.hooks.iter().map(|h| h.handlers).sum()).unwrap_or(0),
            mcp_servers: inv.as_ref().map(|v| v.mcp_servers.len()).unwrap_or(0),
            listing_tokens: inv.as_ref().map(Inventory::listing_tokens).unwrap_or(0),
            runs_code: inv.as_ref().is_some_and(Inventory::runs_code),
        });
    }
    let marketplaces = read_json(&config_dir.join("plugins").join("known_marketplaces.json"))
        .ok()
        .flatten()
        .and_then(|v| v.as_object().cloned())
        .map(|m| {
            m.iter()
                .map(|(name, v)| {
                    let src = v.get("source").cloned().unwrap_or(Value::Null);
                    let kind = src.get("source").and_then(Value::as_str).unwrap_or("unknown").to_string();
                    let location = ["path", "repo", "url"]
                        .iter()
                        .find_map(|k| src.get(*k).and_then(Value::as_str))
                        .map(crate::git::redact_url)
                        .unwrap_or_default();
                    Marketplace { name: name.clone(), kind, location }
                })
                .collect()
        })
        .unwrap_or_default();

    // NEXUS kits (session-scoped).
    for k in i.kits.iter().filter(|k| k.blocked.is_none()) {
        let n = &k.inventory.name;
        skills.extend(items(&k.inventory.skills, "kit", Some(n)));
        agents.extend(items(&k.inventory.agents, "kit", Some(n)));
        commands.extend(items(&k.inventory.commands, "kit", Some(n)));
        hooks.extend(k.inventory.hooks.iter().cloned().map(|group| HookItem { source: "kit", origin: Some(n.clone()), group }));
        mcp_servers.extend(k.inventory.mcp_servers.iter().cloned().map(|server| McpItem { qualified_name: format!("plugin:{n}:{}", server.name), scope: "kit", origin: Some(n.clone()), server }));
    }

    // Memory and context estimate.
    let memory = {
        let md = std::fs::read_to_string(config_dir.join("CLAUDE.md")).unwrap_or_default();
        let mut rules_chars = 0;
        let mut rules = 0;
        for f in std::fs::read_dir(config_dir.join("rules")).into_iter().flatten().flatten() {
            if f.path().extension().is_some_and(|x| x == "md") {
                rules += 1;
                rules_chars += std::fs::metadata(f.path()).map(|m| m.len() as usize).unwrap_or(0);
            }
        }
        MemorySummary { claude_md_lines: md.lines().count(), claude_md_chars: md.chars().count(), rules, rules_chars }
    };
    let tok = |v: &[Item]| v.iter().map(|x| x.listing_chars).sum::<usize>().div_ceil(4);
    let mut context = ContextEstimate {
        skills_tokens: tok(&skills),
        agents_tokens: tok(&agents),
        commands_tokens: tok(&commands),
        memory_tokens: (memory.claude_md_chars + memory.rules_chars).div_ceil(4),
        total_tokens: 0,
    };
    context.total_tokens = context.skills_tokens + context.agents_tokens + context.commands_tokens + context.memory_tokens;

    // Explanations and warnings.
    let policy_effects = effects(&pol);
    if !config_dir.is_dir() {
        warnings.push(format!("{} does not exist yet — Claude creates it on first sign-in", config_dir.display()));
    }
    if let Some(e) = &i.executable_error {
        warnings.push(format!("Claude Code executable: {e}"));
    }
    if let Some(err) = &settings.parse_error {
        warnings.push(format!("settings.json cannot be used: {err}"));
    }
    if !settings.sensitive_env_names.is_empty() {
        warnings.push(format!(
            "settings.json sets credential-like variables for every session of this profile ({}) — values are not shown",
            settings.sensitive_env_names.join(", ")
        ));
    }
    let default_cfg = default_config_dir(i.home);
    if !default_location
        && !p.isolate_instructions
        && (default_cfg.join("CLAUDE.md").is_file() || default_cfg.join("rules").is_dir())
        && !paths::same_path(&config_dir, &default_cfg)
    {
        warnings.push(
            "The default profile's ~/.claude/CLAUDE.md or rules also reach this profile for projects inside your home folder — \
             consider instruction isolation for this profile."
                .into(),
        );
    }
    let mut seen = std::collections::HashMap::new();
    for pl in plugins.iter().filter(|p| p.enabled).map(|p| p.name.clone()).chain(i.kits.iter().filter(|k| k.blocked.is_none()).map(|k| k.inventory.name.clone())) {
        *seen.entry(pl).or_insert(0) += 1;
    }
    for (name, n) in seen.into_iter().filter(|(_, n)| *n > 1) {
        warnings.push(format!("{n} plugins/kits named “{name}” are active — Claude merges them into one namespace, so their skills can collide"));
    }
    if context.total_tokens > LISTING_WARN_TOKENS {
        warnings.push(format!(
            "About {} tokens of skill/agent/command listings and memory are loaded at every session start — consider disabling unused plugins or kit skills",
            context.total_tokens
        ));
    }
    Diagnostics {
        profile_id: p.id.clone(),
        profile_name: p.name.clone(),
        config_exists: config_dir.is_dir(),
        config_dir,
        default_location,
        managed_by_nexus: p.managed_by_nexus,
        isolate_instructions: p.isolate_instructions,
        executable: i.executable.clone(),
        version: i.version.clone(),
        executable_error: i.executable_error.clone(),
        policy_effects,
        policy: pol,
        settings,
        memory,
        skills,
        agents,
        commands,
        output_styles,
        mcp_servers,
        hooks,
        plugins,
        marketplaces,
        kits: i.kits.clone(),
        context,
        warnings,
    }
}

/// Everything for one profile: resolves and asks the executable for its version (`claude --version`, with the
/// profile's own environment — read-only), adds the profile's NEXUS kits, then inspects the files.
pub fn collect(
    conn: &rusqlite::Connection,
    data_dir: &Path,
    home: &Path,
    profile: &ClaudeProfile,
    machine: &MachinePolicyLocations,
    base_env: &[(String, String)],
) -> crate::error::Result<Diagnostics> {
    let (executable, executable_error) = match super::install::resolve(profile.executable.as_deref(), home) {
        Ok(p) => (Some(p), None),
        Err(e) => (None, Some(e.to_string())),
    };
    let version = executable.as_deref().and_then(|exe| {
        let (env, _) = super::launch::profile_env(profile, base_env, &[]);
        super::install::version_with_env(exe, &env)
    });
    let pol = policy::evaluate(&profile.effective_config_dir(home), machine);
    let kits = crate::kits::uses_for_profile(conn, data_dir, &profile.id, &pol)?;
    Ok(diagnose(&Input { profile, home, machine, executable, version, executable_error, kits }))
}

/// What the organisation's managed settings mean for this profile, in words.
pub fn effects(p: &PolicySummary) -> Vec<String> {
    let mut v = Vec::new();
    if p.disable_sideload_flags {
        v.push("Session add-ons are disabled (disableSideloadFlags): NEXUS cannot add its MCP tools or AI Kits to this profile's sessions.".into());
    }
    if p.strict_plugin_only_customization {
        v.push("Customisation is restricted to approved plugins (strictPluginOnlyCustomization): NEXUS does not add MCP tools or AI Kits.".into());
    }
    if p.allow_managed_mcp_servers_only {
        v.push("Only organisation MCP servers run (allowManagedMcpServersOnly): user, plugin and kit servers are ignored.".into());
    }
    if p.disable_all_hooks {
        v.push("All hooks are disabled (disableAllHooks), including NEXUS lifecycle hooks and kit hooks.".into());
    } else if p.allow_managed_hooks_only {
        v.push("Only organisation hooks run (allowManagedHooksOnly): NEXUS lifecycle hooks and kit hooks do not.".into());
    }
    if p.allow_managed_permission_rules_only {
        v.push("Permission rules come only from the organisation (allowManagedPermissionRulesOnly).".into());
    }
    if p.bypass_permissions_disabled {
        v.push("Bypass-permissions mode is disabled by the organisation.".into());
    }
    if p.managed_telemetry {
        v.push("Telemetry is configured by the organisation.".into());
    }
    if p.managed_claude_md {
        v.push("Organisation instructions (managed CLAUDE.md) apply to every session and cannot be excluded.".into());
    }
    if !p.claude_md_excludes.is_empty() {
        v.push(format!("The organisation excludes {} instruction file pattern(s) (claudeMdExcludes).", p.claude_md_excludes.len()));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(p: &Path, t: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }

    fn profile(cfg: &Path) -> ClaudeProfile {
        ClaudeProfile {
            id: "p".into(), name: "Work".into(), description: String::new(), color: String::new(), config_dir: Some(cfg.to_path_buf()),
            executable: None, extra_args: vec![], managed_by_nexus: false, inherit_provider_env: false, isolate_instructions: false,
            default_work_profile: None, default_effort: None, default_model: None, created_at: 0, updated_at: 0, last_used_at: None, archived_at: None,
        }
    }

    #[test]
    fn explains_the_configuration_and_never_exposes_secrets() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path();
        let cfg = root.join("cfg");
        let home = root.join("home");
        w(&home.join(".claude/CLAUDE.md"), "personal");
        w(&cfg.join("settings.json"), r#"{
            "model": "claude-opus-5-5", "env": {"ANTHROPIC_API_KEY": "sk-ant-SECRET-1", "DEBUG": "1"},
            "apiKeyHelper": "C:/bin/get-key.cmd --vault SECRET-2", "permissions": {"allow": ["Bash(npm:*)"], "deny": ["Read(./.env)"], "defaultMode": "acceptEdits"},
            "hooks": {"PostToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "notify.exe --webhook https://hooks.example/SECRET-3"}]}]},
            "enabledPlugins": {"probe-kit@probe-market": true, "ghost@probe-market": true, "off@probe-market": false},
            "extraKnownMarketplaces": {"probe-market": {"source": {"source": "git", "url": "https://user:SECRET-4@git.example/m.git"}}},
            "spinnerTips": true
        }"#);
        w(&cfg.join(".claude.json"), r#"{"primaryApiKey": "SECRET-5", "oauthAccount": {"emailAddress": "someone@example.invalid"},
            "mcpServers": {"db": {"type": "stdio", "command": "uvx", "args": ["--token", "SECRET-6"], "env": {"PGPASSWORD": "SECRET-7"}}},
            "projects": {"C:/work/shop": {"mcpServers": {"local-db": {"command": "node", "args": ["SECRET-8"]}}}}}"#);
        w(&cfg.join(".credentials.json"), r#"{"claudeAiOauth": {"accessToken": "SECRET-9"}}"#);
        w(&cfg.join("skills/user-skill/SKILL.md"), "---\nname: user-skill\ndescription: Mine\n---\n");
        w(&cfg.join("agents/reviewer.md"), "---\nname: reviewer\ndescription: Reviews\n---\n");
        let plugin = cfg.join("plugins/cache/probe-market/probe-kit/1.2.3");
        w(&plugin.join(".claude-plugin/plugin.json"), r#"{"name":"probe-kit","version":"1.2.3"}"#);
        w(&plugin.join("skills/alpha/SKILL.md"), "---\nname: alpha\ndescription: Alpha\n---\n");
        w(&plugin.join(".mcp.json"), r#"{"mcpServers":{"probe-mcp":{"command":"node","args":["SECRET-10"]}}}"#);
        w(&cfg.join("plugins/installed_plugins.json"), &format!(r#"{{"version":2,"plugins":{{"probe-kit@probe-market":[{{"scope":"user","installPath":{},"version":"1.2.3"}}]}}}}"#, serde_json::to_string(&plugin).unwrap()));
        w(&cfg.join("plugins/known_marketplaces.json"), r#"{"probe-market":{"source":{"source":"git","url":"https://user:SECRET-11@git.example/m.git"}}}"#);
        w(&cfg.join("remote-settings.json"), r#"{"allowManagedHooksOnly": true, "env": {"OTEL_EXPORTER_OTLP_HEADERS": "SECRET-12"}}"#);

        let p = profile(&cfg);
        let d = diagnose(&Input { profile: &p, home: &home, machine: &MachinePolicyLocations::default(), executable: None, version: Some("2.1.311".into()), executable_error: None, kits: vec![] });
        assert_eq!(d.settings.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(d.settings.permission_rules, (1, 0, 1));
        assert_eq!(d.settings.sensitive_env_names, ["ANTHROPIC_API_KEY"]);
        assert!(d.settings.api_key_helper && d.settings.other_keys == ["spinnerTips"]);
        assert_eq!(d.skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["user-skill", "probe-kit:alpha"], "names as Claude shows them");
        assert_eq!(d.mcp_servers.iter().map(|m| (m.qualified_name.as_str(), m.scope)).collect::<Vec<_>>(), [("db", "user"), ("local-db", "local"), ("plugin:probe-kit:probe-mcp", "plugin")]);
        assert_eq!(d.mcp_servers[1].origin.as_deref(), Some("shop"), "project folder name only");
        assert_eq!(d.hooks[0].group.programs, ["notify.exe"]);
        assert_eq!(d.plugins.iter().map(|p| (p.id.as_str(), p.enabled, p.installed)).collect::<Vec<_>>(), [
            ("probe-kit@probe-market", true, true), ("ghost@probe-market", true, false), ("off@probe-market", false, false)
        ]);
        assert_eq!(d.marketplaces[0].location, "https://git.example/m.git", "credentials removed from URLs");
        assert!(d.policy.allow_managed_hooks_only && d.policy_effects.iter().any(|e| e.contains("allowManagedHooksOnly")));
        assert!(d.warnings.iter().any(|w| w.contains("ghost@probe-market is enabled in settings but not installed")));
        assert!(d.warnings.iter().any(|w| w.contains("credential-like variables")));
        assert!(d.warnings.iter().any(|w| w.contains("instruction isolation")));
        assert!(d.context.total_tokens > 0);
        let dump = serde_json::to_string(&d).unwrap();
        for leak in ["SECRET", "sk-ant", "someone@example.invalid", "get-key", "webhook", "C:/work"] {
            assert!(!dump.contains(leak), "leaked {leak}: {dump}");
        }
    }
}
