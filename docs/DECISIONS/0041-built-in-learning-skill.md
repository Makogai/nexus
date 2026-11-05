# ADR-0041 — A built-in learning skill and rules: one lean session per project

Status: Accepted (2026-11-05, owner feedback after the first real use of *Learn*; branch `milestone/learn-skill`)

## Context

The first real learning session (a Flutter app, about 60 code files) showed the cost of leaving the procedure to
Claude. NEXUS's learning task carried a checklist, and Claude turned it into its own multi-agent plan — *Read →
Verify → Draft → Attack*, with an adversarial phase that started one checker per claim, so the agent count kept
growing. The work profile chosen in the launch dialog encouraged it (*Balanced* suggests an independent review,
*Professional* and *Critical* more). The owner: "it EATS up tokens like crazy — it should really be a Claude skill from
NEXUS, with constitutions for it."

What a learning session must produce is small and fixed: two drafts the person reviews, a handful of memories, open
questions. The person's review is the quality gate; a second AI review round adds cost, not safety.

The owner chose (2026-11-05): the skill and its rules are **built into NEXUS**, and the person picks a **depth** in
the Learn tab.

## Decision

1. **A built-in plugin `nexus` with the skill `learn-project`** (`crates/nexus-core/builtin/nexus/`, embedded with
   `include_str!`; `claude::builtin`).
   * Skills only — no hooks, no MCP servers, no agents, no code. Read-only and versioned with NEXUS: NEXUS writes it
     into its data folder (`builtin/<content hash>/nexus/`), swapping in a complete folder, repairing it when its
     files differ, and leaving older versions for sessions that may still read them.
   * The skill is the procedure: orient from NEXUS's map and the Graphify report (`graphify query` instead of opening
     files), read in a fixed order until the budget is used, draft the overview and the architecture summary with
     fixed sections and *(verified: path)* / *(inferred)* marks, record memories, update the task, stop and report.
   * It reaches **only learning sessions**: the session gets the plugin folder with `--plugin-dir` (the session-only
     overlay pattern of ADR-0029). Other sessions never list it, so it costs them nothing.
   * The name `nexus` is reserved: registering an AI Kit with that plugin name is refused, and if an existing kit of
     the profile already uses it, the built-in skill is withheld (the rules still apply).
   * Organisation policy wins: with `disableSideloadFlags` or `strictPluginOnlyCustomization` the skill is withheld,
     and the session records why.

2. **The learning rules LEARN-1 to LEARN-7** — a small built-in constitution, sent in the system prompt of learning
   sessions only (`project::learn::rules`). They have stable ids, are NEXUS-authored and built from the validated depth
   alone (no task text, T22), and hold even where the work profile asks for more:
   * LEARN-1 follow the `nexus:learn-project` skill;
   * LEARN-2 work alone — no subagents, agent teams or multi-agent workflows unless the user asks;
   * LEARN-3 read only — no file changes, no builds, tests or installs;
   * LEARN-4 the depth's budget;
   * LEARN-5 mark statements verified or inferred;
   * LEARN-6 drafts and memories only through `draft_project_document` and `record_memory`;
   * LEARN-7 stop when they are saved, report what was skipped, ask before reading further.

   About 1,000 characters. The rules are always present, so the session stays lean even when the skill is withheld or
   Claude does not invoke it.

3. **Depth** — one Claude session in every case; the depth limits the files Claude opens and the length of the drafts:

   | Depth | Files | Overview | Architecture | Memories |
   | --- | --- | --- | --- | --- |
   | Quick | 20 | 250 words | 400 words | 3–5 |
   | Standard (default) | 50 | 400 words | 700 words | 3–8 |
   | Deep | no limit, still selective | 700 words | 1,200 words | 5–10 |

   The depth is a label on the learning task (`learn-quick`, `learn-standard`, `learn-deep`); choosing another depth
   before starting again replaces it. A learning task without a depth label (created before this ADR) is Standard.

4. **Transparency.** The Learn tab shows each depth's budget and, on request, the exact rules Claude gets (the text
   comes from the backend, so it cannot drift). The session's integrations record the depth and whether the skill was
   given (`learn: { depth, skill, withheld }`); the launch preview counts the rules' characters.

5. **Starting.** The launch from *Learn* keeps *Start right away* (the fixed first message `tasks::KICKOFF`); LEARN-1
   then points Claude to the skill.

## Consequences

* A learning session's cost is bounded by the person's choice instead of Claude's plan; the fan-out is ruled out
  unless the person asks for it in the session.
* The procedure changes only with a NEXUS release. People who want another procedure can still give Claude their own
  instructions in the session; making the skill editable per workspace was considered and not chosen (it would need
  the approval flow of synced kits for every edit).
* NEXUS writes a small folder into its data directory per skill version. Old versions are not removed automatically.
* The Constitution Manager does not list LEARN-1 to LEARN-7: they are not workspace documents, and the Learn tab shows
  them where they apply.

## Alternatives considered

* **Only a line in the task brief** ("no subagents") — task text is untrusted data, not instructions (ADR-0020), and
  it would not carry a procedure or a budget.
* **The Prototype work profile for learning** — its text is about implementation work, and the person may want another
  profile for the session; the learning rules state what learning needs and override the profile only there.
* **A workspace skill from the templates** — editable and synced, but every edit needs approval again, and workspaces
  created from the minimal template would not have it.
