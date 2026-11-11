//! Development tools NEXUS knows by name (Workspace Doctor, integration manifests, environment reconstruction, the Dev
//! Environment page — ADR-0051).
//!
//! Workspace files (integration manifests, environment manifests) refer to tools only by these **ids**; program names,
//! version arguments and package-manager ids come from this table, never from a workspace file — a synced file can
//! therefore never make NEXUS look up or run an arbitrary program (threat T22). `find` is a PATH search; `probe` runs
//! the tool's own version command (argv, timeout) and nothing else. Installing is never done here: `install_command`
//! and `version_hint` only *describe* official commands the person can run in a terminal.

use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

/// What a tool is for (the Dev Environment page groups by it, in this order).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    SourceControl,
    Runtime,
    PackageManager,
    VersionManager,
    Build,
    Infrastructure,
    Cloud,
    Database,
    AiAgent,
    Integration,
}

/// A Homebrew package: a formula (`brew install x`) or a cask (`brew install --cask x`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brew {
    Formula(&'static str),
    Cask(&'static str),
}

/// A tool NEXUS can look for.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// Program names tried in order (Windows finds `.exe`/`.cmd` through the PATH search).
    #[serde(skip)]
    pub programs: &'static [&'static str],
    #[serde(skip)]
    pub version_args: &'static [&'static str],
    /// Set for the version command only, so that asking for a version sends no telemetry or update check.
    #[serde(skip)]
    pub probe_env: &'static [(&'static str, &'static str)],
    /// Official installation page.
    pub install_url: &'static str,
    /// WinGet package id (Windows) and Homebrew formula/cask (macOS), when the vendor publishes one — each verified when
    /// it was added (`winget show --id <id> -e`, formulae.brew.sh).
    #[serde(skip)]
    pub winget: Option<&'static str>,
    #[serde(skip)]
    pub brew: Option<Brew>,
    /// Exists only on Windows (nvm for Windows); elsewhere the tool is a shell function NEXUS cannot see.
    #[serde(skip)]
    pub windows_only: bool,
}

const V: &[&str] = &["--version"];

const fn t(id: &'static str, name: &'static str, category: Category, programs: &'static [&'static str], install_url: &'static str) -> Tool {
    Tool { id, name, category, programs, version_args: V, probe_env: &[], install_url, winget: None, brew: None, windows_only: false }
}

impl Tool {
    const fn args(mut self, version_args: &'static [&'static str]) -> Tool {
        self.version_args = version_args;
        self
    }
    const fn env(mut self, probe_env: &'static [(&'static str, &'static str)]) -> Tool {
        self.probe_env = probe_env;
        self
    }
    const fn winget(mut self, id: &'static str) -> Tool {
        self.winget = Some(id);
        self
    }
    const fn formula(mut self, name: &'static str) -> Tool {
        self.brew = Some(Brew::Formula(name));
        self
    }
    const fn cask(mut self, name: &'static str) -> Tool {
        self.brew = Some(Brew::Cask(name));
        self
    }
    const fn windows_only(mut self) -> Tool {
        self.windows_only = true;
        self
    }
    /// Whether the tool exists on the platform NEXUS runs on.
    pub fn on_this_platform(&self) -> bool {
        !self.windows_only || cfg!(windows)
    }
}

use Category::*;

pub const TOOLS: &[Tool] = &[
    // Source control
    t("git", "Git", SourceControl, &["git"], "https://git-scm.com/downloads").winget("Git.Git").formula("git"),
    t("gh", "GitHub CLI", SourceControl, &["gh"], "https://cli.github.com").winget("GitHub.cli").formula("gh"),
    // Runtimes and SDKs
    t("node", "Node.js", Runtime, &["node"], "https://nodejs.org/en/download").winget("OpenJS.NodeJS.LTS").formula("node"),
    t("deno", "Deno", Runtime, &["deno"], "https://docs.deno.com/runtime/getting_started/installation/").winget("DenoLand.Deno").formula("deno"),
    t("bun", "Bun", Runtime, &["bun"], "https://bun.sh/docs/installation").winget("Oven-sh.Bun"),
    t("python", "Python", Runtime, &["python3", "python", "py"], "https://www.python.org/downloads/").winget("Python.Python.3.13").formula("python"),
    t("rustc", "Rust", Runtime, &["rustc"], "https://www.rust-lang.org/tools/install").winget("Rustlang.Rustup").formula("rustup"),
    t("go", "Go", Runtime, &["go"], "https://go.dev/doc/install").args(&["version"]).winget("GoLang.Go").formula("go"),
    t("java", "Java (JDK)", Runtime, &["java"], "https://adoptium.net/temurin/releases/").args(&["-version"]).winget("EclipseAdoptium.Temurin.21.JDK").cask("temurin"),
    t("dotnet", ".NET SDK", Runtime, &["dotnet"], "https://dotnet.microsoft.com/download")
        .env(&[("DOTNET_CLI_TELEMETRY_OPTOUT", "1"), ("DOTNET_NOLOGO", "1"), ("DOTNET_SKIP_FIRST_TIME_EXPERIENCE", "1")])
        .winget("Microsoft.DotNet.SDK.10")
        .cask("dotnet-sdk"),
    t("php", "PHP", Runtime, &["php"], "https://www.php.net/downloads.php").winget("PHP.PHP.8.4").formula("php"),
    t("ruby", "Ruby", Runtime, &["ruby"], "https://www.ruby-lang.org/en/documentation/installation/").winget("RubyInstallerTeam.Ruby.3.4").formula("ruby"),
    t("dart", "Dart", Runtime, &["dart"], "https://dart.dev/get-dart"),
    t("flutter", "Flutter", Runtime, &["flutter"], "https://docs.flutter.dev/get-started/install"),
    // Package managers
    t("npm", "npm", PackageManager, &["npm"], "https://nodejs.org/en/download"),
    t("npx", "npx", PackageManager, &["npx"], "https://nodejs.org/en/download"),
    t("pnpm", "pnpm", PackageManager, &["pnpm"], "https://pnpm.io/installation").winget("pnpm.pnpm").formula("pnpm"),
    t("yarn", "Yarn", PackageManager, &["yarn"], "https://yarnpkg.com/getting-started/install"),
    t("uv", "uv", PackageManager, &["uv"], "https://docs.astral.sh/uv/getting-started/installation/").winget("astral-sh.uv").formula("uv"),
    t("pipx", "pipx", PackageManager, &["pipx"], "https://pipx.pypa.io/stable/installation/").formula("pipx"),
    t("poetry", "Poetry", PackageManager, &["poetry"], "https://python-poetry.org/docs/#installation"),
    t("cargo", "Cargo", PackageManager, &["cargo"], "https://www.rust-lang.org/tools/install").winget("Rustlang.Rustup").formula("rustup"),
    t("composer", "Composer", PackageManager, &["composer"], "https://getcomposer.org/download/").formula("composer"),
    t("bundler", "Bundler", PackageManager, &["bundle"], "https://bundler.io/"),
    t("maven", "Maven", PackageManager, &["mvn"], "https://maven.apache.org/install.html").args(&["-v"]).formula("maven"),
    t("gradle", "Gradle", PackageManager, &["gradle"], "https://gradle.org/install/").formula("gradle"),
    // Version managers (exact versions of a runtime per project — ADR-0051 §6)
    t("fnm", "fnm", VersionManager, &["fnm"], "https://github.com/Schniz/fnm#installation").winget("Schniz.fnm").formula("fnm"),
    t("volta", "Volta", VersionManager, &["volta"], "https://docs.volta.sh/guide/getting-started").winget("Volta.Volta").formula("volta"),
    t("nvm", "nvm for Windows", VersionManager, &["nvm"], "https://github.com/coreybutler/nvm-windows#installation--upgrades").args(&["version"]).winget("CoreyButler.NVMforWindows").windows_only(),
    t("pyenv", "pyenv", VersionManager, &["pyenv"], "https://github.com/pyenv/pyenv#installation").formula("pyenv"),
    t("rustup", "rustup", VersionManager, &["rustup"], "https://rustup.rs").winget("Rustlang.Rustup").formula("rustup"),
    t("mise", "mise", VersionManager, &["mise"], "https://mise.jdx.dev/getting-started.html").winget("jdx.mise").formula("mise"),
    // Build tools
    t("make", "GNU Make", Build, &["make"], "https://www.gnu.org/software/make/"),
    t("cmake", "CMake", Build, &["cmake"], "https://cmake.org/download/").winget("Kitware.CMake").formula("cmake"),
    // Containers and infrastructure
    t("docker", "Docker", Infrastructure, &["docker"], "https://docs.docker.com/get-started/get-docker/").winget("Docker.DockerDesktop").cask("docker-desktop"),
    t("kubectl", "kubectl", Infrastructure, &["kubectl"], "https://kubernetes.io/docs/tasks/tools/").args(&["version", "--client"]).winget("Kubernetes.kubectl").formula("kubernetes-cli"),
    t("helm", "Helm", Infrastructure, &["helm"], "https://helm.sh/docs/intro/install/").args(&["version", "--short"]).winget("Helm.Helm").formula("helm"),
    t("terraform", "Terraform", Infrastructure, &["terraform"], "https://developer.hashicorp.com/terraform/install")
        .args(&["version"])
        .env(&[("CHECKPOINT_DISABLE", "1")])
        .winget("Hashicorp.Terraform")
        .formula("hashicorp/tap/terraform"),
    // Cloud
    t("az", "Azure CLI", Cloud, &["az"], "https://learn.microsoft.com/cli/azure/install-azure-cli").args(&["version"]).env(&[("AZURE_CORE_COLLECT_TELEMETRY", "no")]).winget("Microsoft.AzureCLI").formula("azure-cli"),
    t("aws", "AWS CLI", Cloud, &["aws"], "https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html").winget("Amazon.AWSCLI").formula("awscli"),
    t("gcloud", "Google Cloud CLI", Cloud, &["gcloud"], "https://cloud.google.com/sdk/docs/install").winget("Google.CloudSDK").cask("gcloud-cli"),
    // Database clients
    t("psql", "PostgreSQL client (psql)", Database, &["psql"], "https://www.postgresql.org/download/"),
    t("mysql", "MySQL client", Database, &["mysql"], "https://dev.mysql.com/downloads/"),
    t("sqlite", "SQLite", Database, &["sqlite3"], "https://sqlite.org/download.html").winget("SQLite.SQLite").formula("sqlite"),
    t("redis-cli", "Redis CLI", Database, &["redis-cli"], "https://redis.io/docs/latest/operate/oss_and_stack/install/").formula("redis"),
    // AI agents
    t("claude", "Claude Code", AiAgent, &["claude"], "https://code.claude.com/docs/en/setup").winget("Anthropic.ClaudeCode").cask("claude-code"),
    t("codex", "OpenAI Codex CLI", AiAgent, &["codex"], "https://github.com/openai/codex").winget("OpenAI.Codex").cask("codex"),
    t("gemini", "Gemini CLI", AiAgent, &["gemini"], "https://github.com/google-gemini/gemini-cli").formula("gemini-cli"),
    t("copilot", "GitHub Copilot CLI", AiAgent, &["copilot"], "https://github.com/github/copilot-cli").winget("GitHub.Copilot").cask("copilot-cli"),
    t("opencode", "OpenCode", AiAgent, &["opencode"], "https://opencode.ai/docs/").winget("Anomaly.OpenCode").formula("opencode"),
    // Integrations NEXUS offers
    t("graphify", "Graphify", Integration, &["graphify"], "https://github.com/safishamsi/graphify"),
];

pub fn tool(id: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.id == id)
}

/// Where the tool is on this machine's PATH, if anywhere. Unknown ids are never looked up.
pub fn find(id: &str) -> Option<PathBuf> {
    tool(id)?.programs.iter().find_map(|p| which::which(p).ok())
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub path: PathBuf,
    pub version: Option<String>,
}

/// Ask a program for its version (the tool's own version command, argv, 20 s timeout).
fn version_of(t: &Tool, path: &std::path::Path) -> Option<String> {
    let mut cmd = crate::proc::command(path);
    cmd.args(t.version_args);
    for (k, v) in t.probe_env {
        cmd.env(k, v);
    }
    crate::proc::run(cmd, Duration::from_secs(20)).ok().filter(|o| o.success()).and_then(|o| first_version(&format!("{} {}", o.stdout_str(), o.stderr_str())))
}

/// Find the tool and ask it for its version (its own version command, argv, 20 s timeout).
pub fn probe(id: &str) -> Option<Probe> {
    let t = tool(id)?;
    let path = find(id)?;
    let version = version_of(t, &path);
    Some(Probe { path, version })
}

// ---- inventory of this computer (ADR-0051) -------------------------------------------------------------------

/// One tool on this computer: where it is, its version, and every copy on PATH.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    #[serde(flatten)]
    pub tool: Tool,
    /// The copy that runs (first on PATH).
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    /// Every copy of the program on PATH, in PATH order — more than one means the first shadows the others.
    pub copies: Vec<PathBuf>,
    /// Official install command for this platform when the tool is missing (never run by NEXUS).
    pub install_command: Option<Vec<String>>,
    /// Why what is on PATH does not count (the Microsoft Store placeholder for Python).
    pub note: Option<String>,
}

/// The Windows "app execution alias" placeholders (`python3.exe` in `…\Microsoft\WindowsApps`) open the Store instead
/// of running anything; a real Store app's alias in the same folder answers its version command.
fn store_placeholder(path: &std::path::Path) -> bool {
    cfg!(windows) && path.parent().is_some_and(|d| d.ends_with(std::path::Path::new("Microsoft").join("WindowsApps")))
}

/// Look one tool up on this computer (PATH search, version command). Program names are tried in order until one
/// reports a version (`python3` may be a placeholder where `py` is the real launcher).
pub fn installed(t: &Tool) -> Installed {
    let found: Vec<(&str, PathBuf)> = t.programs.iter().filter_map(|p| which::which(p).ok().map(|f| (*p, f))).collect();
    let answered = found.iter().find_map(|(program, path)| version_of(t, path).map(|v| (*program, path.clone(), v)));
    let (program, path, version) = match (answered, found.first()) {
        (Some((program, path, v)), _) => (Some(program), Some(path), Some(v)),
        (None, Some((program, path))) => (Some(*program), Some(path.clone()), None),
        (None, None) => (None, None, None),
    };
    let mut copies: Vec<PathBuf> = Vec::new();
    if let (Some(program), Some(first)) = (program, &path) {
        for c in which::which_all(program).into_iter().flatten().chain(std::iter::once(first.clone())) {
            if !copies.iter().any(|x| crate::paths::same_path(x, &c)) {
                copies.push(c);
            }
        }
    }
    let (path, note) = match path {
        Some(p) if version.is_none() && found.iter().all(|(_, f)| store_placeholder(f)) => {
            let note = format!("Only the Microsoft Store placeholder is on PATH ({}), not an installed {}", p.display(), t.name);
            (None, Some(note))
        }
        p => (p, None),
    };
    let install_command = if path.is_none() { install_command(t.id) } else { None };
    Installed { tool: *t, path, version, copies, install_command, note }
}

/// Every tool for this platform, looked up with `look` on a few threads at once (slow CLIs such as `gcloud` would
/// otherwise add up), in catalog order.
pub fn inventory_with(look: &(dyn Fn(&Tool) -> Installed + Sync)) -> Vec<Installed> {
    let tools: Vec<&Tool> = TOOLS.iter().filter(|t| t.on_this_platform()).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let slots: Vec<std::sync::Mutex<Option<Installed>>> = tools.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..8.min(tools.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(t) = tools.get(i) else { break };
                    let found = look(t);
                    if let Ok(mut slot) = slots[i].lock() {
                        *slot = Some(found);
                    }
                }
            });
        }
    });
    slots.into_iter().filter_map(|m| m.into_inner().ok().flatten()).collect()
}

/// [`inventory_with`] against this computer.
pub fn inventory() -> Vec<Installed> {
    inventory_with(&installed)
}

// ---- version-manager hints (ADR-0051 §6) ------------------------------------------------------------------------

/// A version manager's own command that installs the version a project asks for.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Hint {
    /// Tool id of the version manager.
    pub manager: &'static str,
    pub command: Vec<String>,
}

/// A version written in a project file that is safe to type into a terminal: digits and dots only (an optional leading
/// `v` is dropped), at most three parts and 20 characters. Ranges and channels give `None`.
pub fn exact_version(constraint: &str) -> Option<String> {
    let v = constraint.trim().strip_prefix(['v', 'V']).unwrap_or(constraint.trim());
    let parts: Vec<&str> = v.split('.').collect();
    let ok = !v.is_empty() && v.len() <= 20 && parts.len() <= 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    ok.then(|| v.to_string())
}

/// The first installed version manager that can install `constraint` (an exact version or prefix) of `tool`, with its
/// command. `has` says whether a manager is installed. Commands that would change project files (`mise use`, `volta
/// pin`) are never suggested.
pub fn version_hint(tool: &str, constraint: &str, has: &dyn Fn(&str) -> bool) -> Option<Hint> {
    let v = exact_version(constraint)?;
    let candidates: Vec<(&'static str, Vec<String>)> = match tool {
        "node" => vec![
            ("fnm", vec!["fnm".into(), "install".into(), v.clone()]),
            ("volta", vec!["volta".into(), "install".into(), format!("node@{v}")]),
            ("nvm", vec!["nvm".into(), "install".into(), v.clone()]),
            ("mise", vec!["mise".into(), "install".into(), format!("node@{v}")]),
        ],
        "python" => vec![
            ("uv", vec!["uv".into(), "python".into(), "install".into(), v.clone()]),
            ("pyenv", vec!["pyenv".into(), "install".into(), v.clone()]),
            ("mise", vec!["mise".into(), "install".into(), format!("python@{v}")]),
        ],
        "rustc" => vec![("rustup", vec!["rustup".into(), "toolchain".into(), "install".into(), v.clone()])],
        "go" | "java" | "ruby" | "deno" | "bun" | "terraform" => vec![("mise", vec!["mise".into(), "install".into(), format!("{}@{v}", tool)])],
        _ => vec![],
    };
    candidates.into_iter().find(|(m, _)| tool_exists(m) && has(m)).map(|(manager, command)| Hint { manager, command })
}

fn tool_exists(id: &str) -> bool {
    tool(id).is_some_and(Tool::on_this_platform)
}

/// The first dotted version number in a tool's output (`v20.11.1`, `Python 3.12.4`, `go1.22.3`).
pub fn first_version(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            let v = s[start..i].trim_end_matches('.');
            if v.contains('.') {
                return Some(v.to_string());
            }
        }
        i += 1;
    }
    None
}

fn nums(v: &str) -> Vec<u64> {
    v.trim().trim_start_matches(['v', 'V', '=']).split(['.', '-', '+']).map_while(|p| p.parse::<u64>().ok()).collect()
}

fn cmp(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let n = a.len().max(b.len());
    (0..n).map(|i| a.get(i).copied().unwrap_or(0).cmp(&b.get(i).copied().unwrap_or(0))).find(|o| o.is_ne()).unwrap_or(std::cmp::Ordering::Equal)
}

/// Whether an installed version meets a constraint as written in a project file. `None` when the constraint cannot be
/// evaluated (a channel such as `stable`, `lts/*`, an unusual syntax) — reported as "check by hand", never guessed.
/// Supports `20`, `20.x`, `20.*`, `>=18`, `>18 <21`, `>=3.11,<3.14`, `^8.2`, `~1.4`, `~=3.11`, `^8.2 || ^8.3`, `=1.2.3`.
pub fn satisfies(installed: &str, constraint: &str) -> Option<bool> {
    let c = constraint.trim();
    if c.is_empty() || c == "*" {
        return Some(true);
    }
    let have = nums(installed);
    if have.is_empty() {
        return None;
    }
    let mut any_known = false;
    for alt in c.split("||").flat_map(|a| a.split('|')) {
        let mut all = Some(true);
        for comp in alt.split([',', ' ']).map(str::trim).filter(|s| !s.is_empty()) {
            let r = comparator(&have, comp);
            all = match (all, r) {
                (Some(true), Some(x)) => Some(x),
                (Some(false), _) => Some(false),
                _ => None,
            };
        }
        match all {
            Some(true) => return Some(true),
            Some(false) => any_known = true,
            None => {}
        }
    }
    any_known.then_some(false)
}

fn comparator(have: &[u64], comp: &str) -> Option<bool> {
    use std::cmp::Ordering::*;
    let (op, v) = ["~=", ">=", "<=", "==", ">", "<", "=", "^", "~"].iter().find_map(|op| comp.strip_prefix(op).map(|v| (*op, v.trim()))).unwrap_or(("", comp));
    let v = v.trim_end_matches(".x").trim_end_matches(".*");
    let want = nums(v);
    if want.is_empty() {
        return None; // `stable`, `lts/*`, `latest`, …
    }
    let o = cmp(have, &want);
    Some(match op {
        ">=" => o != Less,
        ">" => o == Greater,
        "<=" => o != Greater,
        "<" => o == Less,
        "=" | "==" => have.starts_with(&want) || o == Equal,
        "^" => {
            let mut upper = want.clone();
            let i = upper.iter().position(|x| *x != 0).unwrap_or(upper.len().saturating_sub(1));
            upper.truncate(i + 1);
            upper[i] += 1;
            o != Less && cmp(have, &upper) == Less
        }
        "~" | "~=" => {
            let mut upper = want.clone();
            let keep = if op == "~=" { want.len().saturating_sub(1).max(1) } else if want.len() > 1 { 2 } else { 1 };
            upper.truncate(keep);
            if let Some(last) = upper.last_mut() {
                *last += 1;
            }
            o != Less && cmp(have, &upper) == Less
        }
        // A bare version is a prefix: `20` = any 20.x, `3.12` = any 3.12.x.
        _ => have.starts_with(&want),
    })
}

/// An official package-manager command for installing the tool on this platform (`winget install --id …` on Windows,
/// `brew install …` on macOS), for the person to run in a terminal — NEXUS never runs it.
pub fn install_command(id: &str) -> Option<Vec<String>> {
    let t = tool(id)?;
    if cfg!(windows) {
        t.winget.map(|w| vec!["winget".into(), "install".into(), "--id".into(), w.into(), "-e".into()])
    } else if cfg!(target_os = "macos") {
        t.brew.map(|b| match b {
            Brew::Formula(f) => vec!["brew".into(), "install".into(), f.into()],
            Brew::Cask(c) => vec!["brew".into(), "install".into(), "--cask".into(), c.into()],
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_tools_are_looked_up() {
        assert!(tool("node").is_some());
        assert!(find("rm -rf /").is_none() && tool("cmd").is_none(), "ids, not programs");
        let ids: Vec<&str> = TOOLS.iter().map(|t| t.id).collect();
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn package_ids_are_well_formed() {
        for t in TOOLS {
            assert!(!t.programs.is_empty() && !t.version_args.is_empty(), "{}", t.id);
            assert!(t.install_url.starts_with("https://"), "{}: {}", t.id, t.install_url);
            if let Some(w) = t.winget {
                // `Publisher.Package[.More]` — no spaces, no shell characters.
                assert!(w.contains('.') && w.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-')), "{}: winget {w}", t.id);
            }
            match t.brew {
                Some(Brew::Formula(n) | Brew::Cask(n)) => assert!(!n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '@' | '/' | '.')), "{}: brew {n}", t.id),
                None => {}
            }
        }
    }

    #[test]
    fn the_catalog_is_grouped_and_platform_aware() {
        let cats: std::collections::BTreeSet<Category> = TOOLS.iter().map(|t| t.category).collect();
        assert!(cats.contains(&Category::AiAgent) && cats.contains(&Category::VersionManager) && cats.contains(&Category::Database));
        let nvm = tool("nvm").unwrap();
        assert_eq!(nvm.on_this_platform(), cfg!(windows), "nvm for Windows only on Windows");
        assert!(tool("git").unwrap().on_this_platform());
        assert!(TOOLS.len() >= 50, "{}", TOOLS.len());
        // Ids are what workspace files use; they never contain path or shell characters.
        assert!(TOOLS.iter().all(|t| t.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')));
    }

    #[test]
    fn casks_install_with_the_cask_flag() {
        let cmd = |b: Brew| match b {
            Brew::Formula(f) => vec!["brew".to_string(), "install".into(), f.into()],
            Brew::Cask(c) => vec!["brew".to_string(), "install".into(), "--cask".into(), c.into()],
        };
        assert_eq!(cmd(tool("codex").unwrap().brew.unwrap()), ["brew", "install", "--cask", "codex"]);
        assert_eq!(cmd(tool("helm").unwrap().brew.unwrap()), ["brew", "install", "helm"]);
        if cfg!(target_os = "macos") {
            assert_eq!(install_command("codex").unwrap(), ["brew", "install", "--cask", "codex"]);
        }
        if cfg!(windows) {
            assert_eq!(install_command("codex").unwrap(), ["winget", "install", "--id", "OpenAI.Codex", "-e"]);
            assert_eq!(install_command("make"), None, "no official WinGet package: the page only");
        }
    }

    #[test]
    fn inventory_covers_the_platform_in_catalog_order() {
        let look = |t: &Tool| Installed {
            tool: *t,
            path: (t.id == "git").then(|| PathBuf::from("/usr/bin/git")),
            version: (t.id == "git").then(|| "2.47.0".to_string()),
            copies: vec![],
            install_command: None,
            note: None,
        };
        let inv = inventory_with(&look);
        let expected: Vec<&str> = TOOLS.iter().filter(|t| t.on_this_platform()).map(|t| t.id).collect();
        assert_eq!(inv.iter().map(|i| i.tool.id).collect::<Vec<_>>(), expected);
        assert_eq!(inv.iter().filter(|i| i.path.is_some()).count(), 1);
        let json = serde_json::to_value(&inv[0]).unwrap();
        assert_eq!(json["id"], "git");
        assert_eq!(json["category"], "source-control");
        assert!(json.get("programs").is_none() && json.get("winget").is_none(), "program names stay internal");
    }

    #[test]
    fn the_store_placeholder_is_recognised_by_its_folder() {
        let stub = PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\python3.exe");
        assert_eq!(store_placeholder(&stub), cfg!(windows));
        assert!(!store_placeholder(&PathBuf::from(r"C:\Users\x\AppData\Local\Programs\Python\Python314\python.exe")));
    }

    #[test]
    fn only_exact_versions_become_hints() {
        assert_eq!(exact_version("20").as_deref(), Some("20"));
        assert_eq!(exact_version("v20.11.1").as_deref(), Some("20.11.1"));
        assert_eq!(exact_version(" 3.12 ").as_deref(), Some("3.12"));
        for bad in [">=18", "^8.2", "20.x", "lts/*", "stable", "", "1.2.3.4", "20; rm -rf /", "20 && x", "１２", "20.", ".20", "v"] {
            assert_eq!(exact_version(bad), None, "{bad}");
        }
        assert_eq!(exact_version(&"9".repeat(21)), None, "too long");
    }

    #[test]
    fn hints_use_the_first_installed_manager() {
        let only = |ids: &'static [&'static str]| move |m: &str| ids.contains(&m);
        let h = version_hint("node", "v20.11.1", &only(&["volta", "mise"])).unwrap();
        assert_eq!((h.manager, h.command.clone()), ("volta", vec!["volta".to_string(), "install".into(), "node@20.11.1".into()]));
        let h = version_hint("node", "20", &only(&["fnm", "volta"])).unwrap();
        assert_eq!(h.command, ["fnm", "install", "20"]);
        assert_eq!(version_hint("python", "3.12", &only(&["uv"])).unwrap().command, ["uv", "python", "install", "3.12"]);
        assert_eq!(version_hint("rustc", "1.85.0", &only(&["rustup"])).unwrap().command, ["rustup", "toolchain", "install", "1.85.0"]);
        assert_eq!(version_hint("go", "1.22", &only(&["mise"])).unwrap().command, ["mise", "install", "go@1.22"]);
        assert_eq!(version_hint("node", ">=18", &only(&["fnm"])), None, "ranges get no hint");
        assert_eq!(version_hint("node", "20", &only(&[])), None, "no manager installed");
        assert_eq!(version_hint("php", "8.3", &only(&["mise", "fnm"])), None, "no manager for this tool");
        if !cfg!(windows) {
            assert_eq!(version_hint("node", "20", &only(&["nvm"])), None, "nvm for Windows is not a manager elsewhere");
        }
        // No command ever changes the project's own files.
        for tool in ["node", "python", "rustc", "go", "java", "ruby", "deno", "bun", "terraform"] {
            if let Some(h) = version_hint(tool, "1", &|_| true) {
                assert!(!h.command.iter().any(|a| a == "use" || a == "pin"), "{tool}: {:?}", h.command);
            }
        }
    }

    #[test]
    fn versions_are_read_from_tool_output() {
        assert_eq!(first_version("v20.11.1\n").as_deref(), Some("20.11.1"));
        assert_eq!(first_version("Python 3.12.4").as_deref(), Some("3.12.4"));
        assert_eq!(first_version("go version go1.22.3 windows/amd64").as_deref(), Some("1.22.3"));
        assert_eq!(first_version("rustc 1.90.0 (1159e78c4 2025-09-14)").as_deref(), Some("1.90.0"));
        assert_eq!(first_version("no numbers"), None);
        // Outputs of the tools added for the Dev Environment page (ADR-0051).
        assert_eq!(first_version("openjdk version \"21.0.2\" 2024-01-16").as_deref(), Some("21.0.2"));
        assert_eq!(first_version("Client Version: v1.31.1\nKustomize Version: v5.4.2").as_deref(), Some("1.31.1"));
        assert_eq!(first_version("v3.16.2+g13654a5").as_deref(), Some("3.16.2"));
        assert_eq!(first_version("Terraform v1.9.8\non windows_amd64").as_deref(), Some("1.9.8"));
        assert_eq!(first_version("{\n  \"azure-cli\": \"2.65.0\",\n  \"azure-cli-core\": \"2.65.0\"\n}").as_deref(), Some("2.65.0"));
        assert_eq!(first_version("aws-cli/2.18.4 Python/3.12.6 Windows/11").as_deref(), Some("2.18.4"));
        assert_eq!(first_version("\n------------------------------------------------------------\nGradle 8.10.2\n").as_deref(), Some("8.10.2"));
        assert_eq!(first_version("psql (PostgreSQL) 17.0").as_deref(), Some("17.0"));
    }

    #[test]
    fn constraints_follow_the_ecosystems_rules() {
        let yes = |v: &str, c: &str| assert_eq!(satisfies(v, c), Some(true), "{v} satisfies {c}");
        let no = |v: &str, c: &str| assert_eq!(satisfies(v, c), Some(false), "{v} does not satisfy {c}");
        yes("20.11.1", "20");
        yes("20.11.1", "20.x");
        no("18.19.0", "20");
        yes("20.11.1", ">=18");
        yes("3.12.4", ">=3.11,<3.14");
        no("3.14.0", ">=3.11,<3.14");
        yes("8.3.2", "^8.2");
        no("9.0.0", "^8.2");
        yes("0.3.9", "^0.3.1");
        no("0.4.0", "^0.3.1");
        yes("1.4.9", "~1.4.2");
        no("1.5.0", "~1.4.2");
        yes("3.12.1", "~=3.11");
        yes("8.3.0", "^8.2 || ^9.0");
        yes("1.90.0", ">=1.85");
        no("1.80.0", ">=1.85");
        yes("20.11.1", "v20.11.1");
        assert_eq!(satisfies("1.90.0", "stable"), None, "channels are not guessed");
        assert_eq!(satisfies("20.1.0", "lts/*"), None);
        yes("1.0.0", "");
    }
}
