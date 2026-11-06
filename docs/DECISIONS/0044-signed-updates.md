# ADR-0044 — Signed automatic updates

Status: Accepted (2026-11-06, owner choice "signed auto-updates"; branch `milestone/ux-pass`). Closes the "updater"
item left open by ADR-0033.

## Context

NEXUS 0.1.x had no updates: every release meant downloading and installing by hand. The owner asked for an update
checker and automatic updates "like Discord": check on start, update from the app.

## Decision

1. **Tauri's updater, signed.** `tauri-plugin-updater` checks
   `https://github.com/Makogai/nexus/releases/latest/download/latest.json` — the latest *published* release (drafts are
   never offered). Every download is verified against the minisign public key in `tauri.conf.json` before it is
   installed. The private key is the owner's: generated on their computer, stored only as the repository secrets
   `TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD`; NEXUS's code and the agent never see it.
2. **Release workflow.** Only release builds make updater files (`tauri.updater.conf.json`:
   `bundle.createUpdaterArtifacts`), signed with the secrets: the NSIS installer (Windows), `NEXUS_<v>_<arch>.app.tar.gz`
   (macOS, renamed by architecture) and the AppImage (Linux). The draft job writes `latest.json`
   (`scripts/updater-manifest.mjs`: each platform's URL and signature, the release notes' first paragraph) and drops the
   loose `.sig` files. Local installer builds need no key.
3. **Behaviour** (`stores/updates.ts`):
   * Release builds only (`AppInfo.updates`); development and E2E builds never check.
   * On start, NEXUS checks and — unless *Install updates by themselves when NEXUS starts* is off — installs a newer
     version right away (no sessions run at start) and restarts. Windows installs in passive mode.
   * Then it checks every 6 hours and only *offers* the update: a button in the top bar and in Settings. While Claude
     sessions run it asks first (their terminals end; Claude keeps the conversations, so they can be resumed).
   * Offline or no manifest yet: nothing happens; Settings shows the reason.
4. **Data safety** stays as ADR-0032 has it: the new version backs up the database before migrating.

## Consequences

* From 0.1.2 on, releases reach installed NEXUS by themselves; 0.1.1 and earlier must install 0.1.2 by hand once.
* A compromised download location cannot ship code: without the private key, signatures do not verify.
* Losing the private key means installed versions can no longer update automatically (a new key needs a manual install).
* The installers themselves are still not code-signed (SmartScreen, Gatekeeper) — a separate decision.
