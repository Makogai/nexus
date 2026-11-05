---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Database engineering
status: active
applies: on-demand
prefix: DB
stacks: [sql]
created: {{now}}
updated: {{now}}
source: import
---

Rules for relational schema design, migrations, queries, transactions, backups and personal data. Apply when you
change a schema, write a migration or data-access code, or touch backups and stored personal data.

## Migrations

Schema changes are the riskiest part of a deploy: they run against live data and are hard to undo.

#### Version every schema change as a reviewed migration
<!-- rule: id=DB-1 enforcement=rule -->
Make every schema and reference-data change a versioned migration in the repository, applied by the migration tool in
every environment. Never alter a shared database by hand; review migrations like code, because they run against
production data.

#### Keep applied migrations immutable and forward-only
<!-- rule: id=DB-2 enforcement=rule -->
Never edit, reorder or delete a migration that has run in any shared environment; correct mistakes with a new
migration. Rewriting history makes environments diverge silently and breaks the migration tool's checksums.

#### Never run a destructive migration without backup and rollback plan
<!-- rule: id=DB-3 enforcement=mandatory -->
Before any DROP, TRUNCATE, column removal, type narrowing or other irreversible data change, confirm a verified,
restorable backup of the affected data and a written rollback plan. If either is missing, stop and ask; never run it
as a quick fix.

#### Change schemas with expand/contract for zero downtime
<!-- rule: id=DB-4 enforcement=rule -->
Add the new column or table first (expand), deploy code that writes both and reads the new one, backfill in batches,
then remove the old structure in a later release (contract). Every intermediate state must work with both the old and
the new application version.

## Schema and integrity

The database is the last line of defence for correct data; constraints hold even when application code is wrong.

#### Enforce invariants with constraints in the schema
<!-- rule: id=DB-5 enforcement=rule -->
Declare NOT NULL, primary keys, foreign keys, UNIQUE and CHECK constraints for every invariant the data must satisfy
instead of relying on application validation alone; constraints also catch concurrent writes, bugs and other clients.

#### Store timestamps in UTC
<!-- rule: id=DB-6 enforcement=rule -->
Store instants in UTC, using a time-zone-aware type where the database has one, and convert to local time only for
display. Keep a separate IANA time-zone column when the user's local wall-clock time matters, e.g. for scheduling.

#### Store money as exact decimals or integer minor units
<!-- rule: id=DB-7 enforcement=rule -->
Store amounts as DECIMAL/NUMERIC with a fixed scale or as integer minor units (cents), always with an ISO 4217
currency code. Never use FLOAT, REAL or DOUBLE for money: binary floating point cannot represent most decimal
fractions and rounding errors accumulate.

## Queries and performance

Most database incidents are slow queries and lock contention that only appear at production data volumes.

#### Design indexes from query patterns and verify them with EXPLAIN
<!-- rule: id=DB-8 enforcement=rule -->
Index the predicates, joins and sorts the application actually runs, ordering composite columns equality-first, then
range. Confirm with EXPLAIN on representative data that the plan uses the index, and drop unused indexes, since every
index slows writes.

#### Never SELECT * in application queries
<!-- rule: id=DB-9 enforcement=rule -->
List the columns each query needs. `SELECT *` transfers unneeded data, prevents index-only scans and silently changes
the result shape when columns are added, which breaks mappers and leaks new sensitive columns.

#### Avoid N+1 queries
<!-- rule: id=DB-10 enforcement=rule -->
Load related rows with a join, a batched IN/ANY query or the ORM's eager loading instead of one query per parent row.
Check query counts for list endpoints in tests or logs; N+1 patterns stay invisible with small test data.

#### Paginate large result sets with keyset pagination
<!-- rule: id=DB-11 enforcement=rule -->
For large or growing tables page with a keyset (`WHERE (created_at, id) > (:last_ts, :last_id) ORDER BY created_at,
id LIMIT :n`) instead of OFFSET, which reads and discards every skipped row and shifts under concurrent inserts.
Always bound the page size.

## Transactions and connections

Correctness under concurrency and stability under load depend on how transactions and connections are used.

#### Use short explicit transactions and know the isolation level
<!-- rule: id=DB-12 enforcement=rule -->
Wrap each multi-statement unit of work in one short explicit transaction and never hold it across network calls or
user input. Know the default isolation level (often READ COMMITTED) and prevent lost updates with row locks, version
columns or SERIALIZABLE with retries.

#### Pool connections and set statement timeouts
<!-- rule: id=DB-13 enforcement=rule -->
Connect through a bounded pool sized against the database's connection limit across all instances, and set statement
and lock timeouts (e.g. PostgreSQL statement_timeout and lock_timeout) so one runaway query cannot exhaust connections
or block a table.

## Data protection and recovery

Stored personal data is a liability, and a backup is worth only what its last successful restore proves.

#### Minimise and protect personal data
<!-- rule: id=DB-14 enforcement=rule -->
Store only the personal data a feature needs, mark those columns, encrypt sensitive ones at rest where the threat model
requires it and restrict access by role. Implement the retention period with deletion or anonymisation, and never copy
production personal data into dev or test databases.

#### Test backup restores on a schedule
<!-- rule: id=DB-15 enforcement=rule -->
Treat a backup as valid only after a restore succeeded: restore regularly into an isolated environment, run integrity
and row-count checks, and record the achieved recovery time and recovery point against the agreed RTO and RPO.
