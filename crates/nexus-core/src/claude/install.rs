//! Discovering Claude Code executables.
//!
//! Only native executables are launchable. npm `.cmd`/`.ps1` shims would force argument parsing through
//! `cmd.exe`/PowerShell, re-introducing shell injection (threat T1); they are reported with a fix instead.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;

use crate::env::{self, Policy};
use crate::error::{CoreError, Result};
use crate::proc;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub path: PathBuf,
    pub version: Option<String>,
    pub source: InstallSource,
    pub launchable: bool,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstallSource {
    Path,
    NativeInstaller,
    Npm,
    Homebrew,
    Custom,
}

const SHIM_FIX: &str = "This is a package-manager shim, which NEXUS does not launch (arguments would pass \
                        through a shell). Run `claude install` to add the native build, or select claude.exe.";

/// Is this path something NEXUS may spawn directly?
pub fn launchability(path: &Path) -> (bool, Option<String>) {
    native_launchability(path, SHIM_FIX)
}

/// The same rule for any agent's executable: native programs only; `shim_fix` explains what to do about a shim.
pub fn native_launchability(path: &Path, shim_fix: &str) -> (bool, Option<String>) {
    if !path.is_file() {
        return (false, Some("file not found".into()));
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("cmd" | "bat" | "ps1") => (false, Some(shim_fix.into())),
        Some("exe") | Some("com") => (true, None),
        None if !cfg!(windows) => (true, None),
        _ if cfg!(windows) => (false, Some("not a Windows executable".into())),
        _ => (true, None),
    }
}

fn candidates(home: &Path) -> Vec<(PathBuf, InstallSource)> {
    let mut out: Vec<(PathBuf, InstallSource)> = Vec::new();
    if let Ok(found) = which::which_all("claude") {
        out.extend(found.map(|p| (p, InstallSource::Path)));
    }
    let exe = if cfg!(windows) { "claude.exe" } else { "claude" };
    out.push((home.join(".local").join("bin").join(exe), InstallSource::NativeInstaller));
    out.push((home.join(".claude").join("local").join(exe), InstallSource::NativeInstaller));
    if cfg!(windows) {
        if let Some(appdata) = dirs::data_dir() {
            out.push((appdata.join("npm").join("claude.cmd"), InstallSource::Npm));
        }
    } else {
        out.push((PathBuf::from("/opt/homebrew/bin/claude"), InstallSource::Homebrew));
        out.push((PathBuf::from("/usr/local/bin/claude"), InstallSource::Path));
        out.push((home.join(".npm-global").join("bin").join("claude"), InstallSource::Npm));
    }
    out
}

/// Classify a PATH hit more precisely by where it lives.
fn refine_source(path: &Path, source: InstallSource, home: &Path) -> InstallSource {
    let s = path.to_string_lossy().to_lowercase();
    if s.contains("node_modules") || s.contains(&format!("{}npm{}", std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR)) {
        return InstallSource::Npm;
    }
    if s.contains("homebrew") {
        return InstallSource::Homebrew;
    }
    if path.starts_with(home.join(".local")) || path.starts_with(home.join(".claude")) {
        return InstallSource::NativeInstaller;
    }
    source
}

/// All distinct Claude executables on this machine, with versions (queried in parallel).
pub fn discover(home: &Path) -> Vec<Installation> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut unique = Vec::new();
    for (path, source) in candidates(home) {
        if !path.is_file() {
            continue;
        }
        let canon = dunce::canonicalize(&path).unwrap_or(path.clone());
        if seen.iter().any(|s| crate::paths::same_path(s, &canon)) {
            continue;
        }
        seen.push(canon.clone());
        unique.push((canon.clone(), refine_source(&canon, source, home)));
    }
    let handles: Vec<_> = unique
        .into_iter()
        .map(|(path, source)| std::thread::spawn(move || inspect(&path, source)))
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

/// Inspect a single executable (used for custom executables too).
pub fn inspect(path: &Path, source: InstallSource) -> Installation {
    let (launchable, issue) = launchability(path);
    Installation {
        version: if launchable { version_of(path) } else { None },
        path: path.to_path_buf(),
        source,
        launchable,
        issue,
    }
}

/// `claude --version` → "2.1.311".
pub fn version_of(path: &Path) -> Option<String> {
    version_with_env(path, &env::current())
}

/// `claude --version` with an explicit parent environment (scrubbed with the Claude policy before use).
pub fn version_with_env(path: &Path, base_env: &[(String, String)]) -> Option<String> {
    let mut cmd = proc::command(path);
    cmd.arg("--version");
    let (env, _) = env::compose(base_env, &[], Policy::CLAUDE);
    env::apply(&mut cmd, &env);
    let out = proc::run(cmd, Duration::from_secs(15)).ok()?;
    if !out.success() {
        return None;
    }
    parse_version(&out.stdout_str())
}

pub fn parse_version(s: &str) -> Option<String> {
    let first = s.split_whitespace().next()?;
    first.chars().next()?.is_ascii_digit().then(|| first.to_string())
}

/// Resolve the executable for a profile: its explicit override, else the first launchable discovery hit.
pub fn resolve(profile_executable: Option<&Path>, home: &Path) -> Result<PathBuf> {
    if let Some(p) = profile_executable {
        let (ok, issue) = launchability(p);
        if !ok {
            return Err(CoreError::invalid(format!(
                "Claude executable {} cannot be launched: {}",
                p.display(),
                issue.unwrap_or_default()
            )));
        }
        return Ok(p.to_path_buf());
    }
    for (path, _) in candidates(home) {
        if launchability(&path).0 {
            return Ok(path);
        }
    }
    Err(CoreError::NotFound(
        "Claude Code was not found. Install it (https://code.claude.com) or set the executable on the profile.".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(parse_version("2.1.311 (Claude Code)\n").as_deref(), Some("2.1.311"));
        assert_eq!(parse_version("error: nope"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn shims_are_not_launchable() {
        let t = tempfile::tempdir().unwrap();
        let shim = t.path().join("claude.cmd");
        std::fs::write(&shim, "@echo off").unwrap();
        let (ok, issue) = launchability(&shim);
        assert!(!ok);
        assert!(issue.unwrap().contains("claude install"));
        assert!(!launchability(&t.path().join("missing.exe")).0);
        assert!(resolve(Some(&shim), t.path()).is_err());
    }
}
