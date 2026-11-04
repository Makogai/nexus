// Self-contained E2E run (CI and local): start the built NEXUS app with an empty isolated database and WebView2
// remote debugging, run the smoke checks, stop the app, exit non-zero on any failure.
//
//   cargo build -p fake-claude
//   npx tauri build --debug --no-bundle      # embeds the production frontend and CSP
//   node e2e/run-smoke.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only for now (WebView2 DevTools protocol). Development/CI tooling — never used by release builds.
import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { waitForPort } from './cdp-lib.mjs'
import { runSmoke } from './smoke.mjs'

if (process.platform !== 'win32') {
  console.error('run-smoke: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}

const port = process.env.CDP_PORT ?? '9333'
const app =
  process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const dataDir = mkdtempSync(join(tmpdir(), 'nexus-e2e-data-'))

const child = spawn(app, [], {
  env: {
    ...process.env,
    NEXUS_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})
let log = ''
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))
let exited = false
let tearingDown = false
child.on('exit', (code) => {
  exited = true
  if (!tearingDown) console.error(`app exited unexpectedly (code ${code})`)
})

let failures = 1
try {
  await waitForPort(port, 120_000)
  failures = await runSmoke()
} catch (e) {
  console.error(`E2E error: ${e.message}`)
} finally {
  tearingDown = true
  if (!exited) {
    // The app is ours (spawned above); end it and its WebView2 helpers by process tree.
    spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  }
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nE2E OK' : `\nE2E FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
