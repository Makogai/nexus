//! Deterministic project inspection. Repository files are untrusted data: they are read with size caps,
//! parsed, and only ever *displayed* or offered as suggestions — nothing here executes anything.

use std::path::Path;

use serde::{Deserialize, Serialize};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StackInfo {
    /// Detected technologies, most specific first (e.g. `angular`, `typescript`, `node`).
    pub stacks: Vec<String>,
    pub package_managers: Vec<String>,
    /// Suggested commands derived from manifests. Never run automatically.
    pub scripts: Vec<ScriptHint>,
    pub has_readme: bool,
    pub has_claude_md: bool,
    pub has_agents_md: bool,
    pub has_claude_dir: bool,
    pub is_git: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScriptHint {
    /// `dev`, `build`, `test`, `lint`, `typecheck`, `start`.
    pub kind: String,
    pub command: String,
}

fn push(v: &mut Vec<String>, s: &str) {
    if !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

fn read_capped(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_MANIFEST_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn any_with_ext(dir: &Path, exts: &[&str]) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().take(500).any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .is_some_and(|x| exts.iter().any(|want| x.eq_ignore_ascii_case(want)))
            })
        })
        .unwrap_or(false)
}

pub fn detect(root: &Path) -> StackInfo {
    let mut info = StackInfo {
        is_git: root.join(".git").exists(),
        has_readme: ["README.md", "README", "readme.md", "README.rst"].iter().any(|f| root.join(f).is_file()),
        has_claude_md: root.join("CLAUDE.md").is_file() || root.join(".claude").join("CLAUDE.md").is_file(),
        has_agents_md: root.join("AGENTS.md").is_file(),
        has_claude_dir: root.join(".claude").is_dir(),
        ..Default::default()
    };
    let has = |f: &str| root.join(f).exists();

    // JavaScript / TypeScript
    let mut pm = None;
    for (lock, name) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("bun.lock", "bun"),
        ("package-lock.json", "npm"),
    ] {
        if has(lock) {
            push(&mut info.package_managers, name);
            pm.get_or_insert(name);
        }
    }
    if let Some(raw) = read_capped(&root.join("package.json")) {
        let pm = pm.unwrap_or("npm");
        if info.package_managers.is_empty() {
            push(&mut info.package_managers, "npm");
        }
        if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&raw) {
            let dep = |name: &str| {
                ["dependencies", "devDependencies", "peerDependencies"]
                    .iter()
                    .any(|k| pkg.get(k).and_then(|d| d.get(name)).is_some())
            };
            for (dep_name, stack) in [
                ("@angular/core", "angular"),
                ("next", "nextjs"),
                ("nuxt", "nuxt"),
                ("vue", "vue"),
                ("react", "react"),
                ("svelte", "svelte"),
                ("@tauri-apps/api", "tauri"),
                ("electron", "electron"),
                ("@nestjs/core", "nestjs"),
                ("express", "express"),
                ("phaser", "phaser"),
                ("three", "threejs"),
                ("vite", "vite"),
            ] {
                if dep(dep_name) {
                    push(&mut info.stacks, stack);
                }
            }
            if dep("typescript") || has("tsconfig.json") {
                push(&mut info.stacks, "typescript");
            }
            if let Some(scripts) = pkg.get("scripts").and_then(|s| s.as_object()) {
                for kind in ["dev", "start", "build", "test", "lint", "typecheck"] {
                    if scripts.contains_key(kind) {
                        let command = match (pm, kind) {
                            ("npm", "test" | "start") => format!("npm {kind}"),
                            ("npm", _) => format!("npm run {kind}"),
                            (other, _) => format!("{other} {kind}"),
                        };
                        info.scripts.push(ScriptHint {
                            kind: kind.to_string(),
                            command,
                        });
                    }
                }
            }
        }
        push(&mut info.stacks, "node");
    }
    if has("angular.json") {
        push(&mut info.stacks, "angular");
    }

    // Rust
    if let Some(cargo) = read_capped(&root.join("Cargo.toml")) {
        push(&mut info.stacks, "rust");
        push(&mut info.package_managers, "cargo");
        if cargo.contains("tauri") {
            push(&mut info.stacks, "tauri");
        }
        for (kind, cmd) in [("build", "cargo build"), ("test", "cargo test"), ("lint", "cargo clippy")] {
            info.scripts.push(ScriptHint {
                kind: kind.into(),
                command: cmd.into(),
            });
        }
    }

    // PHP / Laravel
    if let Some(composer) = read_capped(&root.join("composer.json")) {
        push(&mut info.package_managers, "composer");
        if composer.contains("laravel/framework") || has("artisan") {
            push(&mut info.stacks, "laravel");
        }
        push(&mut info.stacks, "php");
    }

    // Python
    if has("pyproject.toml") || has("requirements.txt") || has("setup.py") {
        push(&mut info.stacks, "python");
        if has("uv.lock") {
            push(&mut info.package_managers, "uv");
        } else if has("poetry.lock") {
            push(&mut info.package_managers, "poetry");
        } else {
            push(&mut info.package_managers, "pip");
        }
        if let Some(py) = read_capped(&root.join("pyproject.toml")) {
            for (needle, stack) in [("django", "django"), ("fastapi", "fastapi"), ("flask", "flask")] {
                if py.to_lowercase().contains(needle) {
                    push(&mut info.stacks, stack);
                }
            }
        }
    }

    // Flutter / Dart
    if let Some(pubspec) = read_capped(&root.join("pubspec.yaml")) {
        if pubspec.contains("flutter:") || pubspec.contains("sdk: flutter") {
            push(&mut info.stacks, "flutter");
        }
        push(&mut info.stacks, "dart");
    }

    // Others
    if has("go.mod") {
        push(&mut info.stacks, "go");
    }
    if any_with_ext(root, &["sln", "csproj", "fsproj"]) {
        push(&mut info.stacks, "dotnet");
    }
    if has("pom.xml") || has("build.gradle") || has("build.gradle.kts") {
        push(&mut info.stacks, "jvm");
    }
    if has("Gemfile") {
        push(&mut info.stacks, "ruby");
    }
    if has("project.godot") {
        push(&mut info.stacks, "godot");
    }
    if root.join("ProjectSettings").join("ProjectVersion.txt").is_file() {
        push(&mut info.stacks, "unity");
    }
    if any_with_ext(root, &["uproject"]) {
        push(&mut info.stacks, "unreal");
    }
    if has("default.project.json") || any_with_ext(root, &["rbxlx", "rbxl"]) {
        push(&mut info.stacks, "roblox");
    }

    // Most specific first: frameworks before languages/runtimes.
    const GENERIC: &[&str] = &["vite", "typescript", "node", "php", "dart", "python", "rust"];
    info.stacks.sort_by_key(|s| GENERIC.iter().position(|g| g == s).map_or(0, |i| i + 1));
    info
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
    fn angular_project() {
        let t = tempfile::tempdir().unwrap();
        write(
            t.path(),
            "package.json",
            r#"{"dependencies":{"@angular/core":"^20"},"devDependencies":{"typescript":"5"},
               "scripts":{"start":"ng serve","build":"ng build","test":"ng test"}}"#,
        );
        write(t.path(), "angular.json", "{}");
        write(t.path(), "package-lock.json", "{}");
        write(t.path(), "CLAUDE.md", "# rules");
        let info = detect(t.path());
        assert_eq!(info.stacks.first().map(String::as_str), Some("angular"));
        assert!(info.stacks.contains(&"typescript".to_string()));
        assert_eq!(info.package_managers, vec!["npm"]);
        let cmds: Vec<_> = info.scripts.iter().map(|s| s.command.as_str()).collect();
        assert_eq!(cmds, vec!["npm start", "npm run build", "npm test"]);
        assert!(info.has_claude_md);
        assert!(!info.is_git);
    }

    #[test]
    fn pnpm_vue_and_rust_tauri() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "package.json", r#"{"dependencies":{"vue":"3"},"scripts":{"dev":"vite"}}"#);
        write(t.path(), "pnpm-lock.yaml", "");
        write(t.path(), "Cargo.toml", "[dependencies]\ntauri = \"2\"\n");
        let info = detect(t.path());
        assert!(info.stacks.contains(&"vue".into()) && info.stacks.contains(&"tauri".into()));
        assert!(info.scripts.iter().any(|s| s.command == "pnpm dev"));
        assert!(info.package_managers.contains(&"cargo".into()));
    }

    #[test]
    fn laravel_python_flutter_godot() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "composer.json", r#"{"require":{"laravel/framework":"^11"}}"#);
        write(t.path(), "pyproject.toml", "[project]\ndependencies=['FastAPI']\n");
        write(t.path(), "uv.lock", "");
        write(t.path(), "pubspec.yaml", "dependencies:\n  flutter:\n    sdk: flutter\n");
        write(t.path(), "project.godot", "");
        let info = detect(t.path());
        for s in ["laravel", "php", "python", "fastapi", "flutter", "dart", "godot"] {
            assert!(info.stacks.contains(&s.to_string()), "missing {s}: {:?}", info.stacks);
        }
        assert!(info.package_managers.contains(&"uv".into()));
    }

    #[test]
    fn hostile_inputs_are_tolerated() {
        let t = tempfile::tempdir().unwrap();
        write(t.path(), "package.json", "{ this is not json");
        let big = "x".repeat((MAX_MANIFEST_BYTES + 10) as usize);
        write(t.path(), "Cargo.toml", &big);
        let info = detect(t.path());
        assert!(info.stacks.contains(&"node".into()));
        assert!(!info.stacks.contains(&"rust".into()), "oversized manifests are ignored");
    }

    #[test]
    fn empty_dir() {
        let t = tempfile::tempdir().unwrap();
        assert_eq!(detect(t.path()), StackInfo::default());
    }
}
