//! Catalog of skills visible to a session (ADR-0017): workspace skills (`<workspace>/skills/<name>/SKILL.md`) and the
//! project's native Claude skills (`<project>/.claude/skills/<name>/SKILL.md`). Only name and description are read —
//! Claude loads skill bodies itself (progressive disclosure). SKILL.md uses full YAML, so only the two top-level
//! scalar keys are extracted, leniently; nothing is executed.

use std::path::Path;

use serde::Serialize;

const MAX_SKILLS: usize = 50;
const MAX_SKILL_FILE: u64 = 256 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// `workspace` or `project`.
    pub source: &'static str,
}

pub fn catalog(workspace_dir: &Path, project_dir: Option<&Path>) -> Vec<Skill> {
    let mut out = Vec::new();
    collect(&workspace_dir.join("skills"), "workspace", &mut out);
    if let Some(p) = project_dir {
        collect(&p.join(".claude").join("skills"), "project", &mut out);
    }
    out.truncate(MAX_SKILLS);
    out
}

fn collect(dir: &Path, source: &'static str, out: &mut Vec<Skill>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let file = e.path().join("SKILL.md");
        let Ok(meta) = std::fs::metadata(&file) else { continue };
        if meta.len() > MAX_SKILL_FILE {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let fallback = e.file_name().to_string_lossy().into_owned();
        let (name, description) = header(&text);
        out.push(Skill {
            name: clip(name.unwrap_or(fallback)),
            description: clip(description.unwrap_or_default()),
            source,
        });
    }
}

fn header(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }
    let (mut name, mut description) = (None, None);
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
                "description" if !v.is_empty() && !v.starts_with(['|', '>']) => description = Some(v),
                _ => {}
            }
        }
    }
    (name, description)
}

fn clip(s: String) -> String {
    let s: String = s.chars().filter(|c| !c.is_control()).collect();
    if s.chars().count() > 200 { format!("{}…", s.chars().take(199).collect::<String>()) } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_workspace_and_project_skills_leniently() {
        let t = tempfile::tempdir().unwrap();
        let w = t.path().join("ws");
        let p = t.path().join("proj");
        for (dir, body) in [
            (w.join("skills").join("release-notes"), "---\nname: release-notes\ndescription: \"Draft release notes from merged PRs\"\nallowed-tools: Bash(git log:*)\nmetadata:\n  owner: team\n---\nBody"),
            (p.join(".claude").join("skills").join("db"), "---\ndescription: >\n  folded text\n---\n"),
            (w.join("skills").join("no-frontmatter"), "# just text"),
        ] {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), body).unwrap();
        }
        let c = catalog(&w, Some(&p));
        assert_eq!(c.len(), 3);
        assert_eq!(c[1], Skill { name: "release-notes".into(), description: "Draft release notes from merged PRs".into(), source: "workspace" });
        assert_eq!((c[0].name.as_str(), c[0].description.as_str()), ("no-frontmatter", ""));
        assert_eq!((c[2].name.as_str(), c[2].source), ("db", "project"));
    }
}
