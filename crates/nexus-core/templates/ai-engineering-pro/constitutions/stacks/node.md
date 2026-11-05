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

A pinned runtime, one module system and a frozen dependency tree make builds identical everywhere.

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

#### Commit the lockfile and install with npm ci
<!-- rule: id=NODE-3 enforcement=rule -->
Commit package-lock.json (or the pnpm or Yarn lockfile) and install in CI and images with npm ci or the equivalent
frozen-lockfile command, so builds use exactly the reviewed dependency tree and fail when the lockfile is out of date.

## Asynchronous code

Node runs all JavaScript on one thread; lost promises and blocking work affect every request at once.

#### Never leave a promise floating
<!-- rule: id=NODE-4 enforcement=rule check=lint -->
Await or return every promise, or handle it explicitly with .catch(); enable @typescript-eslint/no-floating-promises
and no-misused-promises. An unhandled rejection terminates the process by default, and a missing await loses errors
and ordering.

#### Never block the event loop
<!-- rule: id=NODE-5 enforcement=rule -->
Keep CPU-heavy work (hashing or compressing large data, image processing, huge JSON parsing) and synchronous fs or
crypto calls off the request path; move it to worker_threads (e.g. a Piscina pool) or a background job. One blocked
tick stalls every concurrent request.

#### Stream large data
<!-- rule: id=NODE-6 enforcement=rule -->
Process large files, uploads and responses as streams with pipeline() from node:stream/promises, which propagates
errors and backpressure and destroys streams on failure, instead of buffering them in memory or chaining .pipe()
without error handling.

## Process and operations

A Node service must start with valid configuration and stop without dropping work.

#### Validate configuration at startup
<!-- rule: id=NODE-7 enforcement=rule -->
Read process.env once in a config module that validates and types every variable with a schema at startup and exits
with a clear message when one is missing or invalid; the rest of the code imports the typed config instead of reading
process.env.

#### Shut down gracefully on SIGTERM
<!-- rule: id=NODE-8 enforcement=rule -->
Handle SIGTERM and SIGINT: stop accepting connections with server.close(), close idle keep-alive connections, finish
in-flight work, close pools, then exit, forcing exit after a timeout. Start node directly in containers, not via npm
start, so signals reach the process.

#### Spawn processes without a shell
<!-- rule: id=NODE-9 enforcement=rule -->
Use execFile or spawn with an argument array and without shell: true; never build a command string for exec() from
input, because any unescaped character becomes command injection. The mandatory rule is SEC-4.

#### Log with a structured logger
<!-- rule: id=NODE-10 enforcement=rule -->
Log through a structured JSON logger such as pino, with a child logger per request carrying the request id and
redaction configured for sensitive fields; do not use console.log in server code. Pretty-print only in development.
