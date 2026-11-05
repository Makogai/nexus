# ADR-0035 — Licence (proposal)

Status: **Accepted (2026-11-05)** — **MIT OR Apache-2.0** (dual), public repository `Makogai/nexus`.

## Options
| | MIT | Apache-2.0 |
| --- | --- | --- |
| Length / clarity | ~20 lines, universally understood | long, precise |
| Permissions | use, modify, distribute, sublicense, commercial | the same |
| Conditions | keep the copyright and licence notice | keep notices, state changes, pass on `NOTICE` |
| **Patent grant** | none explicit | **explicit grant from every contributor; terminates for anyone who sues over patents** |
| Contributions | terms implied | explicit: contributions are under the licence (no CLA needed for most projects) |
| Trademarks | silent | explicitly **not** granted (protects the product name) |
| GPL compatibility | GPLv2 and GPLv3 | GPLv3 only (not GPLv2) |
| Ecosystem | most Tauri/Vue apps | common in Rust (often dual with MIT) |

## Recommendation
**Apache-2.0** for NEXUS. It is an application that may receive outside contributions; the explicit patent grant and
patent-retaliation clause, the clear contribution terms and the trademark clause (the NEXUS name is not licensed) are
worth the longer text. Choose **MIT OR Apache-2.0 (dual)** instead if you want parts (e.g. `nexus-core`) to be usable by
GPLv2 projects — the Rust ecosystem convention.

## Dependencies (inventory 2026-11-05)
Method: `cargo metadata --locked --filter-platform x86_64-pc-windows-msvc`, walking normal and build dependencies of
`nexus-desktop` and `nexus-cli` (what is compiled into the shipped programs; dev dependencies excluded), and the
non-dev entries of `apps/desktop/package-lock.json`.

* **Rust, 293 third-party packages**: 288 under permissive licences — MIT and/or Apache-2.0 (the large majority),
  Unicode-3.0 (18, ICU data), Unlicense-or-MIT, Zlib, BSD-2/3-Clause, 0BSD, CC0. **5 under MPL-2.0** (file-level
  copyleft): `cssparser`, `cssparser-macros`, `selectors`, `dtoa-short` (HTML handling in Tauri's build utilities) and
  `option-ext` (via `dirs`). Used unmodified, MPL-2.0 only requires keeping their notices and pointing to their
  source; it does not extend to NEXUS's own code. No GPL/LGPL/AGPL, no package without a licence.
* **Frontend, 106 non-dev npm packages**: what is actually bundled (Vue, Vue Router, Pinia, xterm and its add-ons,
  Lucide icons, the Tauri API and plugins) is MIT, ISC or MIT/Apache-2.0. The 11 MPL-2.0 entries are `lightningcss`
  native binaries — a build-time CSS tool pulled in transitively, not part of the shipped bundle; Babel, Rolldown and
  the Vue compiler are build-time as well.
* **Added 2026-11-05 (project workbench, W2/W3)**: CodeMirror 6 (`@codemirror/*`, `@lezer/*`) and markdown-it with
  their dependencies — 57 packages under MIT, `entities` under BSD-2-Clause and `argparse` under PSF-2.0 (used only by
  markdown-it's command-line tool, not bundled). All permissive; include them in the third-party notices.

All of it is compatible with MIT, Apache-2.0 or the dual licence. Before a public release, ship a third-party notices
file (licence texts plus the MPL-2.0 source pointers) generated from this inventory with the installer — e.g. with
`cargo about` for Rust and `license-checker`-style output for npm.

## Decision needed from the owner
Apache-2.0, MIT, or dual — then add `LICENSE` (and `NOTICE` for Apache), set `license` in the Cargo and npm manifests,
and generate the third-party notices.

## Decision (2026-11-05)

The owner chose the dual licence **MIT OR Apache-2.0** when the repository became public. In place:

* `LICENSE-MIT` and `LICENSE-APACHE` at the repository root;
* `license = "MIT OR Apache-2.0"` in the workspace `Cargo.toml` (inherited by every crate), in
  `apps/desktop/package.json` and in the bundle metadata;
* `THIRD_PARTY_NOTICES.md`, generated from this inventory by `scripts/third-party-notices.mjs`, attached to every
  release.

