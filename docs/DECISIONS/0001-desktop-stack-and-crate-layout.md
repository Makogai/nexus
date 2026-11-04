# ADR-0001 — Desktop stack and minimal crate layout

Status: Accepted (2026-11-04)

## Context
The product must be a cross-platform desktop app with a real terminal, native process control and a small
footprint. The spec suggests Tauri 2 + Vue 3 + TypeScript + Rust and a nine-crate workspace.

## Decision
* **Tauri 2** (latest 2.x; Tauri 3 is alpha and is not used) with the system webview (WebView2 on Windows).
* **Vue 3 + TypeScript + Pinia + vue-router**, Vite build, no UI component framework (custom design tokens
  keep the look distinctive and the bundle small). TypeScript 5.9.x: vue-tsc is not yet validated on the
  native TypeScript 7 compiler.
* **Cargo workspace**: `crates/nexus-core` (all domain logic), `apps/desktop/src-tauri` (`nexus-desktop`:
  IPC + PTY), `crates/nexus-cli` (`nexus`: CLI now; MCP server and hook handler in Phase 2) and a test-double
  binary `crates/fake-claude` used by launch tests.

## Alternatives
* **Electron** — mature, but ~10× memory and a bundled Chromium; node-pty would put PTY handling in JS.
* **Nine crates** — premature boundaries; refactors across crates are slow; no second consumer yet.

## Consequences
* WebView differences (WebView2 / WKWebView / WebKitGTK) must be tested per platform, especially xterm.js
  rendering (WebGL addon is optional with DOM fallback).
* Modules are split into crates only when a second consumer or a compile-time boundary appears.
