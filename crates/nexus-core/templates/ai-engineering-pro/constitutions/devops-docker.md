---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: DevOps and Docker
status: active
applies: on-demand
prefix: OPS
stacks: [docker]
created: {{now}}
updated: {{now}}
source: import
---

Rules for builds, CI/CD, container images, infrastructure and deployments. Apply when you change build or CI
configuration, Dockerfiles, infrastructure code or release procedures; DKR refines the Dockerfile details.

## Builds and CI

A change is only trustworthy when an automated, repeatable pipeline has built and checked it.

#### Build reproducibly from a clean checkout
<!-- rule: id=OPS-1 enforcement=rule -->
A fresh clone plus the documented prerequisites must build, test and package the project with one command, without
manual steps or files from a developer machine. Commit lockfiles and generate everything else during the build.

#### Run build, lint and tests in CI on every change
<!-- rule: id=OPS-2 enforcement=rule check=build -->
CI builds the project and runs lint and the test suite on every push and pull request, and merging is blocked when
any step fails. Never mark a failing step as allowed to fail; fix it, or quarantine the test with an issue and an
owner.

#### Pin tool and base-image versions
<!-- rule: id=OPS-3 enforcement=rule -->
Pin runtimes, build tools, CI actions and base images to exact versions (third-party CI actions by commit SHA,
production images by digest) and update them deliberately through automated update pull requests such as Renovate or
Dependabot.

## Container images

An image is the unit you ship; what is inside it is what runs, and what can be attacked.

#### Use multi-stage builds
<!-- rule: id=OPS-4 enforcement=rule -->
Compile and install build dependencies in a builder stage and copy only the runtime artefacts into a minimal final
stage, so compilers, package caches, tests and source code never ship in the production image.

#### Run containers as a non-root user
<!-- rule: id=OPS-5 enforcement=rule -->
Run every production container as a non-root user with a fixed UID, without added Linux capabilities and with a
read-only root filesystem where the application allows it; root inside a container is one kernel bug away from root
on the host.

#### Use minimal base images and rebuild them regularly
<!-- rule: id=OPS-6 enforcement=rule -->
Base production images on slim or distroless variants, rebuild them at least weekly and when relevant CVEs are
published, even without code changes, so OS patches reach production. Scan images in CI and fail on fixable critical
vulnerabilities.

#### Give every service a health check
<!-- rule: id=OPS-7 enforcement=rule -->
Define the health check the platform uses (Kubernetes liveness, readiness and startup probes, or a Docker HEALTHCHECK)
against the endpoints described in BE, with timeouts and thresholds tuned so slow starts are not killed and hung
instances are.

#### Never bake secrets into images or commit env files with values
<!-- rule: id=OPS-8 enforcement=mandatory -->
Secrets never go into Dockerfiles, image layers, build args, compose files or committed .env files; commit only an
.env.example with placeholders. Inject secrets at runtime from the platform's secret store, and rotate any secret that
reached an image or Git history.

#### Keep the build context small with .dockerignore
<!-- rule: id=OPS-9 enforcement=rule -->
Add a .dockerignore that excludes .git, dependency folders, build output, local env files and test data, so builds
stay fast, layer caching is reliable and nothing private is sent to the builder.

## Deployment

Every deploy must be traceable to an exact artefact and reversible without improvisation.

#### Deploy immutable image digests, never latest
<!-- rule: id=OPS-10 enforcement=rule -->
Tag images with the commit SHA, promote the same digest from staging to production and reference deployments by
digest. Never deploy a mutable tag such as latest: you cannot tell what is running or reliably roll back to it.

#### Manage infrastructure as reviewed code
<!-- rule: id=OPS-11 enforcement=rule -->
Define infrastructure (networks, databases, DNS, IAM, clusters) in code such as Terraform/OpenTofu, Pulumi or
Kubernetes manifests, review each change with its plan or diff in the pull request and apply it from CI. Manual
console changes are drift: import or revert them.

#### Keep environments at parity
<!-- rule: id=OPS-12 enforcement=rule -->
Run the same images, configuration structure and backing-service types and versions in development, staging and
production, differing only in scale and configuration values, so a passing staging deploy predicts production.

#### Centralise logs and metrics
<!-- rule: id=OPS-13 enforcement=rule -->
Write logs to stdout/stderr, not files inside the container, and ship them with service metrics to a central system
with retention and alerting. Alert on user-facing symptoms (error rate, latency, saturation), not only on host
metrics.

#### Have a rollback plan for every deploy
<!-- rule: id=OPS-14 enforcement=rule -->
Before deploying, know how to return to the previous state (redeploy the previous digest, switch a feature flag off,
or roll forward with a prepared fix) and make sure the release's migrations are backward compatible (see DB), so
rolling back code is safe.

#### Give CI least-privilege, short-lived credentials
<!-- rule: id=OPS-15 enforcement=rule -->
Grant each CI job only the permissions it needs (read-only by default) and use OIDC federation to obtain short-lived
cloud credentials instead of long-lived keys stored as CI secrets. Restrict deploy credentials to protected branches
and environments.
