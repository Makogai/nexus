# ADR-0014 — Session hosts and lifecycle (prepares an explicit background-service mode)

Status: Accepted (2026-11-04). Implemented: host registry, leases, reconciliation, safe shutdown.
Not implemented (by design): any background service.

## Context
Today the desktop process owns the PTYs of the sessions it starts. The CLI (`nexus launch`) will start sessions in
the user's own terminal. The owner wants a later, *explicit* managed background-service mode in which sessions can
outlive the window. Reconciliation must therefore stop assuming "every live session belongs to the desktop" and must
detect dead owners without guessing from PIDs (PID reuse, cross-platform differences).

## Decision
* A **session host** is a process that owns session child processes. Kinds: `desktop`, `cli`, later `service`.
* Every host instance registers a row in `session_hosts(id, kind, pid, version, started_at, heartbeat_at, stopped_at)`
  and every session records the `host_id` that owns it.
* **Liveness is a lease**: a host refreshes `heartbeat_at` every 15 s (one tiny UPDATE). A host is *dead* when
  `stopped_at` is set or its heartbeat is older than 60 s.
* **Reconciliation** (on every host start and every heartbeat of the desktop): sessions in `starting`/`running`
  whose host is dead become `interrupted`. A starting desktop additionally closes its own previous instances
  (single instance per machine).
* **Ownership**: only the owning host may write to, resize or terminate a session's process. Other hosts see
  metadata only (the desktop lists a CLI session as running, without a terminal to attach).
* **Safe shutdown**: a host terminates only its own children, records their sessions as `interrupted` (closed by
  shutdown) unless they already exited, then marks itself stopped. The desktop asks for confirmation when live
  sessions exist. Nothing is ever killed by PID or name.
* **Future service mode** (not built): an opt-in `service` host started/stopped explicitly from the UI/CLI, owning
  PTYs and offering attach over an OS-access-controlled local channel (named pipe / Unix socket). The data model and
  reconciliation rules above already accommodate it; adding it needs no schema change.

## Alternatives
* PID liveness checks — unreliable with PID reuse and requires platform-specific (unsafe) process APIs.
* A daemon from day one — always-on infrastructure the owner explicitly does not want yet.

## Consequences
* A crashed CLI's sessions become `interrupted` within ~60 s of the next reconciliation.
* Cost: one UPDATE per host every 15 s.
