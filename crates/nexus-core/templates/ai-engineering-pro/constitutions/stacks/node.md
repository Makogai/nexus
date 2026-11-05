---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Node.js
status: active
applies: on-demand
prefix: NODE
stacks: [node, nestjs, express]
created: {{now}}
updated: {{now}}
source: import
---

Rules for the Node.js runtime in servers, workers and CLIs, including NestJS and Express. Apply to Node.js code; they
refine BE and TS.

## Runtime and modules

A pinned runtime, one module system and the platform's own APIs keep Node projects small and identical everywhere.

#### Pin a supported LTS version
<!-- rule: id=NODE-1 enforcement=rule -->
Run on a Node.js LTS line in active or maintenance support, declare it in package.json engines and in .nvmrc or
.node-version, and use the same version in CI and the container image. Plan the upgrade before the line reaches end of
life.

#### Use one module system, preferably ESM
<!-- rule: id=NODE-2 enforcement=rule -->
Set "type": "module" for new packages and use import syntax with explicit file extensions in relative imports; do not
mix require and import within a package. Import built-ins with the node: prefix (import { readFile } from
'node:fs/promises').

#### Prefer built-in Node APIs to new dependencies
<!-- rule: id=NODE-3 enforcement=guideline -->
Before adding a package for new code, use what the LTS runtime ships: global fetch with AbortSignal.timeout(),
node:test with node --test, --watch, --env-file, util.parseArgs, crypto.randomUUID() and fs/promises. Keep the
libraries a project already uses.

## Asynchronous code

Node runs all JavaScript on one thread; lost promises and blocking work affect every request at once.

#### Never leave a promise floating
<!-- rule: id=NODE-4 enforcement=rule check=lint -->
Await or return every promise, or handle it explicitly with .catch(); enable @typescript-eslint/no-floating-promises
and no-misused-promises. An unhandled rejection terminates the process by default, and a missing await loses errors
and ordering.

#### Keep synchronous and CPU-heavy work off request paths
<!-- rule: id=NODE-5 enforcement=rule -->
Do not call synchronous fs, zlib or crypto APIs (readFileSync, pbkdf2Sync) or run CPU-heavy work (large JSON parsing,
image processing) on request paths; use the async APIs, worker_threads (e.g. a Piscina pool) or a background job. One
blocked tick stalls every concurrent request.

#### Stream large data
<!-- rule: id=NODE-6 enforcement=rule -->
Process large files, uploads and responses as streams with pipeline() from node:stream/promises, which propagates
errors and backpressure and destroys streams on failure, instead of buffering them in memory or chaining .pipe()
without error handling.

## Process and operations

A Node service must start with valid configuration and stop without dropping work.

#### Read process.env in one validated config module
<!-- rule: id=NODE-7 enforcement=rule -->
Read process.env only in one config module that validates and types it at startup and exits with a clear message when
a value is missing; everything else imports that config. For local development load a git-ignored .env with node
--env-file instead of adding dotenv.

#### Shut down gracefully on SIGTERM
<!-- rule: id=NODE-8 enforcement=rule -->
On SIGTERM and SIGINT call server.close() and server.closeIdleConnections(), let in-flight work finish, close pools,
then exit, forcing exit after a timeout. Start node directly in containers, not via npm start, so signals reach the
process.

#### Spawn processes without a shell
<!-- rule: id=NODE-9 enforcement=rule -->
Use execFile or spawn with an argument array and without shell: true; never build a command string for exec() from
input, because any unescaped character becomes command injection. The mandatory rule is SEC-4.

#### Log with a structured logger
<!-- rule: id=NODE-10 enforcement=rule -->
Log through the project's structured JSON logger (pino for new services) with a child logger per request carrying the
request id and redaction for sensitive fields; no console.log in server code. Pretty-print only in development.
