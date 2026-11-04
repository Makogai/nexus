//! Branch-aware knowledge (ADR-0018). Git is authoritative for branch state: NEXUS never stores which commits a
//! branch contains. A document may *name* a branch (`branch:`) and the commit it was recorded at (`commit:`, a stable
//! identifier); relevance is decided at query time from the project's live Git history.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use super::index::Item;
use crate::git;

/// How far back the HEAD history is read to decide whether a recorded commit is part of the current branch.
pub const HISTORY_DEPTH: usize = 5000;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GitRef {
    pub branch: Option<String>,
    pub head: Option<String>,
}

impl GitRef {
    /// Live state of the repository at `repo` (empty when it is not a repository).
    pub fn of(repo: &Path) -> GitRef {
        git::status(repo).map(|s| GitRef { branch: s.branch, head: s.head_oid }).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Relevance {
    /// Not tied to a branch.
    Unbound,
    /// Recorded for the branch that is checked out now.
    ThisBranch,
    /// Recorded on a commit that is part of the current history (e.g. its branch was merged).
    Inherited,
    /// Belongs to another branch whose work is not in the current history — hidden by default.
    OtherBranch,
}

impl Relevance {
    pub fn visible(self) -> bool {
        self != Relevance::OtherBranch
    }
}

/// Relevance of one document for the checked-out state. `history` holds commit ids reachable from HEAD.
pub fn relevance(item: &Item, current: &GitRef, history: &HashSet<String>) -> Relevance {
    match (&item.branch, &item.commit) {
        (None, None) => Relevance::Unbound,
        (Some(b), _) if current.branch.as_deref() == Some(b.as_str()) => Relevance::ThisBranch,
        (_, Some(c)) if history.contains(c) => Relevance::Inherited,
        _ => Relevance::OtherBranch,
    }
}

/// Commit ids reachable from HEAD (bounded), or empty outside a repository.
pub fn history(repo: &Path) -> HashSet<String> {
    git::history(repo, HISTORY_DEPTH).unwrap_or_default().into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(branch: Option<&str>, commit: Option<&str>) -> Item {
        Item {
            id: None, workspace_id: Some("w".into()), path: "x.md".into(), kind: Some("note".into()), title: "t".into(),
            status: None, project_id: None, priority: None, stacks: vec![], applies: None, privacy: "shared".into(),
            updated: None, hash: String::new(), valid: true, error: None, conflict: None,
            branch: branch.map(Into::into), commit: commit.map(Into::into),
        }
    }

    #[test]
    fn relevance_follows_the_live_history() {
        let c1 = "1".repeat(40);
        let c2 = "2".repeat(40);
        let on_main = GitRef { branch: Some("main".into()), head: Some(c1.clone()) };
        let history: HashSet<String> = [c1.clone()].into();
        assert_eq!(relevance(&item(None, None), &on_main, &history), Relevance::Unbound);
        assert_eq!(relevance(&item(Some("main"), None), &on_main, &history), Relevance::ThisBranch);
        assert_eq!(relevance(&item(Some("feature/x"), Some(&c1)), &on_main, &history), Relevance::Inherited, "merged work");
        assert_eq!(relevance(&item(Some("feature/x"), Some(&c2)), &on_main, &history), Relevance::OtherBranch);
        assert_eq!(relevance(&item(Some("feature/x"), None), &on_main, &history), Relevance::OtherBranch);
        assert!(!Relevance::OtherBranch.visible() && Relevance::Inherited.visible());
    }
}
