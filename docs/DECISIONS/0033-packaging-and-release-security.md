# ADR-0033 — Packaging, installed layout, installed-app verification and release security

Status: Accepted, implemented (2026-11-05, stage "Beta readiness"); signing and the updater are open decisions

## Context
The desktop starts `nexus mcp` (MCP server) and `nexus hook` (lifecycle hooks) for every Claude session from *next to
its own program*. An installed NEXUS must therefore ship the CLI, find it without PATH, keep program files apart from
user data, never contain the test-only WebView2 debugging feature, and produce artifacts whose integrity can be checked.

## Decision
* **Installer**: NSIS only on Windows (`tauri.windows.conf.json`), per-user (`installMode: currentUser`, no admin).
  The CLI is a Tauri `externalBin` sidecar added only by the installer configuration (`tauri.installer.conf.json`,
  `--config`), staged by `scripts/stage-sidecar.mjs` (built with the same profile, `--locked`) — development and test
  builds are unaffected. Installed layout: `nexus-desktop.exe` (fixed `mainBinaryName`: the product name `NEXUS.exe`
  would be the same file as `nexus.exe` on Windows), `nexus.exe`, `uninstall.exe` — nothing else.
* **Program and data apart**: Tauri's per-user default `%LOCALAPPDATA%\NEXUS` is NEXUS's data folder, so the installer
  hook redirects the default to `%LOCALAPPDATA%\Programs\NEXUS` (and removes the default folder it created, only if
  empty). Uninstall deletes the installed files, the Windows entry, the Start Menu shortcut and NEXUS's own
  registrations (notification sender, `nexus://`); the data folder is never touched.
* **Launch paths**: sessions get the CLI by its full path (MCP config and hook settings, exec form — spaces are safe);
  the shell-launched `nexus://` handler is registered without the `\\?\` prefix canonical paths carry (Windows Shell
  cannot launch those — a click after a restart did nothing before this fix).
* **Self-check**: `nexus-desktop.exe --self-check <report.json>` verifies an installed app without a window or
  developer tools — CLI found next to it and runnable, debugging feature absent and WebView2 overrides dropped, data
  folder and schema compatibility (read-only), notification identity. Settings shows the CLI path; NEXUS never edits
  PATH (it copies a command for the person).
* **Release security**: `#[cfg(all(feature = "e2e-webview-debugging", not(debug_assertions)))] compile_error!` —
  a release build with the test feature cannot exist; T18 runs against the release program; `scripts/release-check.mjs`
  checks the configuration (feature off by default, CSP, no global Tauri object, only the CLI bundled, fixed binary
  name, source maps only in debug builds), the embedded frontend (`dist/` not rebuilt since the installer — a later
  debug build writes source maps into it; no source maps, credentials or machine paths) and writes `SHA256SUMS.txt`.
* **CI**: `ci.yml` builds the installer and runs the installed-app test on Windows; `release.yml` (tags `v*` or manual)
  builds the release installer, runs T18, the installed-app test and the release check, attests build provenance,
  uploads the artifacts and creates a **draft** release — never published automatically. Code signing and the updater
  are wired as explicit, disabled steps until the owner decides (certificate; update signing key).

## Verification
`e2e/installed.mjs` against the real installer (production phase, 27 checks: layout, nothing extra, Windows entry,
shortcut, CLI without PATH, self-check, sessions get the installed CLI as MCP server and hook handler, MCP handshake,
T18 in the installed app, first-launch backup, toast under NEXUS without paths or ids, `nexus://` click reaches the
app, uninstall removes programs and registrations, data kept, no installer state left behind) and against a separately built *test* installer
(`--ui`, 18 checks: first-run screen, terminal, synthetic profile launch). `release-check.mjs` on the release bundle.

## Known installer behaviour (Tauri NSIS template)
* The installer reads its options (`/R` restart the app, `/NS` no shortcut, `/P` passive, `/UPDATE`) from the whole
  command line **including its own path**, case-insensitively. Started with a forward-slash path such as
  `../target/release/bundle/nsis/…-setup.exe`, `/release` reads as `/R` (the app is started after installing — without
  the test's data folder) and `/nsis` as `/NS` (no Start Menu shortcut). Double-clicking in Explorer passes a backslash
  path and is unaffected. Scripts and CI therefore always start the installer by an absolute Windows path
  (`e2e/installed.mjs` resolves it); the installer environment carries a temporary `NEXUS_DATA_DIR` as a second guard.
* The last install location is remembered under `HKCU\Software\<manufacturer>\NEXUS` and kept after uninstalling; the
  next install reuses it. The installed-app test refuses to start while that key exists and removes it at the end.

## Alternatives
* MSI (WiX) — needs elevation for per-user installs to be clean, adds tooling; NSIS covers a personal beta.
* CLI on PATH by the installer — changes the person's environment; a copyable command is enough.
* A custom NSIS template — more control, more maintenance; one hook redirect suffices.

## Consequences
* Unsigned installers trigger SmartScreen warnings until code signing is decided.
* The first installer build downloads NSIS tooling from Tauri's binary releases.
