// The update manifest the release publishes (ADR-0044): every platform, its URL and its signature — or a clear failure.
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
// @ts-expect-error — a plain Node script without types
import { manifest, summary } from '../scripts/updater-manifest.mjs'

function release(withSigs = true) {
  const dir = mkdtempSync(join(tmpdir(), 'nexus-manifest-'))
  for (const f of ['NEXUS_0.1.2_x64-setup.exe', 'NEXUS_0.1.2_aarch64.app.tar.gz', 'NEXUS_0.1.2_x64.app.tar.gz', 'NEXUS_0.1.2_amd64.AppImage']) {
    writeFileSync(join(dir, f), 'x')
    if (withSigs) writeFileSync(join(dir, `${f}.sig`), `sig-of-${f}\n`)
  }
  writeFileSync(join(dir, 'NEXUS_0.1.2_aarch64.dmg'), 'x')
  return dir
}

describe('updater manifest', () => {
  it('names each platform’s signed file in the tagged release', () => {
    const m = manifest(release(), 'v0.1.2', 'Makogai/nexus', 'Better updates.')
    expect(m.version).toBe('0.1.2')
    expect(Object.keys(m.platforms)).toEqual(['windows-x86_64', 'darwin-aarch64', 'darwin-x86_64', 'linux-x86_64'])
    expect(m.platforms['windows-x86_64']).toEqual({
      signature: 'sig-of-NEXUS_0.1.2_x64-setup.exe',
      url: 'https://github.com/Makogai/nexus/releases/download/v0.1.2/NEXUS_0.1.2_x64-setup.exe',
    })
    expect(m.platforms['darwin-aarch64'].url).toMatch(/NEXUS_0\.1\.2_aarch64\.app\.tar\.gz$/)
  })

  it('refuses a release whose files are not signed', () => {
    expect(() => manifest(release(false), 'v0.1.2', 'Makogai/nexus')).toThrow(/no signature/)
  })

  it('uses the first paragraph of the release notes', () => {
    expect(summary('# NEXUS 0.1.2 — updates\r\n\r\nNEXUS now updates\nitself.\n\n## What is new\n- a')).toBe('NEXUS now updates itself.')
  })
})
