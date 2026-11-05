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

Rules for builds, CI/CD, running containers, infrastructure and deployments. Apply when you change build or CI
configuration, infrastructure code, deployment manifests or release procedures; DKR covers writing Dockerfiles and
Compose files.

## Builds and CI

A change is only trustworthy when an automated, repeatable pipeline has built and checked it.

#### Build reproducibly from a clean checkout
<!-- rule: id=OPS-1 enforcement=rule -->
A fresh clone plus the documented prerequisites must build, test and package the project with one command, without
manual steps or files from a developer machine. Pin runtime and tool versions in version files (.nvmrc,
.python-version, rust-toolchain.toml) or the build image, and generate everything else during the build.

#### Gate every change in CI
<!-- rule: id=OPS-2 enforcement=rule -->
Where the project has CI, it builds, lints and tests every push and pull request and blocks merging when a step
fails. Never mark a failing step as allowed to fail or skip it to get a change through; fix the cause, or quarantine
a flaky test as TST describes.

#### Give CI least-privilege, short-lived credentials
<!-- rule: id=OPS-3 enforcement=rule -->
Make the CI token read-only by default and grant write scopes per job, get cloud access through OIDC federation
instead of stored long-lived keys, bind deploy credentials to protected branches and environments, and never expose
secrets to jobs that run code from forks or untrusted pull requests.

## Running containers

What a container may do at runtime decides how far an attacker or a bug can reach.

#### Run containers with least privilege
<!-- rule: id=OPS-4 enforcement=rule -->
Run production containers as non-root (runAsNonRoot), drop all Linux capabilities and add back only what is needed,
use a read-only root filesystem where the application allows it, and never run them privileged or mount the Docker
socket: root in a container is one kernel bug away from root on the host.

#### Rebuild and scan images regularly
<!-- rule: id=OPS-5 enforcement=rule -->
Rebuild production images at least weekly and when relevant CVEs are published, even without code changes, so OS
patches reach production. Scan images in CI and fail on fixable critical vulnerabilities.

#### Give every service a health check
<!-- rule: id=OPS-6 enforcement=rule -->
Point the platform's checks (Kubernetes liveness, readiness and startup probes, or a Docker HEALTHCHECK) at the
service's liveness and readiness endpoints (BE), with timeouts and thresholds tuned so slow starts are not killed and
hung instances are.

## Deployment

Every deploy must be traceable to an exact artefact and reversible without improvisation.

#### Deploy immutable image digests, never latest
<!-- rule: id=OPS-7 enforcement=rule -->
Tag images with the commit SHA, promote the same digest from staging to production and reference deployments by
digest. Never deploy a mutable tag such as latest: you cannot tell what is running or reliably roll back to it.

#### Manage infrastructure as reviewed code
<!-- rule: id=OPS-8 enforcement=rule -->
Define infrastructure (networks, databases, DNS, IAM, clusters) in code such as Terraform/OpenTofu, Pulumi or
Kubernetes manifests, review each change with its plan or diff in the pull request and apply it from CI. Manual
console changes are drift: import or revert them.

#### Change shared environments only when the person asks
<!-- rule: id=OPS-9 enforcement=mandatory -->
Never run deploys, terraform/tofu apply or destroy, kubectl apply or delete, helm upgrade or image pushes against
shared or production environments unless the person explicitly asks in this session. Otherwise prepare the change
with its plan or diff for review.

#### Keep environments at parity
<!-- rule: id=OPS-10 enforcement=rule -->
Run the same images, configuration structure and backing-service types and versions in development, staging and
production, differing only in scale and configuration values, so a passing staging deploy predicts production.

#### Log to stdout and alert on symptoms
<!-- rule: id=OPS-11 enforcement=rule -->
Write logs to stdout/stderr, not to files inside the container, and ship them with metrics to a central system with
retention. Alert on user-facing symptoms (error rate, latency, saturation), not only on host metrics; what to log and
trace follows ARC.

#### Have a rollback plan for every deploy
<!-- rule: id=OPS-12 enforcement=rule -->
Before deploying, know how to return to the previous state (redeploy the previous digest, switch a feature flag off,
or roll forward with a prepared fix) and keep the release's migrations backward compatible (DB expand/contract), so
rolling back code is safe.
