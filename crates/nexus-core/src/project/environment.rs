//! Development environment requirements (Portable Workspace P5), detected deterministically from the project's own
//! files and recorded in `project-context/<slug>/environment.json` so another computer knows what to install before (and
//! after) cloning. The project files stay authoritative: every requirement names the file it came from and is
//! re-detected whenever the context is refreshed. Environment variables are recorded by **name** from example files
//! only (`.env.example` …) — never values, and `.env` itself is never read. Nothing here runs anything.

use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_FILE: u64 = 512 * 1024;
const MAX_ENV_VARS: usize = 80;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    /// Tool id from `toolchain::TOOLS`.
    pub tool: String,
    /// The version constraint as written in the source (`>=20`, `^8.2`, `3.12`, `stable`, …); empty = any version.
    pub constraint: String,
    /// The file (and field) it came from.
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Containers {
    pub dockerfile: Option<String>,
    pub compose_file: Option<String>,
    /// Service names from the Compose file (local services the project expects).
    pub compose_services: Vec<String>,
    pub devcontainer: Option<String>,
    pub devcontainer_image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVar {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub nexus: u32,
    pub requirements: Vec<Requirement>,
    pub containers: Containers,
    pub env_vars: Vec<EnvVar>,
    /// README section that explains the setup (a pointer, not a copy).
    pub setup_section: Option<String>,
    /// Commands from the manifests (`kind`, command, source) — shown, never run.
    pub commands: Vec<super::context::CommandRef>,
}

fn read(root: &Path, rel: &str) -> Option<String> {
    let p = root.join(rel);
    let m = std::fs::metadata(&p).ok()?;
    (m.is_file() && m.len() <= MAX_FILE).then(|| std::fs::read_to_string(&p).ok()).flatten()
}

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#')).unwrap_or("").chars().filter(|c| !c.is_control()).take(60).collect()
}

/// `key = "value"` / `key: value` in a simple TOML/YAML line (first match), without quotes.
fn simple_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let t = l.trim();
        let rest = t.strip_prefix(key)?.trim_start();
        let rest = rest.strip_prefix('=').or_else(|| rest.strip_prefix(':'))?;
        let v = rest.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        (!v.is_empty()).then(|| v.chars().take(60).collect())
    })
}

/// Tool id for an asdf / mise plugin name.
fn asdf_tool(name: &str) -> Option<&'static str> {
    Some(match name {
        "nodejs" | "node" => "node",
        "python" => "python",
        "rust" => "rustc",
        "php" => "php",
        "golang" | "go" => "go",
        "flutter" => "flutter",
        "dart" => "dart",
        "java" => "java",
        "ruby" => "ruby",
        "deno" => "deno",
        "bun" => "bun",
        "terraform" => "terraform",
        "kubectl" => "kubectl",
        "helm" => "helm",
        "dotnet" | "dotnet-core" => "dotnet",
        _ => return None,
    })
}

/// The first file directly in `root` with one of these extensions (sorted, so the answer is stable).
fn top_level_with(root: &Path, extensions: &[&str]) -> Option<String> {
    let mut names: Vec<String> = std::fs::read_dir(root)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| extensions.iter().any(|x| n.to_ascii_lowercase().ends_with(x)))
        .collect();
    names.sort();
    names.into_iter().next()
}

fn push(reqs: &mut Vec<Requirement>, tool: &str, constraint: &str, source: &str) {
    if reqs.iter().any(|r| r.tool == tool && r.source == source) {
        return;
    }
    reqs.push(Requirement { tool: tool.into(), constraint: constraint.trim().chars().take(60).collect(), source: source.into() });
}

pub fn detect(root: &Path) -> Environment {
    let mut env = Environment { nexus: 1, ..Default::default() };
    let reqs = &mut env.requirements;

    // Node.js and its package manager.
    if let Some(v) = read(root, ".nvmrc") {
        push(reqs, "node", &first_line(&v), ".nvmrc");
    }
    if let Some(v) = read(root, ".node-version") {
        push(reqs, "node", &first_line(&v), ".node-version");
    }
    if let Some(pkg) = read(root, "package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        if let Some(v) = pkg.pointer("/engines/node").and_then(|v| v.as_str()) {
            push(reqs, "node", v, "package.json engines.node");
        }
        if let Some(v) = pkg.pointer("/volta/node").and_then(|v| v.as_str()) {
            push(reqs, "node", v, "package.json volta.node");
        }
        if let Some(pm) = pkg.get("packageManager").and_then(|v| v.as_str())
            && let Some((name, version)) = pm.split_once('@')
            && ["npm", "pnpm", "yarn", "bun"].contains(&name)
        {
            push(reqs, name, version.split('+').next().unwrap_or(version), "package.json packageManager");
        }
        if !reqs.iter().any(|r| r.tool == "node") {
            push(reqs, "node", "", "package.json");
        }
    }
    for (lock, tool) in [("pnpm-lock.yaml", "pnpm"), ("yarn.lock", "yarn"), ("bun.lockb", "bun"), ("bun.lock", "bun"), ("package-lock.json", "npm")] {
        if root.join(lock).is_file() && !reqs.iter().any(|r| r.tool == tool) {
            push(reqs, tool, "", lock);
        }
    }
    // asdf / mise .tool-versions
    if let Some(t) = read(root, ".tool-versions") {
        for l in t.lines() {
            let mut parts = l.split_whitespace();
            if let (Some(name), Some(v)) = (parts.next(), parts.next()) {
                let Some(tool) = asdf_tool(name) else { continue };
                push(reqs, tool, v, ".tool-versions");
            }
        }
    }
    // mise: `[tools]` in mise.toml / .mise.toml (`node = "20"`, `python = ["3.12"]`).
    for f in ["mise.toml", ".mise.toml"] {
        if let Some(t) = read(root, f) {
            let mut in_tools = false;
            for l in t.lines().map(str::trim) {
                if l.starts_with('[') {
                    in_tools = l == "[tools]";
                    continue;
                }
                if let (true, Some((name, v))) = (in_tools, l.split_once('=')) {
                    let name = name.trim().trim_matches('"');
                    let v = v.trim().trim_start_matches('[').trim_end_matches(']').split(',').next().unwrap_or("").trim().trim_matches(['"', '\'']);
                    if let Some(tool) = asdf_tool(name) {
                        push(reqs, tool, v, f);
                    }
                }
            }
        }
    }
    // Python
    if let Some(v) = read(root, ".python-version") {
        push(reqs, "python", &first_line(&v), ".python-version");
    }
    if let Some(t) = read(root, "pyproject.toml") {
        match simple_value(&t, "requires-python") {
            Some(v) => push(reqs, "python", &v, "pyproject.toml requires-python"),
            None => push(reqs, "python", "", "pyproject.toml"),
        }
        if root.join("uv.lock").is_file() {
            push(reqs, "uv", "", "uv.lock");
        } else if root.join("poetry.lock").is_file() {
            push(reqs, "poetry", "", "poetry.lock");
        }
    } else if root.join("requirements.txt").is_file() {
        push(reqs, "python", "", "requirements.txt");
    }
    // Rust
    if let Some(t) = read(root, "rust-toolchain.toml") {
        push(reqs, "rustc", &simple_value(&t, "channel").unwrap_or_default(), "rust-toolchain.toml channel");
    } else if let Some(t) = read(root, "rust-toolchain") {
        push(reqs, "rustc", &first_line(&t), "rust-toolchain");
    }
    if let Some(t) = read(root, "Cargo.toml") {
        push(reqs, "cargo", "", "Cargo.toml");
        if let Some(v) = simple_value(&t, "rust-version") {
            push(reqs, "rustc", &format!(">={v}"), "Cargo.toml rust-version");
        }
    }
    // PHP
    if let Some(c) = read(root, "composer.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        push(reqs, "composer", "", "composer.json");
        if let Some(v) = c.pointer("/require/php").and_then(|v| v.as_str()) {
            push(reqs, "php", v, "composer.json require.php");
        } else {
            push(reqs, "php", "", "composer.json");
        }
    }
    // Dart / Flutter
    if let Some(t) = read(root, "pubspec.yaml") {
        let is_flutter = t.contains("sdk: flutter");
        if let Some(sdk) = t.lines().skip_while(|l| l.trim() != "environment:").skip(1).take_while(|l| l.starts_with(' ')).find_map(|l| {
            let l = l.trim();
            l.strip_prefix("sdk:").map(|v| v.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
        }) {
            push(reqs, "dart", &sdk, "pubspec.yaml environment.sdk");
        }
        if is_flutter {
            push(reqs, "flutter", "", "pubspec.yaml");
        }
    }
    // Go
    if let Some(t) = read(root, "go.mod") {
        let v = t.lines().find_map(|l| l.trim().strip_prefix("go ").map(|v| format!(">={}", v.trim()))).unwrap_or_default();
        push(reqs, "go", &v, "go.mod");
    }
    // Java: the version file, then Maven/Gradle builds (a wrapper script brings its own Maven/Gradle).
    if let Some(v) = read(root, ".java-version") {
        push(reqs, "java", &first_line(&v), ".java-version");
    }
    if root.join("pom.xml").is_file() {
        push(reqs, "java", "", "pom.xml");
        if !root.join("mvnw").is_file() && !root.join("mvnw.cmd").is_file() {
            push(reqs, "maven", "", "pom.xml");
        }
    }
    if let Some(f) = ["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"].iter().find(|f| root.join(f).is_file()) {
        push(reqs, "java", "", f);
        if !root.join("gradlew").is_file() && !root.join("gradlew.bat").is_file() {
            push(reqs, "gradle", "", f);
        }
    }
    // .NET: global.json pins an SDK; otherwise a solution or project file at the top. `dotnet --version` reports one
    // SDK of several, so the pinned version is named in the source instead of being compared.
    if let Some(g) = read(root, "global.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) {
        let pinned = g.pointer("/sdk/version").and_then(|v| v.as_str()).map(|v| format!("global.json sdk.version {}", v.chars().take(20).collect::<String>()));
        push(reqs, "dotnet", "", pinned.as_deref().unwrap_or("global.json"));
    } else if let Some(f) = top_level_with(root, &[".sln", ".slnx", ".csproj", ".fsproj", ".vbproj"]) {
        push(reqs, "dotnet", "", &f);
    }
    // Ruby
    if let Some(v) = read(root, ".ruby-version") {
        push(reqs, "ruby", first_line(&v).trim_start_matches("ruby-"), ".ruby-version");
    }
    if root.join("Gemfile").is_file() {
        push(reqs, "ruby", "", "Gemfile");
        push(reqs, "bundler", "", "Gemfile");
    }
    // Deno
    if let Some(f) = ["deno.json", "deno.jsonc"].iter().find(|f| root.join(f).is_file()) {
        push(reqs, "deno", "", f);
    }
    // Build tools and infrastructure the repository drives.
    if let Some(f) = ["Makefile", "GNUmakefile"].iter().find(|f| root.join(f).is_file()) {
        push(reqs, "make", "", f);
    }
    if root.join("CMakeLists.txt").is_file() {
        push(reqs, "cmake", "", "CMakeLists.txt");
    }
    if let Some(v) = read(root, ".terraform-version") {
        push(reqs, "terraform", &first_line(&v), ".terraform-version");
    } else if let Some(f) = top_level_with(root, &[".tf"]) {
        push(reqs, "terraform", "", &f);
    }
    if root.join("Chart.yaml").is_file() {
        push(reqs, "helm", "", "Chart.yaml");
    }
    // Only catalog ids are recorded (they are what another computer looks up).
    reqs.retain(|r| crate::toolchain::tool(&r.tool).is_some());

    // Containers
    let c = &mut env.containers;
    c.dockerfile = ["Dockerfile", "Containerfile"].iter().find(|f| root.join(f).is_file()).map(|f| f.to_string());
    c.compose_file = ["compose.yaml", "compose.yml", "docker-compose.yml", "docker-compose.yaml"].iter().find(|f| root.join(f).is_file()).map(|f| f.to_string());
    if let Some(f) = &c.compose_file
        && let Some(t) = read(root, f)
    {
        c.compose_services = compose_services(&t);
    }
    for dc in [".devcontainer/devcontainer.json", ".devcontainer.json"] {
        if let Some(t) = read(root, dc) {
            c.devcontainer = Some(dc.to_string());
            // devcontainer.json allows comments; read the image value from its line.
            c.devcontainer_image = t.lines().find_map(|l| {
                let l = l.trim();
                l.strip_prefix("\"image\"").and_then(|r| r.trim_start().strip_prefix(':')).map(|v| v.trim().trim_end_matches(',').trim_matches('"').chars().take(120).collect())
            });
            break;
        }
    }
    if c.dockerfile.is_some() || c.compose_file.is_some() || c.devcontainer.is_some() {
        let source = c.compose_file.clone().or(c.dockerfile.clone()).or(c.devcontainer.clone()).unwrap_or_default();
        push(reqs, "docker", "", &source);
    }

    // Environment variable names from example files only.
    for f in [".env.example", ".env.sample", ".env.template", ".env.dist", "example.env"] {
        if let Some(t) = read(root, f) {
            for l in t.lines() {
                let l = l.trim().trim_start_matches("export ").trim();
                if l.starts_with('#') {
                    continue;
                }
                if let Some((name, _value_never_read)) = l.split_once('=') {
                    let name = name.trim();
                    if !name.is_empty() && name.len() <= 80 && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && !env.env_vars.iter().any(|v| v.name == name) {
                        env.env_vars.push(EnvVar { name: name.into(), source: f.into() });
                    }
                }
                if env.env_vars.len() >= MAX_ENV_VARS {
                    break;
                }
            }
        }
    }
    // Commands by reference (the manifests stay authoritative).
    let stack = super::detect::detect(root);
    env.commands = stack
        .scripts
        .iter()
        .map(|s| super::context::CommandRef { kind: s.kind.clone(), command: s.command.clone(), source: super::context::source_of(&s.kind, &stack.stacks) })
        .collect();
    // Where the README explains setup.
    if let Some(t) = read(root, "README.md") {
        env.setup_section = t.lines().filter_map(|l| l.trim_start().strip_prefix("## ").or_else(|| l.trim_start().strip_prefix("### "))).map(str::trim).find(|h| {
            let h = h.to_lowercase();
            ["getting started", "setup", "set up", "installation", "install", "development", "local development", "quick start", "quickstart"].iter().any(|k| h.starts_with(k))
        }).map(|h| h.chars().take(80).collect());
    }
    env
}

// ---- checking this computer ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    pub tool: String,
    pub name: String,
    pub constraint: String,
    pub source: String,
    pub installed: Option<String>,
    pub found_at: Option<String>,
    /// `ok`, `missing`, `mismatch` (installed but outside the constraint) or `unknown` (cannot be compared).
    pub status: &'static str,
    pub install_url: Option<String>,
    /// Official package-manager command for this platform, for the person to run (never run by NEXUS).
    pub install_command: Option<Vec<String>>,
}

/// Whether what is installed fits a constraint: `missing` (`None`), `ok`, `mismatch`, or `unknown` when the version
/// could not be read or the constraint cannot be evaluated (a channel such as `stable` — never guessed).
pub fn fit(installed: Option<Option<&str>>, constraint: &str) -> &'static str {
    match installed {
        None => "missing",
        Some(version) => match version.map(|v| crate::toolchain::satisfies(v, constraint)) {
            Some(Some(true)) => "ok",
            Some(Some(false)) => "mismatch",
            _ if constraint.is_empty() => "ok",
            _ => "unknown",
        },
    }
}

/// Compare the requirements with what `probe` finds on this computer (one probe per tool).
pub fn check_with(env: &Environment, probe: &mut dyn FnMut(&str) -> Option<crate::toolchain::Probe>) -> Vec<CheckItem> {
    let mut cache: std::collections::HashMap<String, Option<crate::toolchain::Probe>> = Default::default();
    env.requirements
        .iter()
        .map(|r| {
            let t = crate::toolchain::tool(&r.tool);
            let found = cache.entry(r.tool.clone()).or_insert_with(|| if t.is_some() { probe(&r.tool) } else { None }).clone();
            let status = if t.is_none() { "unknown" } else { fit(found.as_ref().map(|p| p.version.as_deref()), &r.constraint) };
            CheckItem {
                tool: r.tool.clone(),
                name: t.map(|t| t.name.to_string()).unwrap_or_else(|| r.tool.clone()),
                constraint: r.constraint.clone(),
                source: r.source.clone(),
                installed: found.as_ref().and_then(|p| p.version.clone()),
                found_at: found.as_ref().map(|p| p.path.display().to_string()),
                status,
                install_url: t.map(|t| t.install_url.to_string()),
                install_command: if status == "missing" { crate::toolchain::install_command(&r.tool) } else { None },
            }
        })
        .collect()
}

/// [`check_with`] against this computer.
pub fn check(env: &Environment) -> Vec<CheckItem> {
    check_with(env, &mut |id| crate::toolchain::probe(id))
}

pub const ENVIRONMENT_FILE: &str = "environment.json";

/// The project's environment: detected from its files when the project is on this computer (authoritative), otherwise
/// the record in the workspace (`environment.json`, from the computer that last refreshed it).
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub environment: Environment,
    /// `repository` or `workspace record`.
    pub from: &'static str,
    pub checks: Vec<CheckItem>,
}

pub fn report(ws: &crate::workspace::Workspace, p: &super::Project, probe: bool) -> Option<Report> {
    let (environment, from) = match p.local_path.as_deref().filter(|d| d.is_dir()) {
        Some(d) => (detect(d), "repository"),
        None => {
            let dir = super::context::context_dir(ws, &p.id)?;
            let text = std::fs::read_to_string(dir.join(ENVIRONMENT_FILE)).ok()?;
            (serde_json::from_str(&text).ok()?, "workspace record")
        }
    };
    let checks = if probe { check(&environment) } else { Vec::new() };
    Some(Report { environment, from, checks })
}

/// Top-level service names of a Compose file (`services:` block, two-space or tab indented keys).
fn compose_services(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_services = false;
    let mut indent: Option<usize> = None;
    for line in text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let lead = line.len() - line.trim_start().len();
        if lead == 0 {
            in_services = line.trim_end() == "services:";
            indent = None;
            continue;
        }
        if !in_services {
            continue;
        }
        let want = *indent.get_or_insert(lead);
        if lead == want
            && let Some(name) = line.trim().strip_suffix(':')
            && !name.is_empty()
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
            out.push(name.to_string());
            if out.len() >= 40 {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn requirements_come_from_the_projects_own_files_with_their_source() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        write(r, "package.json", r#"{"engines":{"node":">=20.11"},"packageManager":"pnpm@9.12.0+sha512.abc","dependencies":{"vue":"3"}}"#);
        write(r, ".nvmrc", "20.11.1\n");
        write(r, "pyproject.toml", "[project]\nname = \"x\"\nrequires-python = \">=3.11,<3.14\"\n");
        write(r, "uv.lock", "");
        write(r, "Cargo.toml", "[package]\nname = \"x\"\nrust-version = \"1.85\"\n");
        write(r, "composer.json", r#"{"require":{"php":"^8.2"}}"#);
        write(r, "compose.yaml", "services:\n  db:\n    image: postgres:16\n  cache:\n    image: redis\nvolumes:\n  data: {}\n");
        write(r, ".devcontainer/devcontainer.json", "// comment\n{\n  \"image\": \"mcr.microsoft.com/devcontainers/typescript-node:20\",\n}\n");
        // A credential-shaped example value, built at runtime (no such literal in the source).
        let url = format!("postgres://{}:{}@localhost/db", "user", ["pa", "ss"].concat());
        write(r, ".env.example", &format!("# comment\nDATABASE_URL={url}\nexport STRIPE_KEY=sk_test_x\nlowercase=no\n"));
        write(r, ".env", "SECRET_FROM_REAL_ENV=never-read\n");
        write(r, "README.md", "# X\n\n## Getting started\n\nRun it.\n");
        let e = detect(r);
        let find = |tool: &str, source: &str| e.requirements.iter().find(|q| q.tool == tool && q.source == source).map(|q| q.constraint.clone());
        assert_eq!(find("node", "package.json engines.node").as_deref(), Some(">=20.11"));
        assert_eq!(find("node", ".nvmrc").as_deref(), Some("20.11.1"));
        assert_eq!(find("pnpm", "package.json packageManager").as_deref(), Some("9.12.0"));
        assert_eq!(find("python", "pyproject.toml requires-python").as_deref(), Some(">=3.11,<3.14"));
        assert_eq!(find("uv", "uv.lock").as_deref(), Some(""));
        assert_eq!(find("rustc", "Cargo.toml rust-version").as_deref(), Some(">=1.85"));
        assert_eq!(find("php", "composer.json require.php").as_deref(), Some("^8.2"));
        assert!(e.requirements.iter().any(|q| q.tool == "docker"));
        assert_eq!(e.containers.compose_services, ["db", "cache"]);
        assert_eq!(e.containers.devcontainer_image.as_deref(), Some("mcr.microsoft.com/devcontainers/typescript-node:20"));
        assert_eq!(e.env_vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["DATABASE_URL", "STRIPE_KEY"]);
        let json = serde_json::to_string(&e).unwrap();
        assert!(!json.contains("postgres://") && !json.contains("sk_test") && !json.contains("SECRET_FROM_REAL_ENV"), "names only, never values, never .env: {json}");
        assert_eq!(e.setup_section.as_deref(), Some("Getting started"));
    }

    #[test]
    fn checks_compare_this_computer_with_the_requirements() {
        let env = Environment {
            nexus: 1,
            requirements: vec![
                Requirement { tool: "node".into(), constraint: ">=20".into(), source: ".nvmrc".into() },
                Requirement { tool: "node".into(), constraint: "".into(), source: "package.json".into() },
                Requirement { tool: "php".into(), constraint: "^8.2".into(), source: "composer.json require.php".into() },
                Requirement { tool: "rustc".into(), constraint: "stable".into(), source: "rust-toolchain.toml channel".into() },
                Requirement { tool: "made-up".into(), constraint: "".into(), source: "x".into() },
            ],
            ..Default::default()
        };
        let mut probes = Vec::new();
        let items = check_with(&env, &mut |id| {
            probes.push(id.to_string());
            match id {
                "node" => Some(crate::toolchain::Probe { path: "node".into(), version: Some("18.19.0".into()) }),
                "rustc" => Some(crate::toolchain::Probe { path: "rustc".into(), version: Some("1.90.0".into()) }),
                _ => None,
            }
        });
        let st: Vec<(&str, &str)> = items.iter().map(|i| (i.tool.as_str(), i.status)).collect();
        assert_eq!(st, [("node", "mismatch"), ("node", "ok"), ("php", "missing"), ("rustc", "unknown"), ("made-up", "unknown")]);
        assert_eq!(probes, ["node", "php", "rustc"], "one probe per known tool; unknown ids are never probed");
        assert!(items[2].install_url.is_some());
    }

    #[test]
    fn more_ecosystems_are_recognised_from_their_files() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        write(r, ".java-version", "21\n");
        write(r, "pom.xml", "<project/>");
        write(r, "build.gradle.kts", "plugins {}");
        write(r, "gradlew", "#!/bin/sh");
        write(r, "global.json", r#"{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}"#);
        write(r, ".ruby-version", "ruby-3.3.5\n");
        write(r, "Gemfile", "source 'https://rubygems.org'");
        write(r, "deno.jsonc", "{}");
        write(r, "Makefile", "all:");
        write(r, "CMakeLists.txt", "project(x)");
        write(r, "main.tf", "terraform {}");
        write(r, "Chart.yaml", "name: x");
        write(r, ".tool-versions", "kubectl 1.31.1\nunknown-plugin 1\n");
        write(r, "mise.toml", "[env]\nNODE = \"x\"\n[tools]\nnode = \"22\"\npython = [\"3.12\", \"3.11\"]\nsomething = \"1\"\n");
        let e = detect(r);
        let find = |tool: &str, source: &str| e.requirements.iter().find(|q| q.tool == tool && q.source == source).map(|q| q.constraint.clone());
        assert_eq!(find("java", ".java-version").as_deref(), Some("21"));
        assert_eq!(find("maven", "pom.xml").as_deref(), Some(""), "no Maven wrapper: Maven itself is needed");
        assert!(!e.requirements.iter().any(|q| q.tool == "gradle"), "the Gradle wrapper brings its own Gradle");
        assert_eq!(find("dotnet", "global.json sdk.version 8.0.100").as_deref(), Some(""), "the pinned SDK is named, not compared");
        assert_eq!(find("ruby", ".ruby-version").as_deref(), Some("3.3.5"));
        assert!(find("bundler", "Gemfile").is_some() && find("deno", "deno.jsonc").is_some());
        assert!(find("make", "Makefile").is_some() && find("cmake", "CMakeLists.txt").is_some());
        assert!(find("terraform", "main.tf").is_some() && find("helm", "Chart.yaml").is_some());
        assert_eq!(find("kubectl", ".tool-versions").as_deref(), Some("1.31.1"));
        assert_eq!(find("node", "mise.toml").as_deref(), Some("22"));
        assert_eq!(find("python", "mise.toml").as_deref(), Some("3.12"), "the first of several versions");
        assert!(e.requirements.iter().all(|q| crate::toolchain::tool(&q.tool).is_some()), "catalog ids only");

        let t2 = tempfile::tempdir().unwrap();
        write(t2.path(), "App.csproj", "<Project/>");
        assert_eq!(detect(t2.path()).requirements.iter().map(|q| (q.tool.as_str(), q.source.as_str())).collect::<Vec<_>>(), [("dotnet", "App.csproj")]);
    }

    #[test]
    fn fit_reports_what_it_can_and_never_guesses() {
        assert_eq!(fit(None, ">=20"), "missing");
        assert_eq!(fit(Some(Some("20.11.1")), ">=20"), "ok");
        assert_eq!(fit(Some(Some("18.0.0")), ">=20"), "mismatch");
        assert_eq!(fit(Some(None), ""), "ok", "installed, any version");
        assert_eq!(fit(Some(None), ">=20"), "unknown", "version unreadable");
        assert_eq!(fit(Some(Some("1.90.0")), "stable"), "unknown");
    }
}
