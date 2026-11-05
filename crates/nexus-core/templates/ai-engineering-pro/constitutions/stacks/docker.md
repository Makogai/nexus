---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Docker
status: active
applies: on-demand
prefix: DKR
stacks: [docker]
created: {{now}}
updated: {{now}}
source: import
---

Rules for writing Dockerfiles and Compose files. Apply when you create or change them; they refine OPS, which holds
the build, secret and deployment policies.

## Base images and layers

The base image and layer order decide image size, patch level and build speed.

#### Pin base images by version and digest
<!-- rule: id=DKR-1 enforcement=rule -->
Reference base images by a specific version tag (node:24.11-slim, not node or node:latest) and pin production images
by digest as well (`image:tag@sha256:<digest>`), letting an update bot such as Renovate raise tag and digest together.

#### Use multi-stage builds with named stages
<!-- rule: id=DKR-2 enforcement=rule -->
Name stages (FROM node:24.11-slim AS build), copy only built artefacts into the final stage with COPY --from=build,
and keep test and dev tooling in separate stages selected with --target, so one Dockerfile serves CI and production.

#### Order layers for cache efficiency
<!-- rule: id=DKR-3 enforcement=rule -->
Copy dependency manifests and lockfiles first, install dependencies, then copy the source, so code changes do not
invalidate the dependency layer. Use RUN --mount=type=cache for package caches, and run apt-get update, install and
cleanup in one RUN.

#### Copy only what the image needs
<!-- rule: id=DKR-4 enforcement=rule -->
Use COPY, not ADD, which also fetches URLs and auto-extracts archives; copy explicit paths instead of the whole
context, and keep .dockerignore in step, ideally allowlist-style (exclude everything with * and re-include needed paths
with !).

## Runtime

How the container process runs decides its privileges and whether it stops cleanly.

#### Run as a non-root user with a numeric UID
<!-- rule: id=DKR-5 enforcement=rule -->
Switch to an unprivileged user with a fixed numeric UID (USER 10001, or the image's built-in user such as node) before
CMD, and set file ownership with COPY --chown. Numeric UIDs let orchestrators verify runAsNonRoot.

#### Run one concern per container in exec form
<!-- rule: id=DKR-6 enforcement=rule -->
Run one main process per container (web, worker and scheduler as separate services from the same image) instead of
supervisord bundles. Use exec-form CMD or ENTRYPOINT (JSON array) so the process is PID 1 and gets SIGTERM; add --init
or tini if it spawns children.

#### Declare a HEALTHCHECK that works inside the image
<!-- rule: id=DKR-7 enforcement=rule -->
Add a HEALTHCHECK with interval, timeout, start period and retries for images run by Docker or Compose, using a
command that exists in the final image (an app health subcommand on distroless, not a missing curl). Kubernetes
ignores it and uses probes (OPS-7).

#### Pass build secrets with BuildKit secret mounts
<!-- rule: id=DKR-8 enforcement=rule -->
Provide build-time credentials (registry or package tokens) with RUN --mount=type=secret,id=npmrc,target=... and
docker build --secret, never through ARG, ENV or COPY: build args and env values persist in image history and layers
(OPS-8).

## Compose and tooling

Compose makes local dependencies reproducible; a Dockerfile linter catches what review misses.

#### Use Compose for local development with named volumes
<!-- rule: id=DKR-9 enforcement=rule -->
Define local dependencies (database, cache, broker) in compose.yaml with pinned images, named volumes for data,
healthchecks with depends_on condition: service_healthy, and an uncommitted env file next to a committed
.env.example; keep production settings out of it.

#### Lint Dockerfiles with hadolint
<!-- rule: id=DKR-10 enforcement=rule check=lint -->
Run hadolint on every Dockerfile in CI and fix findings (unpinned packages, missing --no-install-recommends,
shell-form CMD) rather than ignoring them; document any inline ignore with the reason.
