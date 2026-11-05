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
or review SQL or migrations; they refine DB.

## Queries

Correct, index-friendly SQL is mostly about avoiding a handful of well-known traps.

#### Use parameterised queries only
<!-- rule: id=SQL-1 enforcement=rule -->
Pass every value as a bind parameter ($1, ?, :name), including values in LIMIT, LIKE patterns and IN lists, and never
concatenate or interpolate values into SQL text. Dynamic identifiers come from an allowlist. The mandatory rule is
SEC-4.

#### Name target columns and avoid positional references
<!-- rule: id=SQL-2 enforcement=rule -->
Name the target columns in every INSERT (INSERT INTO invoice (id, customer_id, total) VALUES ...) and use column
names, not ordinal positions, in ORDER BY and GROUP BY, so statements keep working when columns are added or
reordered.

#### Keep predicates sargable
<!-- rule: id=SQL-3 enforcement=rule -->
Do not wrap indexed columns in functions, casts or arithmetic in WHERE and JOIN conditions (date(created_at) = :day);
use a range on the bare column (created_at >= :day AND created_at < :next_day) or a matching expression index. Avoid
leading-wildcard LIKE on big tables.

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
sequential scans, misestimated row counts and sorts spilling to disk. EXPLAIN ANALYZE executes the statement: wrap
writes in a transaction you roll back.

## Schema and DDL

Names, types and indexes are hard to change later, and DDL on busy tables can stop production.

#### Follow one naming convention
<!-- rule: id=SQL-7 enforcement=rule -->
Use lowercase snake_case for all identifiers, choose singular or plural table names once per database and keep it,
name keys consistently (id, `<table>_id`) and name constraints and indexes explicitly (fk_order_customer,
ix_order_created_at).

#### Index foreign keys and frequent predicates
<!-- rule: id=SQL-8 enforcement=rule -->
Index every foreign-key column used in joins or cascading deletes (PostgreSQL does not create these indexes for you)
and the columns of frequent WHERE and ORDER BY clauses, using composite or partial indexes that match the real query
shape.

#### Use time-zone-aware timestamp types
<!-- rule: id=SQL-9 enforcement=rule -->
Store instants as timestamptz in PostgreSQL or datetimeoffset in SQL Server; in MySQL store UTC in DATETIME with the
session time zone set to UTC. Never store dates or times as strings, and filter with half-open ranges.

#### Review migrations for locking impact
<!-- rule: id=SQL-10 enforcement=rule -->
Before merging DDL on a large or busy table, check the lock it takes: create indexes CONCURRENTLY (PostgreSQL) or with
online DDL (MySQL), add constraints NOT VALID and VALIDATE them separately, and set lock_timeout so a blocked
migration fails fast instead of queueing traffic.
