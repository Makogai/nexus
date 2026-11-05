---
nexus: 1
id: {{id}}
kind: rule
format: constitution
title: Rust
status: active
applies: on-demand
prefix: RS
stacks: [rust, tauri]
created: {{now}}
updated: {{now}}
source: import
---

Rules for Rust libraries and binaries, including Tauri backends. Apply to Rust code; they refine CQ and TST.

## Project setup

Edition, MSRV, lockfile and features decide whether a crate builds the same way for everyone.

#### Use edition 2024 and declare an MSRV
<!-- rule: id=RS-1 enforcement=rule -->
Set edition = "2024" and rust-version (the MSRV) in every Cargo.toml, build the MSRV in CI and raise it deliberately.
With resolver 3, the edition 2024 default, Cargo then prefers dependency versions compatible with that MSRV.

#### Commit Cargo.lock and build with --locked
<!-- rule: id=RS-2 enforcement=rule -->
Commit Cargo.lock (Cargo's current default for libraries as well as binaries) so builds are reproducible, and build in
CI with --locked so an outdated lockfile fails the build instead of being rewritten silently.

#### Keep Cargo features additive
<!-- rule: id=RS-3 enforcement=rule -->
Design features so enabling one only adds functionality and never removes or changes behaviour, because Cargo unifies
features across the dependency graph; avoid mutually exclusive features and test with default features off and all
features on.

## Code

Rust's guarantees hold only when errors are handled and unsafe code is contained.

#### Do not unwrap recoverable errors in library code
<!-- rule: id=RS-4 enforcement=rule -->
Return Result for anything that can fail at runtime (I/O, parsing, input) and propagate with ?. Reserve unwrap and
expect for invariants the code guarantees, with expect stating the invariant; consider denying clippy::unwrap_used in
library crates.

#### Use typed errors in libraries and contextual errors in binaries
<!-- rule: id=RS-5 enforcement=rule -->
Define error enums (e.g. with thiserror) in library crates so callers can match on causes; in binaries use the
project's error crate (e.g. anyhow) and add context at each layer with .context(...). Do not return String or
`Box<dyn Error>` errors from public library APIs.

#### Deny unsafe code unless audited and documented
<!-- rule: id=RS-6 enforcement=mandatory -->
Set unsafe_code = "forbid" or "deny" under `[lints.rust]` in Cargo.toml. Where unsafe is unavoidable, confine it to a
small module with an explicit allow, a // SAFETY: comment on every block stating the invariants, tests (Miri where
possible) and a reviewer sign-off.

#### Borrow instead of cloning in hot paths
<!-- rule: id=RS-7 enforcement=guideline -->
Take &str, &[T] or impl AsRef parameters and borrow data rather than calling clone() to satisfy the borrow checker; in
hot paths each clone of a String, Vec or map is an allocation. Use Cow or Arc when ownership is genuinely shared.

## Tooling

clippy, rustfmt and cargo test are the standard gates for every Rust change.

#### Keep rustfmt and clippy clean with warnings denied
<!-- rule: id=RS-8 enforcement=rule check=lint -->
Run cargo fmt --all --check and cargo clippy --all-targets --all-features -- -D warnings in CI and fix findings rather
than allowing them. When an exception is justified, put #[expect(clippy::name, reason = "...")] on the smallest item;
keep rustfmt.toml minimal.

#### Test with cargo test, including doc tests
<!-- rule: id=RS-9 enforcement=rule check=test -->
Put unit tests in #[cfg(test)] modules next to the code, integration tests in tests/ and runnable examples in doc
comments, and run cargo test --workspace so doc tests execute too. cargo-nextest skips doc tests: add cargo test --doc
when you use it.
