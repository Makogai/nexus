//! Path resolution and validation.
//!
//! Every path that originates outside NEXUS (UI input, workspace manifests, config) goes through
//! [`validate_user_path`] or [`resolve_within`] before it touches the filesystem (threat T10).

use std::path::{Component, Path, PathBuf};

use crate::error::{CoreError, Result};

/// Environment variable that overrides the NEXUS data directory (tests, isolated development).
pub const DATA_DIR_ENV: &str = "NEXUS_DATA_DIR";

const MAX_PATH_CHARS: usize = 4096;

/// Platform-local data directory for NEXUS (`%LOCALAPPDATA%\NEXUS`, `~/Library/Application Support/NEXUS`,
/// `$XDG_DATA_HOME/NEXUS`). Local rather than roaming: the database holds machine-specific paths.
pub fn data_dir() -> Result<PathBuf> {
    if let Some(v) = std::env::var_os(DATA_DIR_ENV).filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(v));
    }
    dirs::data_local_dir()
        .map(|d| d.join("NEXUS"))
        .ok_or_else(|| CoreError::Unsupported("cannot determine the local data directory".into()))
}

/// The home folder as Claude Code sees it: Node's `os.homedir()` reads `USERPROFILE` on Windows (and `HOME`
/// elsewhere, which `dirs` already follows), so `~` means the same folder to NEXUS and to Claude.
pub fn home_dir() -> Result<PathBuf> {
    if cfg!(windows)
        && let Some(p) = std::env::var_os("USERPROFILE").map(PathBuf::from).filter(|p| p.is_absolute() && p.is_dir())
    {
        return Ok(p);
    }
    dirs::home_dir().ok_or_else(|| CoreError::Unsupported("cannot determine the home directory".into()))
}

/// What a validated path must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// Must exist and be a directory.
    ExistingDir,
    /// Must exist and be a file.
    ExistingFile,
    /// May exist (then it must be a directory) or not exist (then its parent must be a directory).
    DirOrCreatable,
}

/// Validate and normalize a path supplied by a user or the UI.
///
/// Rejects empty input, NUL bytes, overlong paths, relative paths, Windows device namespaces
/// (`\\?\`, `\\.\`) and UNC/network paths. Existing paths are canonicalized (symlinks resolved,
/// without the `\\?\` prefix on Windows).
pub fn validate_user_path(raw: &str, expect: Expect) -> Result<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(CoreError::invalid("path is empty"));
    }
    if raw.contains('\0') {
        return Err(CoreError::invalid("path contains a NUL character"));
    }
    if raw.chars().count() > MAX_PATH_CHARS {
        return Err(CoreError::invalid("path is too long"));
    }
    let normalized_seps = raw.replace('/', "\\");
    if normalized_seps.starts_with("\\\\?\\") || normalized_seps.starts_with("\\\\.\\") {
        return Err(CoreError::invalid("device namespace paths are not allowed"));
    }
    if cfg!(windows) && normalized_seps.starts_with("\\\\") {
        return Err(CoreError::invalid("network (UNC) paths are not supported"));
    }
    let path = expand_home(raw)?;
    if !path.is_absolute() {
        return Err(CoreError::invalid("path must be absolute"));
    }

    match expect {
        Expect::ExistingDir => {
            let canon = canonicalize(&path)?;
            if !canon.is_dir() {
                return Err(CoreError::invalid(format!("{} is not a directory", canon.display())));
            }
            Ok(canon)
        }
        Expect::ExistingFile => {
            let canon = canonicalize(&path)?;
            if !canon.is_file() {
                return Err(CoreError::invalid(format!("{} is not a file", canon.display())));
            }
            Ok(canon)
        }
        Expect::DirOrCreatable => {
            if path.exists() {
                let canon = canonicalize(&path)?;
                if !canon.is_dir() {
                    return Err(CoreError::invalid(format!("{} exists and is not a directory", canon.display())));
                }
                return Ok(canon);
            }
            let lexical = lexical_normalize(&path)?;
            let parent = lexical
                .parent()
                .ok_or_else(|| CoreError::invalid("path has no parent directory"))?;
            let parent = canonicalize(parent)?;
            if !parent.is_dir() {
                return Err(CoreError::invalid(format!("{} is not a directory", parent.display())));
            }
            let name = lexical
                .file_name()
                .ok_or_else(|| CoreError::invalid("path has no final component"))?;
            Ok(parent.join(name))
        }
    }
}

/// `~`, `~/x` and `~\x` → the home directory (shells like PowerShell do not expand `~` for native programs).
fn expand_home(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/").or_else(|| raw.strip_prefix("~\\")) {
        return Ok(home_dir()?.join(rest));
    }
    Ok(PathBuf::from(raw))
}

/// Resolve a relative path from untrusted content (e.g. a workspace manifest) inside `root`.
/// Rejects absolute paths and any `..`, root or prefix components, so the result can never escape `root`.
pub fn resolve_within(root: &Path, rel: &str) -> Result<PathBuf> {
    if rel.is_empty() || rel.contains('\0') {
        return Err(CoreError::invalid("relative path is empty or contains NUL"));
    }
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() || rel.starts_with('/') || rel.starts_with('\\') {
        return Err(CoreError::invalid(format!("'{rel}' must be relative to the workspace")));
    }
    let mut out = root.to_path_buf();
    for c in rel_path.components() {
        match c {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => return Err(CoreError::invalid(format!("'{rel}' escapes the workspace"))),
        }
    }
    Ok(out)
}

/// Express `path` relative to `root` with forward slashes, if it is inside `root`.
pub fn relative_slash_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    let parts: Vec<String> = rel
        .components()
        .map(|c| match c {
            Component::Normal(p) => Some(p.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(parts.join("/"))
}

/// Case-insensitive on Windows, exact elsewhere.
pub fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// `path` is `dir` or inside it (lexical, by components; case-insensitive and `/`≡`\` on Windows). No file access.
pub fn is_within(path: &Path, dir: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p.to_string_lossy().replace('\\', "/");
        let s = s.trim_end_matches('/').to_string();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    let (p, d) = (norm(path), norm(dir));
    !d.is_empty() && (p == d || p.starts_with(&format!("{d}/")))
}

fn canonicalize(path: &Path) -> Result<PathBuf> {
    dunce::canonicalize(path).map_err(|e| CoreError::io("cannot access", path, e))
}

fn lexical_normalize(path: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return Err(CoreError::invalid("path escapes its root"));
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_hostile_inputs() {
        for bad in ["", "   ", "relative/dir", "a\0b", r"\\?\C:\Windows", r"\\.\pipe\x", "//./pipe/x"] {
            assert!(validate_user_path(bad, Expect::ExistingDir).is_err(), "accepted {bad:?}");
        }
        let long = format!("C:\\{}", "a".repeat(5000));
        assert!(validate_user_path(&long, Expect::DirOrCreatable).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_unc_on_windows() {
        assert!(validate_user_path(r"\\server\share\x", Expect::DirOrCreatable).is_err());
    }

    #[test]
    fn existing_dir_is_canonicalized() {
        let t = tempfile::tempdir().unwrap();
        let sub = t.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let dotted = format!("{}{}..{}sub", sub.display(), std::path::MAIN_SEPARATOR, std::path::MAIN_SEPARATOR);
        let got = validate_user_path(&dotted, Expect::ExistingDir).unwrap();
        assert!(same_path(&got, &dunce::canonicalize(&sub).unwrap()));
        assert!(!got.to_string_lossy().starts_with(r"\\?\"));
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = dunce::canonicalize(home_dir().unwrap()).unwrap();
        assert!(same_path(&validate_user_path("~", Expect::ExistingDir).unwrap(), &home));
        let p = validate_user_path("~/nexus-test-does-not-exist", Expect::DirOrCreatable).unwrap();
        assert!(p.starts_with(&home));
        assert!(validate_user_path("~user/x", Expect::DirOrCreatable).is_err(), "~user is not supported");
    }

    #[test]
    fn file_is_not_a_dir() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("f.txt");
        std::fs::write(&f, "x").unwrap();
        assert!(validate_user_path(f.to_str().unwrap(), Expect::ExistingDir).is_err());
        assert!(validate_user_path(f.to_str().unwrap(), Expect::ExistingFile).is_ok());
    }

    #[test]
    fn creatable_requires_existing_parent() {
        let t = tempfile::tempdir().unwrap();
        let ok = t.path().join("new-ws");
        assert!(validate_user_path(ok.to_str().unwrap(), Expect::DirOrCreatable).is_ok());
        let deep = t.path().join("missing").join("new-ws");
        assert!(validate_user_path(deep.to_str().unwrap(), Expect::DirOrCreatable).is_err());
    }

    #[test]
    fn resolve_within_blocks_escapes() {
        let root = Path::new(if cfg!(windows) { r"C:\ws" } else { "/ws" });
        assert!(resolve_within(root, "repos/app").is_ok());
        assert!(resolve_within(root, "./repos/app").is_ok());
        for bad in ["../x", "repos/../../x", "/etc", r"\x", "", "C:/x"] {
            let r = resolve_within(root, bad);
            if cfg!(windows) || bad != "C:/x" {
                assert!(r.is_err(), "accepted {bad:?}");
            }
        }
    }

    #[test]
    fn relative_slash_paths() {
        let root = PathBuf::from(if cfg!(windows) { r"C:\ws" } else { "/ws" });
        assert_eq!(relative_slash_path(&root, &root.join("repos").join("app")).as_deref(), Some("repos/app"));
        assert_eq!(relative_slash_path(&root, &root), None);
        let other = PathBuf::from(if cfg!(windows) { r"D:\elsewhere" } else { "/elsewhere" });
        assert_eq!(relative_slash_path(&root, &other), None);
    }
}
