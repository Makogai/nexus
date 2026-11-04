//! Shells NEXUS can open in its terminal (ADR-0007).

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellInfo {
    pub id: String,
    pub name: String,
    pub program: PathBuf,
    pub args: Vec<String>,
}

fn shell(id: &str, name: &str, program: PathBuf, args: &[&str]) -> Option<ShellInfo> {
    program.is_file().then(|| ShellInfo {
        id: id.into(),
        name: name.into(),
        program,
        args: args.iter().map(|a| a.to_string()).collect(),
    })
}

#[cfg(windows)]
pub fn available() -> Vec<ShellInfo> {
    let system_root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let mut out = Vec::new();
    out.extend(which::which("pwsh").ok().and_then(|p| shell("pwsh", "PowerShell 7", p, &["-NoLogo"])));
    out.extend(shell(
        "powershell",
        "Windows PowerShell",
        system_root.join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        &["-NoLogo"],
    ));
    let comspec = std::env::var_os("ComSpec").map(PathBuf::from).unwrap_or_else(|| system_root.join(r"System32\cmd.exe"));
    out.extend(shell("cmd", "Command Prompt", comspec, &[]));
    if let Some(bash) = git_bash() {
        out.extend(shell("git-bash", "Git Bash", bash, &["--login", "-i"]));
    }
    out
}

#[cfg(windows)]
fn git_bash() -> Option<PathBuf> {
    // git.exe lives in <Git>\cmd\ or <Git>\bin\; bash.exe in <Git>\bin\.
    let from_git = which::which("git").ok().and_then(|git| {
        let root = git.parent()?.parent()?;
        Some(root.join("bin").join("bash.exe"))
    });
    let program_files = std::env::var_os("ProgramFiles").map(|p| Path::new(&p).join(r"Git\bin\bash.exe"));
    [from_git, program_files].into_iter().flatten().find(|p| p.is_file())
}

#[cfg(not(windows))]
pub fn available() -> Vec<ShellInfo> {
    let mut out = Vec::new();
    if let Some(sh) = std::env::var_os("SHELL").map(PathBuf::from) {
        let name = sh.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "shell".into());
        out.extend(shell("login", &format!("{name} (login shell)"), sh, &["-l"]));
    }
    for (id, path) in [("zsh", "/bin/zsh"), ("bash", "/bin/bash")] {
        if !out.iter().any(|s: &ShellInfo| s.program == Path::new(path)) {
            out.extend(shell(id, id, PathBuf::from(path), &["-l"]));
        }
    }
    out
}

pub fn find(id: &str) -> Option<ShellInfo> {
    available().into_iter().find(|s| s.id == id)
}

pub fn default_shell() -> Option<ShellInfo> {
    available().into_iter().next()
}
