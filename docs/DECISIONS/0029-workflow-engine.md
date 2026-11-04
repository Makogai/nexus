# ADR-0029 — Workflow Engine: portable definitions on tasks and sessions, trusted gates, AI-reported vs. verified

Status: Accepted, implemented (2026-11-04, stage "Workflow Engine"; refines ADR-0012, which it implements)

## Context
The owner wants reusable workflows — feature implementation, bug investigation, code review, refactoring, testing,
release preparation — with steps, dependencies, acceptance criteria and deterministic quality gates, launched from
projects, tasks and a library, with notifications, rigor by work profile, cancellation, retry and crash recovery,
and a clear line between what Claude *says* it did and what was *verified*. No separate heavyweight orchestrator;
reuse tasks, sessions, skills, hooks, constitutions, checkpoints and MCP; keep the subsystems separate (ADR-0026).

## Decision
* **Definition** = a portable workspace document (`kind: workflow`, `format: workflow`; `category`, default `rigor`,
  `skills: [kit:skill]`). Body: `## step` + `<!-- step: id=… kind=claude|check|review needs=a,b gates=test,lint
  rigor=professional -->` + instructions + `### Acceptance` list. Validated (ids, dependencies exist, no cycles,
  ≤ 50 steps, gate *names* only). Versions changed outside NEXUS need approval before they run (as rules, T19).
  Six built-in templates are copied into the workspace as ordinary documents.
* **Gates** = the trusted verification mechanism: per project, **machine-local**, defined by the person (desktop or
  `nexus gates`) as program + argument vector (no shell), timeout, suggested from detected scripts but never added
  automatically. A workflow names gates; it cannot carry or change a command (T33). Gates run in a NEXUS **host**
  process — the desktop's runner thread, or `nexus workflow verify` / `nexus gates run` in a terminal — never in the
  MCP server (ADR-0009) and never because Claude says so: with the user's environment minus Claude session markers,
  captured output (tail, credential-like tokens redacted, T34), exit code and duration; executions record their host
  so a crash is reconciled (interrupted → verified again).
* **Gate approval and versions** (amendment, migration 0013) — separate from approving a workflow definition: a gate
  runs only when its *current command* was explicitly approved. Every gate has a `command_hash` over program +
  arguments (timeout and title excluded) and a `version` that increments whenever the command changes; approval pins
  the hash the person reviewed (`gates::approve(expected_hash)` refuses a command changed since; the CLI requires
  `--version n`, the desktop shows the exact command in the confirmation). The single choke point `gates::begin`
  refuses unless the stored approval equals both the stored and the recomputed hash, so every path — runner,
  `nexus workflow verify`, `nexus gates run`, `nexus constitution verify`, the desktop's "Run now" — is covered. A
  step whose gates are missing or unapproved is blocked with `blocked_on = gates` (nothing executes) and an
  input-required notification; approving resumes its verification without a new report from Claude. Results record
  the gate version that produced them. `nexus gates approve|revoke`, desktop Approve / Revoke, constitution
  compliance `not-approved`.
* **Run** = machine-local state (`workflow_runs`, `workflow_steps`, `gate_results`, migration 0012) over a snapshot of
  the approved definition, carried by an ordinary **task** (created with one checklist item per included step, or an
  existing task left as it is). Steps go pending → ready / verifying / awaiting-review → passed | failed | blocked |
  skipped | cancelled; each passed step records **how** it is known:
  * `ai-reported` — Claude's `report_step` on a step without gates (shown as "AI-reported — not verified");
  * `verified` — all attached gates passed in a host process;
  * `human` — a person approved a review step (or, at **critical** rigor, an AI-reported step without gates);
  * `skipped` — not part of this rigor.
  A failing gate on a Claude step returns it to Claude with the output (attempts bounded by rigor: 3 / 3 / 2 / 2);
  a failing `check` step blocks for a person's retry; running out of attempts fails the run. A rejected review
  reopens the Claude steps it depends on. Missing gates block with an explanation and a notification.
* **Rigor** = the work profiles (ADR-0011): steps declare a minimum rigor; the run's rigor is also the session's work
  profile; critical requires a person for every unverified step.
* **Sessions** = the existing task launch (`claude::prepare` with the run's task): the system prompt gets only a
  NEXUS-authored pointer; SessionStart and `get_workflow_run` deliver the bounded, escaped run brief and the ready
  steps' instructions; `report_step` records claims. The workflow's skills are added **for that session only** — in
  a separate overlay folder, and only from kits already enabled for the session's profile; others are refused with
  the reason (profile isolation, ADR-0024).
* **Memory & notifications**: step reports and verification results go to the task's log and checklist; completion
  adds a checkpoint (so proposals work unchanged). Notifications (ADR-0025): input needed / blocked, review pending,
  gate failed, workflow completed (new category), workflow failed — targets open the run.
* **Constitutions** (ADR-0027): a rule may name a gate (`check=`); a `gates=constitution` step runs every gate linked
  from the effective rules — compliance is then *verified*, otherwise *instructed only*.
* **Surfaces**: desktop *Workflows* (library with templates, visual steps, Markdown editing, approval, default rigor
  and skills; runs with progress, per-step verification badges, gate results and output, review / retry / cancel,
  open or launch the session; gates per project with suggestions and "run now"); entry points from the project page
  and the task dialog; `nexus workflow templates|list|show|new|start|runs|status|verify|review|cancel|retry` and
  `nexus gates list|add|approve|revoke|remove|suggest|run`; MCP `get_workflow_run`, `report_step` (21 tools).

## Verification
Core tests (definition parsing/round trip/validation/cycles; templates valid; gates: exit codes, missing program,
redaction, reconcile; runs: AI-reported → verified → signed off, failure → back to Claude → failed → retry, rigor
selection, critical sign-off, rejected review, missing gate, cancel, crash recovery, one active run per task,
existing-task runs; constitution compliance; gate approval: unapproved / changed / forged commands refused, approval
resumes waiting work without a new report); CLI E2E (template → start → task launch with pointer → MCP reports →
unapproved gate refused → approve → `nexus workflow verify` → missing gate → unapproved → approval resumes → completed
+ notified + task done); vitest (run detail, start dialog, gates panel incl. approval, helpers); desktop E2E
`e2e/workflows.mjs` (incl. approval through the UI dialog, a changed command waiting for re-approval, the desktop
runner, review in the UI, notification click, failure path, cancel, kill + restart during a running gate); real Claude 2.1.311
`e2e/workflow-real-claude.mjs` (7 checks: the workflow's skill appears only in the workflow's session of the profile
that has the kit; a Personal session gets nothing and the refusal is recorded).

## Alternatives
* A scheduler that drives Claude step by step (separate sessions per step, prompt injection) — duplicates Claude's
  own agent loop, costs context, and turns NEXUS into an orchestrator (rejected by the owner and ADR-0012).
* Gates inside workflow files — synced files would run arbitrary commands on every machine (T33).
* Letting Claude run the gates through MCP — makes verification depend on the party being verified.

## Consequences
* A run needs the project folder on this machine to verify; without it the step is blocked with that reason.
* Gates run with the user's rights (like any build); the UI says so, the user defines them and approves each command
  version before it runs.
* Parallel steps are allowed by the dependency graph; one Claude session works on the run (native subagents can
  parallelise inside it).
