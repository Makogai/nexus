# ADR-0034 — Application identity (proposal)

Status: **Proposed — awaiting the owner's choice. Nothing is changed until then.**

## Context
The identifier `dev.nexus.desktop` was provisional. Reverse-DNS identifiers should be in a namespace the publisher
controls (a domain, or a code-hosting namespace); `nexus.dev` is not ours. The identifier must be fixed before the
first installer reaches anyone, because Windows and the installer key things on it.

## What depends on the identifier (Windows)
| Item | Today | Effect of a change |
| --- | --- | --- |
| Notification sender (AppUserModelID, Start Menu shortcut) | `dev.nexus.desktop` | old toasts stay under the old sender; the old registry key must be removed (uninstall hook + a one-time cleanup at start) |
| WebView2 profile folder `%LOCALAPPDATA%\<identifier>` | `dev.nexus.desktop` | window/UI cache starts fresh (no user data — preferences are in SQLite) |
| Installer "delete app data" option (`$LOCALAPPDATA\<identifier>`) | WebView2 cache | follows the new id |
| Uninstall entry / install folder | product name `NEXUS` | unchanged — installing the new build over the old one works |
| Installer manufacturer (publisher shown in *Installed apps*; remembered install location `HKCU\Software\<manufacturer>\NEXUS`) | `nexus` — Tauri derives it from the identifier's second segment | changes with the id unless `bundle.publisher` is set; the old location key stays behind (Tauri never removes it) — set `publisher` explicitly together with the id and delete the old key in the uninstall hook |
| NEXUS data `%LOCALAPPDATA%\NEXUS` | product name, not the id | **unchanged — no data migration** |
| `nexus://` protocol | scheme, not the id | unchanged (but see the name question below) |
| Future: macOS bundle id, Linux app id, updater | — | the same id everywhere |

## Proposal
1. Choose a namespace you control:
   * **`io.github.<your-github-user-or-org>.nexus`** — recommended if releases come from GitHub; free, stable, verifiable;
   * `com.<your-domain>.nexus` — if you own a domain you will keep.
2. Keep the product name for the beta; decide before a public release whether **"NEXUS"** stays — the name is widely
   used (e.g. Sonatype Nexus); a distinctive name avoids confusion and trademark questions. If the name changes, also
   consider the `nexus://` scheme (shared with other software) — e.g. `<name>-app://`.
3. Implementation once chosen (one change, before the first distributed installer): set `identifier`; replace the
   literal in the notifier, the self-check, the NSIS hook and the tests by one constant; set `bundle.publisher`; remove
   the old AppUserModelID key at first start if it points to this program; the uninstall hook removes both ids and the
   old remembered-location key. No user data migration.

## Decision needed from the owner
The namespace and final identifier (and whether the product name stays for the public release).
