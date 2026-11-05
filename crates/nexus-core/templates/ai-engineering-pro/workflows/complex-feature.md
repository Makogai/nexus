---
nexus: 1
id: {{id}}
kind: workflow
format: workflow
title: Complex feature
status: active
category: feature
rigor: professional
skills: [workspace:feature-development, workspace:security-review, workspace:playwright-testing, workspace:architecture-documentation]
created: {{now}}
updated: {{now}}
source: import
---

Use this workflow for a feature that needs a design decision before code: it spans several components or services, changes public APIs or stored data, handles personal, financial or security-sensitive data, or is hard to reverse. A person approves the design and the risk analysis before implementation starts; implementation proceeds in tested vertical slices, and critical user journeys are verified end to end.

## Discovery and requirements
<!-- step: id=discover kind=claude -->

Establish what is being built and why: the users and their journeys, functional requirements as testable acceptance criteria, non-functional requirements with numbers (latency, throughput, data volume, availability, accessibility level), constraints (compatibility, deadlines, regulated data) and explicit non-goals. Survey the existing code, ADRs and constitutions that govern the affected areas (with the NEXUS tools: `get_constitution`, `search_knowledge`). Report needs-input for questions only the person can answer, and list the assumptions that remain.

### Acceptance

- Acceptance criteria are testable and recorded in the task
- Non-functional requirements have measurable targets or are marked not applicable
- Non-goals, constraints and assumptions are listed
- Open questions are answered or reported as needs-input

## Design note and ADR proposal
<!-- step: id=design kind=claude needs=discover -->

Write a design note in the project's ADR format (status: proposed): context, the decision, at least two alternatives with the reason each lost, component and data-flow changes, API and schema changes with their versioning and migration plan, failure modes, rollout (feature flag, migration order, compatibility window) and rollback. Split the implementation into ordered vertical slices that can each be tested and merged on their own, with incomplete behaviour behind a flag.

### Acceptance

- The design note exists with status proposed and lists the alternatives considered
- Migration, rollout and rollback are described
- The implementation is split into ordered slices, each with the tests it needs

## Risk and security analysis
<!-- step: id=risks kind=claude needs=design -->

Threat-model the design with the security-review skill: assets and trust boundaries touched, new inputs and who controls them, authentication and authorisation changes, the classification of the data handled (personal, financial, credentials) and where it is stored, logged or sent, new dependencies with their licence and maintenance state, and abuse cases. Add operational risks: migrations on large tables, backward compatibility, performance regressions, concurrency. For each significant risk give likelihood, impact, the mitigation and the test or check that will verify the mitigation, and add them to the design note.

### Acceptance

- Trust boundaries, data handled and new dependencies are listed
- Each significant risk has a mitigation and a named verification
- Handling of personal or regulated data is described, or recorded as none

## Design and risk approval
<!-- step: id=approve-design kind=review needs=design,risks -->

A person reviews the design note and the risk analysis before any implementation starts. Approve to proceed; request changes with a note to send the design and the analysis back to Claude.

## Implement in slices
<!-- step: id=implement kind=claude needs=approve-design gates=test -->

Implement the slices in the approved order. Per slice: write tests for its acceptance criteria and risk mitigations first, then the code, then refactor while green; keep each slice a reviewable commit that leaves the main branch releasable. Record a progress checkpoint after each slice. If the implementation has to depart from the approved design, stop and report needs-input instead of diverging silently. Report tests as passing only if they ran and passed in this session.

### Acceptance

- Every acceptance criterion and security mitigation is covered by an automated test
- Each slice is a separate commit on which the test suite passes
- Departures from the approved design are recorded and were confirmed by the person

## Static checks
<!-- step: id=checks kind=check needs=implement gates=lint,typecheck -->

NEXUS runs the project's lint and typecheck gates on the implementation.

## End-to-end tests for critical journeys
<!-- step: id=e2e kind=claude needs=implement rigor=professional -->

Cover the critical user journeys from discovery end to end against a running build: the main success path, the most likely failure path (validation error, permission denied, unavailable dependency) and every path that crosses a service boundary. For web interfaces use the playwright-testing skill: role-based locators, web-first assertions, no fixed waits, isolated test data. Run the new tests several times in a row before reporting to expose flakiness, and record the command and the results in the task log.

### Acceptance

- Each critical journey has an end-to-end test covering success and its main failure path
- The tests were run repeatedly in this session and passed every time
- Test data is isolated and created by the tests themselves

## Documentation and architecture update
<!-- step: id=docs kind=claude needs=implement -->

Use the architecture-documentation skill. Move the ADR from proposed to accepted and record any departures from the approved design. Update architecture descriptions and diagrams for new components and data flows, the API reference, configuration and operations notes (new settings, migrations, feature flags and how to remove them, monitoring and alerts), user-facing documentation and the changelog.

### Acceptance

- The ADR is accepted and matches what was built
- Architecture, API, configuration and operations documentation reflect the change
- Removal of any feature flag is tracked as a task

## Final review
<!-- step: id=review kind=review needs=implement,checks,e2e,docs -->

A person reviews the diff against the approved design and the risk mitigations, together with the gate results, the end-to-end results and the documentation. Approve to finish; request changes with a note to send implementation, end-to-end tests and documentation back to Claude.
