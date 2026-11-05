# NEXUS release notes

## Unreleased — personal beta candidate

First installable build of NEXUS for Windows (per user, no administrator rights).

* Workspaces, projects, Claude profiles, terminals and Claude sessions with NEXUS's tools (MCP server, hooks).
* Knowledge (tasks, notes, decisions, rules), constitutions, context strategies, workflows with approved quality gates.
* Usage analytics from Claude Code's local transcripts; Git-based workspace sync with reviewed commits and pushes.
* Backups (daily, before upgrades and restores) and a safe restore; the installer keeps programs and data apart.

Known limitations: the installer is not code-signed yet (Windows SmartScreen warns); no automatic updates — install a
newer version over the old one. See docs/user/ for the guides.

Integrity: compare the installer with `SHA256SUMS.txt` (`Get-FileHash -Algorithm SHA256 <file>`); releases built by CI
carry a build-provenance attestation (`gh attestation verify <file> --repo <owner>/<repo>`).
