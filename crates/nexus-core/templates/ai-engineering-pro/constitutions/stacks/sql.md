---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: SQL
status: active
applies: on-demand
prefix: SQL
stacks: [sql]
created: {{now}}
updated: {{now}}
source: import
---

Rules for writing SQL queries and DDL in any relational database (examples use PostgreSQL syntax). Apply when you write
or review SQL or migrations; they refine DB, which holds schema design, migration process and transactions.

## Queries

Correct, index-friendly SQL is mostly about avoiding a handful of well-known traps.

#### Use parameterised queries only
<!-- rule: id=SQL-1 enforcement=rule -->
Pass every value as a bind parameter ($1, ?, :name), including LIMIT values, LIKE patterns (escape % and _ in user
input) and IN lists (= ANY($1) with an array), and take dynamic identifiers from an allowlist. The mandatory rule is
SEC-4.

#### Name columns explicitly
<!-- rule: id=SQL-2 enforcement=rule -->
List the columns each application query needs instead of SELECT *, name the target columns in every INSERT, and use
column names rather than ordinal positions in ORDER BY and GROUP BY, so added or reordered columns cannot break
mappers, change result shapes or leak new sensitive columns.

#### Keep predicates sargable
<!-- rule: id=SQL-3 enforcement=rule -->
Do not wrap indexed columns in functions, casts or arithmetic in WHERE and JOIN conditions (date(created_at) = :day);
use a half-open range on the bare column (created_at >= :day AND created_at < :next_day) or a matching expression
index. Avoid leading-wildcard LIKE on big tables.

#### Prefer set-based statements to row-by-row loops
<!-- rule: id=SQL-4 enforcement=rule -->
Express bulk changes as single set-based statements (UPDATE ... FROM, INSERT ... SELECT, MERGE or INSERT ... ON
CONFLICT) instead of looping row by row in application code or cursors; split very large changes into batches to
limit lock time and log volume.

#### Handle NULL explicitly
<!-- rule: id=SQL-5 enforcement=rule -->
NULL = NULL is unknown, not true: use IS [NOT] NULL or IS [NOT] DISTINCT FROM, replace NOT IN over a nullable subquery
with NOT EXISTS, and use COALESCE only where the default is semantically right. COUNT(col) skips NULLs; COUNT(*) does
not.

#### Check the plan of new queries on large tables
<!-- rule: id=SQL-6 enforcement=rule -->
Run EXPLAIN (ANALYZE, BUFFERS) on representative data for new or changed queries on large tables and look for
sequential scans, misestimated row counts and sorts spilling to disk. ANALYZE executes the statement: use a
non-production database and wrap writes in a transaction you roll back.

#### Paginate large tables with a keyset, not OFFSET
<!-- rule: id=SQL-7 enforcement=rule -->
Page on a deterministic order that ends in a unique column, with a row comparison backed by a matching index
(WHERE (created_at, id) > ($1, $2) ORDER BY created_at, id LIMIT $3). OFFSET reads and discards every skipped row
and shifts under concurrent inserts.

## Schema and DDL

Names, types and indexes are hard to change later, and DDL on busy tables can stop production.

#### Follow one naming convention
<!-- rule: id=SQL-8 enforcement=rule -->
Keep the database's existing convention. For new schemas use lowercase snake_case identifiers, choose singular or
plural table names once, name keys consistently (id, `<table>_id`) and name constraints and indexes explicitly
(fk_order_customer, ix_order_created_at).

#### Review migrations for locking impact
<!-- rule: id=SQL-9 enforcement=rule -->
Before merging DDL on a large or busy table, check the lock it takes: create indexes CONCURRENTLY (PostgreSQL) or with
online DDL (MySQL), add constraints NOT VALID and VALIDATE them separately, and set lock_timeout so a blocked
migration fails fast instead of queueing traffic.
