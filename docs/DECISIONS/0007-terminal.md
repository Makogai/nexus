# ADR-0007 — Terminal: xterm.js + portable-pty with raw byte channels

Status: Accepted (2026-11-04)

## Decision
* Renderer: `@xterm/xterm` 6 with fit, search and web-links addons (WebGL optional later).
* PTY: `portable-pty` (ConPTY on Windows 10 1809+, openpty elsewhere).
* Output: a reader thread per PTY sends raw byte chunks over a `tauri::ipc::Channel` as a binary payload
  (no JSON number arrays, no base64). xterm decodes UTF-8 statefully, so chunk boundaries are harmless.
* Each PTY keeps a bounded ring buffer (512 KiB) to re-attach after view changes, webview reload or HMR.
* xterm instances are pooled outside routed views; hidden tabs keep their state.
* Shells offered: Windows PowerShell, `pwsh` and Git Bash when present, `cmd`; `$SHELL` (fallback
  `/bin/zsh` or `/bin/bash`) on Unix. WSL distros are added in Phase 2.

## Consequences
* Claude's TUI renders natively — it is the same program in a real PTY (verified with Claude Code 2.1.311).
* portable-pty creates the pseudo console with `PSEUDOCONSOLE_INHERIT_CURSOR`, so ConPTY opens every session with a
  cursor-position query (`ESC[6n`) and blocks the child until it is answered. The PTY manager answers it
  immediately (`ESC[1;1R`) and removes it from the stream, so the child never waits for a view to attach.
* Replayed ring-buffer output has terminal *queries* stripped: otherwise the re-attached xterm would answer them
  again and its replies would be typed into the program.
* Tauri's `freezePrototype` hardening cannot be enabled: xterm.js assigns `toString` on objects, which throws under a
  frozen `Object.prototype`.
* portable-pty 0.9's Windows `ChildKiller::kill` reports success as `Err(os error 0)`; `pty.rs` treats that as success.
* PTYs do not survive NEXUS exiting; durable sessions are a later opt-in via native `claude --bg` + `attach`.
