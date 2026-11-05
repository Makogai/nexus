// Release checks before an installer may be published (ADR-0033). Run after building the installer:
//   node scripts/release-check.mjs [bundle-dir]     (default: target/release/bundle/nsis)
// Checks the configuration and the frontend bundle, then writes SHA256SUMS.txt next to the installer.
// Exit code 1 when anything fails. Never uploads or publishes anything.
import { createHash } from 'node:crypto'
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const app = join(here, '..')
const repo = join(app, '..', '..')
const bundleDir = process.argv[2] ?? join(repo, 'target', 'release', 'bundle', 'nsis')
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const walk = (d) => readdirSync(d).flatMap((e) => (statSync(join(d, e)).isDirectory() ? walk(join(d, e)) : [join(d, e)]))

// 1. Configuration: no debugging in production, a CSP, the CLI bundled, nothing extra.
const conf = JSON.parse(readFileSync(join(app, 'src-tauri', 'tauri.conf.json'), 'utf8'))
const installerConf = JSON.parse(readFileSync(join(app, 'src-tauri', 'tauri.installer.conf.json'), 'utf8'))
const cargo = readFileSync(join(app, 'src-tauri', 'Cargo.toml'), 'utf8')
check('the e2e-webview-debugging feature is off by default', !/default\s*=\s*\[[^\]]*e2e-webview-debugging/.test(cargo))
check('release builds refuse the e2e feature at compile time', readFileSync(join(app, 'src-tauri', 'src', 'lib.rs'), 'utf8').includes('compile_error!("the `e2e-webview-debugging` feature'))
check('a Content-Security-Policy is set and no global Tauri object is exposed', typeof conf.app?.security?.csp === 'string' && conf.app.security.csp.includes("default-src 'self'") && !conf.app?.withGlobalTauri)
check('the installer bundles the nexus CLI and no other resources', js(installerConf.bundle?.externalBin) === js(['binaries/nexus']) && !conf.bundle?.resources && !installerConf.bundle?.resources)
check('the main program has a fixed name (not the product name, which would clash with nexus.exe)', conf.mainBinaryName === 'nexus-desktop')
const vite = readFileSync(join(app, 'vite.config.ts'), 'utf8')
check('source maps and unminified code only in debug builds', /sourcemap:\s*!!process\.env\.TAURI_ENV_DEBUG/.test(vite) && /minify:\s*!process\.env\.TAURI_ENV_DEBUG/.test(vite))
function js(v) {
  return JSON.stringify(v)
}

// 2. Frontend bundle: no source maps, credentials or machine paths in what is embedded in the app. dist/ is what the
// installer embeds only if it was not rebuilt afterwards (a later debug build writes source maps into it).
const installers = existsSync(bundleDir) ? readdirSync(bundleDir).filter((f) => f.endsWith('-setup.exe')) : []
const dist = join(app, 'dist')
if (existsSync(dist)) {
  const files = walk(dist)
  if (installers.length === 1) {
    const newest = Math.max(...files.map((f) => statSync(f).mtimeMs))
    check('dist/ is the frontend inside this installer (not rebuilt since)', newest <= statSync(join(bundleDir, installers[0])).mtimeMs,
      newest <= statSync(join(bundleDir, installers[0])).mtimeMs ? '' : 'dist/ is newer than the installer — rebuild the release installer and run this check right after it')
  }
  check('no source maps in the production frontend', !files.some((f) => f.endsWith('.map')))
  const text = files.filter((f) => /\.(js|css|html|json|svg)$/.test(f)).map((f) => [f, readFileSync(f, 'utf8')])
  const secret = /(sk-ant-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)/
  const hits = text.filter(([, t]) => secret.test(t)).map(([f]) => f)
  check('no credential-shaped strings in the frontend bundle', hits.length === 0, hits.join(', '))
  const machine = /[A-Za-z]:\\\\Users\\\\[A-Za-z0-9._-]+\\\\/
  const paths = text.filter(([, t]) => machine.test(t)).map(([f]) => f)
  check('no absolute machine paths in the frontend bundle', paths.length === 0, paths.join(', '))
} else {
  check('the frontend was built (dist/)', false, 'run npx vite build or the installer build first')
}

// 3. Artifacts: the installer exists; checksums for integrity.
check('an installer was built', installers.length === 1, `${bundleDir}: ${installers.join(', ') || 'none'}`)
const version = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8')).version
check('the installer carries the app version', installers.every((f) => f.includes(`_${conf.version}_`)) && conf.version === version, `${conf.version} / package ${version}`)
if (installers.length) {
  const lines = installers.map((f) => `${createHash('sha256').update(readFileSync(join(bundleDir, f))).digest('hex')}  ${f}`)
  writeFileSync(join(bundleDir, 'SHA256SUMS.txt'), lines.join('\n') + '\n')
  console.log(`      ${lines.join('\n      ')}`)
  check('SHA256SUMS.txt written next to the installer', existsSync(join(bundleDir, 'SHA256SUMS.txt')))
}
console.log(failures === 0 ? '\nRELEASE CHECK OK' : `\nRELEASE CHECK FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
