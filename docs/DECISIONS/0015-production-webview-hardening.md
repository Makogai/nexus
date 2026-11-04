# ADR-0015 — Production webview hardening (threat T18)

Status: Accepted, implemented (2026-11-04)

## Context
WebView2 reads `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` and related variables from the process environment, and they
override the options an application passes. An inherited `--remote-debugging-port` therefore opens a DevTools
endpoint on localhost that any local process — including other users' — can use to drive NEXUS's privileged IPC
(start processes in PTYs, read workspace data). WebKitGTK has `WEBKIT_INSPECTOR_SERVER`. Machine/user WebView2 policies
in the registry can inject the same switches. The development E2E tooling, however, relies on exactly this mechanism.

## Decision
* `main` calls `webview_guard::harden()` before anything else. In **production** builds (embedded frontend, no test
  feature) it removes every webview override variable (`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`,
  `…_BROWSER_EXECUTABLE_FOLDER`, `…_USER_DATA_FOLDER`, `…_RELEASE_CHANNEL_PREFERENCE`, `…_CHANNEL_SEARCH_KIND`,
  `…_PIPE_FOR_SCRIPT_DEBUGGER`, `WEBKIT_INSPECTOR_SERVER`, `WEBKIT_INSPECTOR_HTTP_SERVER`) from the process environment,
  matching names case-insensitively on raw OS strings. Removed names (never values) are logged.
* If a WebView2 `AdditionalBrowserArguments` policy (HKLM or HKCU, for this executable or `*`) contains a
  debugging switch, NEXUS **refuses to start** with a native error naming the policy (fail closed). Other
  administrator-chosen arguments (e.g. a proxy) are respected.
* Test support is explicit: the Cargo feature `e2e-webview-debugging` (never enabled for distributed builds) leaves
  the overrides in place. Dev-server builds (`tauri dev`) also leave them; they cannot run without Vite.
* NEXUS has no runtime setting that enables DevTools; release builds are compiled without Tauri's `devtools` feature.
* `std::env::remove_var` is `unsafe` in Rust 2024. The workspace lint is `unsafe_code = "deny"` (not `forbid`) with
  this single, audited exception: it runs before any thread exists.

## Verification
* Unit tests: override detection in any case and with non-UTF-8 values; policy detection.
* `e2e/t18-check.mjs` against a production-mode build started with a hostile environment: the app's own WebView2
  processes exist, their command lines contain no debugging switch, the port is closed, the override is logged.
  Control: the same check fails against the `e2e-webview-debugging` build. Both run in CI.

## Consequences
* E2E builds must be built with `--features e2e-webview-debugging`; production builds must not (release gate).
* Whether `HKCU\Software\Policies` is writable by standard users was not verified on the development machine (the
  inspection was not permitted); the fail-closed policy check makes the answer irrelevant for NEXUS.
