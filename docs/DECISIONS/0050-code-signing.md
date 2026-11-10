# ADR-0050 — Code signing

Status: Accepted (2026-11-10; the owner delegated the decision and asked to ship it later). Implementation deferred:
NEXUS 0.5 alpha ships unsigned, as 0.1.x did. Closes the "signing" item left open by ADR-0033.

## Context

Installers are not signed (ADR-0033): Windows SmartScreen warns on the first start of a downloaded installer, macOS asks
the person to confirm an ad-hoc signed app. What protects people today: checksums (`SHA256SUMS.txt`), build provenance
attestations in the release job, and the updater's own signature — every update is verified against the public key in
`tauri.conf.json` before it is installed (ADR-0044). Today NEXUS reaches its owner and people who get it from the owner.
Signing costs money every year and puts a key somewhere that must be protected. The owner left the choice to Claude:
"u decide code signing, ship it later".

## Decision

1. **Not now.** 0.5 alpha and further alphas ship unsigned. **Signing becomes a release requirement** before the first
   release meant for people who do not get NEXUS from the owner directly (a public beta or 1.0), or earlier if the
   SmartScreen warning keeps someone from installing it.
2. **Windows: Authenticode, signed inside the build.** `bundle.windows.signCommand` in `tauri.installer.conf.json` signs
   `nexus-desktop.exe`, the `nexus` CLI sidecar and the NSIS installer while `tauri build` runs: SHA-256 digests and an
   RFC 3161 timestamp, so signatures stay valid after the certificate expires. Never a separate step after the build:
   that would change the installer after its updater signature (`.sig`) was made, and every update would fail
   verification. The comment in `release.yml` marks the place.
3. **The key never leaves a hardware security module (HSM).** There is no key file in the repository, no `.pfx` in a CI
   secret, and the owner's machine signs nothing. Signing runs only in the release job, with credentials stored as
   secrets of a protected GitHub environment (release tags only). Preferred provider: Microsoft's managed signing service
   (Trusted Signing), if the publisher is eligible: Microsoft manages the certificates and their renewal, there is no
   hardware to look after, and it costs a small monthly fee. Otherwise: an OV certificate from a certificate authority
   (CA) that signs in the cloud, with the key in the CA's HSM. Not EV: EV certificates no longer earn instant SmartScreen
   reputation, so the extra cost and hardware buy nothing here. Never a self-signed certificate: it would ask people to
   trust a root of NEXUS's own.
4. **The publisher on the certificate is whoever publishes NEXUS.** That is the owner, unless NEXUS moves to an
   organisation's name. The application identity (ADR-0034) does not change.
5. **macOS: Developer ID signing and notarization** (Apple Developer Program), switched on in the same release as
   Windows signing; until then the bundle stays ad-hoc signed (`signingIdentity: "-"`). Linux packages keep relying on
   checksums and the updater signature.
6. **Verified, not assumed.** When signing is switched on, `scripts/release-check.mjs` checks that the installer and both
   programs carry a valid, timestamped signature from the expected publisher, and `installed.mjs` checks the installed
   copies. A release whose signature is missing fails the job.

## Consequences

* Until then, installing NEXUS needs one click past SmartScreen ("More info → Run anyway"); the README and the release
  notes say how.
* Even after signing, SmartScreen reputation for a new certificate builds up over downloads, so the first signed releases
  may still warn for a while.
* Signing adds a yearly or monthly cost and a provider account. The decision is recorded now, so turning it on is a
  configuration change, not a new design.

## Alternatives considered

* **Sign now with an OV certificate.** Pays for a warning that matters little at the current audience, and the
  reputation would build up on alpha builds.
* **A separate `signtool` step after the build** (the earlier placeholder in `release.yml`). It breaks the updater
  signature (item 2).
* **Store the certificate as a `.pfx` in repository secrets.** Since 2023 CAs issue code-signing keys only on hardware,
  and a key that can be copied from CI is the risk signing is supposed to remove.
