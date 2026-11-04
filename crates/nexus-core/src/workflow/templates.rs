//! Built-in workflow templates (ADR-0029). "New from template" copies one into the workspace as an ordinary,
//! editable workflow document; nothing runs from the template itself. Gates are names (`test`, `lint`, `build`,
//! `typecheck`) the user binds to commands per project; steps marked `rigor=` only join runs at that rigor or above.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: &'static str,
    pub title: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    #[serde(skip)]
    pub body: &'static str,
}

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "feature",
        title: "Feature implementation",
        category: "feature",
        summary: "Plan, implement with tests, verify with the project's gates, review.",
        body: "Implement a feature end to end. Claude reports each step; NEXUS verifies with the project's gates.

## Understand and plan
<!-- step: id=plan kind=claude -->

Restate the goal and the acceptance criteria of the task. Identify the files and components involved and record a short plan as a NEXUS checkpoint (decision).

### Acceptance

- The plan is recorded as a checkpoint
- Open questions are asked before implementing

## Risk analysis
<!-- step: id=risks kind=claude needs=plan rigor=critical -->

List risks, failure modes, security or data implications and a rollback approach.

### Acceptance

- Risks and rollback are recorded as a checkpoint

## Implement with tests
<!-- step: id=implement kind=claude needs=plan,risks gates=test -->

Implement the change in small steps. Add or update automated tests for the new behaviour.

### Acceptance

- New behaviour is covered by tests
- Existing tests still pass

## Static checks
<!-- step: id=checks kind=check needs=implement gates=lint,typecheck rigor=balanced -->

## Documentation
<!-- step: id=docs kind=claude needs=implement rigor=professional -->

Update the documentation affected by the change (README, ADRs, comments where behaviour is non-obvious).

## Review
<!-- step: id=review kind=review needs=checks,docs rigor=professional -->

Review the diff and the verification results before the work is considered done.
",
    },
    Template {
        id: "bugfix",
        title: "Bug investigation and fix",
        category: "bugfix",
        summary: "Reproduce first, find the root cause, fix with a regression test.",
        body: "Investigate a bug from reproduction to a verified fix.

## Reproduce
<!-- step: id=reproduce kind=claude -->

Reproduce the problem. Prefer an automated failing test; otherwise record exact steps and observed vs. expected behaviour as a checkpoint.

### Acceptance

- A failing test or exact reproduction steps exist

## Root cause
<!-- step: id=cause kind=claude needs=reproduce -->

Find the root cause, not only the symptom. Record it as a checkpoint (decision) with the evidence.

## Fix with a regression test
<!-- step: id=fix kind=claude needs=cause gates=test -->

Fix the root cause. The reproduction test must pass now and stay in the suite.

### Acceptance

- The regression test passes
- No unrelated behaviour changed

## Review
<!-- step: id=review kind=review needs=fix rigor=professional -->

Confirm the root cause explanation and the fix.
",
    },
    Template {
        id: "review",
        title: "Code review",
        category: "review",
        summary: "Claude reviews a change; the gates confirm it builds and passes; you decide.",
        body: "Review a change (branch, pull request or working tree) and report findings.

## Verify the change builds and passes
<!-- step: id=verify kind=check gates=test -->

## Review the diff
<!-- step: id=analyse kind=claude needs=verify -->

Read the diff against its base. Report correctness, security, performance and maintainability findings with file and line, most severe first. Do not change code in this workflow.

### Acceptance

- Findings are recorded as checkpoints or in the task log
- Each finding names a file and line

## Decision
<!-- step: id=decision kind=review needs=analyse -->

Accept, request changes, or reject.
",
    },
    Template {
        id: "refactor",
        title: "Refactoring",
        category: "refactor",
        summary: "Behaviour-preserving change with green gates before and after.",
        body: "Refactor without changing behaviour.

## Baseline
<!-- step: id=baseline kind=check gates=test -->

## Refactor in small steps
<!-- step: id=refactor kind=claude needs=baseline gates=test -->

Make behaviour-preserving changes in small, reviewable steps. Do not mix in feature changes.

### Acceptance

- Behaviour is unchanged (tests unchanged and passing)
- The motivation is recorded as a checkpoint

## Static checks
<!-- step: id=checks kind=check needs=refactor gates=lint,typecheck rigor=balanced -->

## Review
<!-- step: id=review kind=review needs=checks rigor=professional -->
",
    },
    Template {
        id: "testing",
        title: "Test coverage",
        category: "testing",
        summary: "Find untested behaviour and add meaningful tests.",
        body: "Improve test coverage where it matters.

## Find gaps
<!-- step: id=gaps kind=claude -->

Identify important behaviour without tests (error paths, edge cases, recently changed code). Record the list as a checkpoint.

## Add tests
<!-- step: id=tests kind=claude needs=gaps gates=test -->

Add tests that would fail if the behaviour broke. Do not change production code unless a test reveals a bug (record it).

### Acceptance

- New tests fail without the behaviour they test
- The suite passes

## Review
<!-- step: id=review kind=review needs=tests rigor=professional -->
",
    },
    Template {
        id: "release",
        title: "Release preparation",
        category: "release",
        summary: "Gates, changelog and version, then your go/no-go.",
        body: "Prepare a release. NEXUS never publishes; the final step is your decision.

## Build and test
<!-- step: id=gates kind=check gates=build,test -->

## Static checks
<!-- step: id=checks kind=check needs=gates gates=lint,typecheck rigor=balanced -->

## Changelog and version
<!-- step: id=notes kind=claude needs=gates -->

Update the changelog and version numbers consistently. List breaking changes and migration notes.

### Acceptance

- Changelog lists user-visible changes since the last release
- Version numbers agree everywhere

## Go / no-go
<!-- step: id=decision kind=review needs=checks,notes -->

Check the verification results and the notes. Publishing stays a manual step outside NEXUS.
",
    },
];

pub fn get(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_is_a_valid_workflow() {
        for t in TEMPLATES {
            let (d, problems) = super::super::parse(t.body);
            assert!(problems.is_empty(), "{}: {problems:?}", t.id);
            assert!(super::super::order(&d).is_some(), "{}", t.id);
            assert!(super::super::CATEGORIES.contains(&t.category));
        }
    }
}
