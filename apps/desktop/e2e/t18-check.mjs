// Regression test for threat T18: a PRODUCTION-mode build must ignore inherited WebView2 debugging overrides.
//
//   npx tauri build --debug --no-bundle          # embedded frontend, WITHOUT the e2e-webview-debugging feature
//   node e2e/t18-check.mjs [path/to/nexus-desktop.exe]
//
// Starts the app with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>, waits until the app's own
// WebView2 browser process exists (so a crashed app cannot pass), then asserts that the browser process was started
// without debugging switches, that the port is closed and that NEXUS logged the ignored override. Windows only.
import { execFileSync, spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'win32') {
  console.error('t18-check: Windows only (WebView2).')
  process.exit(2)
}
const port = '9444'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const child = spawn(app, [], {
  env: {
    ...process.env,
    NEXUS_DATA_DIR: mkdtempSync(join(tmpdir(), 'nexus-t18-')),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER: 'nexus-t18-pipe',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})
let log = ''
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))

/** Command lines of WebView2 processes descended from our app (our own child process tree only). */
function webviewCommandLines(rootPid) {
  const script =
    'Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,Name,CommandLine | ConvertTo-Json -Compress'
  const all = JSON.parse(execFileSync('powershell.exe', ['-NoProfile', '-Command', script], { encoding: 'utf8', maxBuffer: 64 << 20 }))
  const tree = new Set([rootPid])
  let grew = true
  while (grew) {
    grew = false
    for (const p of all) {
      if (tree.has(p.ParentProcessId) && !tree.has(p.ProcessId)) {
        tree.add(p.ProcessId)
        grew = true
      }
    }
  }
  return all.filter((p) => tree.has(p.ProcessId) && /msedgewebview2/i.test(p.Name ?? '')).map((p) => p.CommandLine ?? '')
}

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

try {
  let lines = []
  const deadline = Date.now() + 60_000
  while (Date.now() < deadline && lines.length === 0) {
    await new Promise((r) => setTimeout(r, 1000))
    lines = webviewCommandLines(child.pid)
  }
  check('the app created its webview', lines.length > 0, `${lines.length} WebView2 process(es)`)
  check(
    'WebView2 was started without debugging switches',
    lines.length > 0 && lines.every((l) => !/remote-debugging|nexus-t18-pipe/i.test(l)),
  )
  const open = await fetch(`http://127.0.0.1:${port}/json`).then(() => true, () => false)
  check('no DevTools endpoint is listening', !open)
  check('NEXUS reports the ignored override', /ignored webview override variables/.test(log))
} finally {
  spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
}
console.log(failures === 0 ? '\nT18 OK' : `\nT18 FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
