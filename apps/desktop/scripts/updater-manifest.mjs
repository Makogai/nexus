// The update manifest (`latest.json`) for a release (ADR-0044): which file each platform downloads and its signature.
// The Tauri updater in NEXUS fetches it from the latest *published* release and verifies the signature against the
// public key built into NEXUS before installing anything.
//
//   node scripts/updater-manifest.mjs <release-folder> <tag> <owner/repo> [notes.md]
//
// Expects the signed updater files next to their `.sig` files:
//   Windows  NEXUS_<v>_x64-setup.exe          macOS  NEXUS_<v>_aarch64.app.tar.gz, NEXUS_<v>_x64.app.tar.gz
//   Linux    NEXUS_<v>_amd64.AppImage
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

export const PLATFORMS = [
  ['windows-x86_64', (f) => /_x64-setup\.exe$/.test(f)],
  ['darwin-aarch64', (f) => /_aarch64\.app\.tar\.gz$/.test(f)],
  ['darwin-x86_64', (f) => /_x64\.app\.tar\.gz$/.test(f)],
  ['linux-x86_64', (f) => /_amd64\.AppImage$/.test(f)],
]

/** The first paragraph of the release notes after the title — the summary the update shows. */
export function summary(markdown) {
  const body = markdown.replace(/\r\n/g, '\n').split('\n').filter((l) => !l.startsWith('# '))
  const text = body.join('\n').trim().split(/\n\s*\n/)[0] ?? ''
  return text.replace(/\s+/g, ' ').trim()
}

export function manifest(dir, tag, repo, notes = '') {
  const files = readdirSync(dir)
  const version = tag.replace(/^v/, '')
  const platforms = {}
  for (const [key, match] of PLATFORMS) {
    const file = files.find((f) => match(f) && !f.endsWith('.sig'))
    if (!file) throw new Error(`no updater file for ${key} in ${dir}`)
    const sig = join(dir, `${file}.sig`)
    if (!existsSync(sig)) throw new Error(`${file} has no signature (${file}.sig) — was TAURI_SIGNING_PRIVATE_KEY set?`)
    platforms[key] = { signature: readFileSync(sig, 'utf8').trim(), url: `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(file)}` }
  }
  return { version, notes, pub_date: new Date().toISOString(), platforms }
}

if (process.argv[1]?.replaceAll('\\', '/').endsWith('scripts/updater-manifest.mjs')) {
  const [dir, tag, repo, notesFile] = process.argv.slice(2)
  if (!dir || !tag || !repo) {
    console.error('usage: node scripts/updater-manifest.mjs <release-folder> <tag> <owner/repo> [notes.md]')
    process.exit(2)
  }
  const notes = notesFile && existsSync(notesFile) ? summary(readFileSync(notesFile, 'utf8')) : ''
  const m = manifest(dir, tag, repo, notes)
  writeFileSync(join(dir, 'latest.json'), `${JSON.stringify(m, null, 2)}\n`)
  console.log(`latest.json: ${m.version} for ${Object.keys(m.platforms).join(', ')}`)
}
