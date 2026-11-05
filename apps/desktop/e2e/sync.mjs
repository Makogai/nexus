// End-to-end Sync Center against the REAL app (ADR-0031, M29) — temporary workspace, local bare remote, a teammate
// simulated with plain Git. Never touches real repositories or remotes.
//   workspace folder → git init + remote → Sync Center: local changes, a blocked key file and a blocked secret →
//   reviewed commit (confirmed) → reviewed first push (confirmed) → teammate pushes → fetch → incoming diff →
//   fast-forward pull (confirmed) → diverged: pull disabled, push refused → missing remote explained in history →
//   onboarding a second workspace from Git with the restore checklist.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/sync.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol).
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('sync: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9339'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-sync-'))
const dataDir = join(root, 'data')
const wsDir = join(root, 'team-ws')
const remote = join(root, 'remote.git')
const teammate = join(root, 'teammate')
const url = `file://${remote.replace(/\\/g, '/')}`
const git = (cwd, ...args) => execFileSync('git', ['-c', 'init.defaultBranch=main', '-c', 'core.autocrlf=false', ...args], { cwd, encoding: 'utf8' })
const ident = (dir, name) => {
  git(dir, 'config', 'user.name', name)
  git(dir, 'config', 'user.email', `${name.toLowerCase()}@example.invalid`)
}

let log = ''
function startApp() {
  const child = spawn(app, [], {
    env: { ...process.env, NEXUS_DATA_DIR: dataDir, NEXUS_E2E_NOTIFIER: 'record', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  child.stdout.on('data', (d) => (log += d))
  child.stderr.on('data', (d) => (log += d))
  return child
}
async function stopApp(child) {
  execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  for (let i = 0; i < 60; i++) {
    try {
      await fetch(`http://127.0.0.1:${port}/json`)
    } catch {
      return
    }
    await sleep(300)
  }
}

const child = startApp()
let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const setValue = (sel, value) =>
  cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event('input', { bubbles: true })); return true })()`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el || el.disabled) return false; el.click(); return true })()`)
const text = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
const disabled = (sel) => cdp.evaluate(`!!document.querySelector(${js(sel)})?.disabled`)
async function until(fn, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(250)
  }
}
const state = () => cdp.evaluate(`document.querySelector('[data-sync-view]')?.dataset.syncState ?? ''`)
async function confirmDialog() {
  const m = await cdp.waitFor(`document.querySelector('[data-confirm-message]')?.innerText`, 8000).catch(() => '')
  await click('[data-confirm-ok]')
  return m
}

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const ws = await inv('workspace_create', { path: wsDir, name: 'Team', description: null })
  mkdirSync(remote)
  git(remote, 'init', '-q', '--bare')
  git(wsDir, 'init', '-q')
  ident(wsDir, 'Owner')
  git(wsDir, 'remote', 'add', 'origin', url)
  writeFileSync(join(wsDir, 'deploy.pem'), 'not read by NEXUS')
  mkdirSync(join(wsDir, 'notes'))
  writeFileSync(join(wsDir, 'notes', 'leak.md'), `ship it with ${'sk-ant-'}${'k'.repeat(30)}\n`)
  await cdp.evaluate(`location.hash = '#/sync'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-sync-view]')?.dataset.syncState === 'no-upstream'`, 20000)
  check('the Sync Center shows a repository that was never pushed', (await text('[data-sync-badge]')) === 'Not pushed yet')
  const pem = await cdp.evaluate(`(() => { const r = document.querySelector('[data-sync-file="deploy.pem"]'); return r && { blocked: r.dataset.syncBlocked, checked: r.querySelector('input').checked } })()`)
  check('a key file is flagged by name and left out of the selection', pem?.blocked === 'yes' && pem.checked === false, js(pem))

  // Review: the secret in notes/leak.md blocks; leaving it out makes the commit possible.
  await click('[data-sync-review]')
  const finding = await cdp.waitFor(`document.querySelector('[data-sync-finding="secret:block"]')?.innerText`, 10000).catch(() => '')
  check('the review blocks a secret by file and line, without showing it', finding.includes('notes/leak.md:1') && !finding.includes('kkkk'), finding)
  await setValue('[data-sync-message]', 'Workspace')
  check('…and the commit button stays disabled', await disabled('[data-sync-commit-confirm]'))
  await cdp.evaluate(`document.querySelector('[data-sync-commit-dialog]')?.closest('[role=dialog]')?.querySelector('footer .btn')?.click(); true`)
  await cdp.evaluate(`(() => { const c = document.querySelector('[data-sync-file="notes/leak.md"] input'); if (c.checked) c.click(); return true })()`)
  await click('[data-sync-review]')
  await cdp.waitFor(`document.querySelector('[data-sync-commit-dialog]') && !document.querySelector('[data-sync-finding$=":block"]')`, 10000)
  await setValue('[data-sync-message]', 'Workspace')
  await click('[data-sync-commit-confirm]')
  const confirmText = await confirmDialog()
  check('committing needs a confirmation that says nothing is pushed', confirmText.includes('Nothing is pushed'), confirmText)
  const committed = await until(async () => ((await cdp.evaluate(`document.querySelectorAll('[data-sync-outgoing-commit]').length`)) === 1 ? true : null))
  const left = await cdp.evaluate(`[...document.querySelectorAll('[data-sync-file]')].map((r) => r.dataset.syncFile).sort().join(',')`)
  check('exactly the reviewed files were committed', !!committed && left === 'deploy.pem,notes/leak.md', left)

  // First push: the whole history travels — said before, confirmed by the person.
  await click('[data-sync-push]')
  const pushDialog = await cdp.waitFor(`document.querySelector('[data-sync-push-dialog]')?.innerText`, 10000).catch(() => '')
  check('the push review says the whole history travels on a first push', pushDialog.includes('whole history travels'), pushDialog.slice(0, 120))
  await click('[data-sync-push-confirm]')
  await confirmDialog()
  const upToDate = await until(async () => ((await state()) === 'up-to-date' ? true : null))
  check('after the confirmed push the workspace is up to date', !!upToDate && git(remote, 'log', '--oneline').includes('Workspace'), await state())

  // A teammate pushes; fetch shows it; the incoming diff can be read; pull fast-forwards after a confirmation.
  git(root, 'clone', '-q', url, teammate)
  ident(teammate, 'Mate')
  mkdirSync(join(teammate, 'docs'), { recursive: true })
  writeFileSync(join(teammate, 'docs', 'deploy.md'), 'Deploy on Tuesdays.\n')
  git(teammate, 'add', '-A')
  git(teammate, 'commit', '-qm', 'Team note')
  git(teammate, 'push', '-q', 'origin', 'HEAD')
  await click('[data-sync-fetch]')
  const incoming = await until(async () => ((await state()) === 'behind' ? await text('[data-sync-incoming]') : null))
  check('fetch shows the incoming commit with its author', !!incoming && incoming.includes('Team note') && incoming.includes('Mate'), incoming ?? '')
  await cdp.evaluate(`document.querySelector('[data-sync-incoming-file="docs/deploy.md"] button')?.click(); true`)
  const diff = await cdp.waitFor(`document.querySelector('[data-sync-diff]')?.innerText`, 10000).catch(() => '')
  check('the incoming diff is shown before pulling', diff.includes('+Deploy on Tuesdays.'), diff.slice(0, 80))
  await click('[data-sync-pull]')
  const pullConfirm = await confirmDialog()
  const pulled = await until(async () => ((await state()) === 'up-to-date' && existsSync(join(wsDir, 'docs', 'deploy.md')) ? true : null))
  check('pull fast-forwards after a confirmation that mentions approvals', !!pulled && pullConfirm.includes('stay inactive until you approve'), pullConfirm.slice(0, 100))

  // Diverged: the teammate pushes again while there is a local commit — no pull, no forced push.
  writeFileSync(join(teammate, 'docs', 'deploy.md'), 'Deploy on Wednesdays.\n')
  git(teammate, 'commit', '-qam', 'Change of plan')
  git(teammate, 'push', '-q', 'origin', 'HEAD')
  writeFileSync(join(wsDir, 'README.md'), '# Team workspace\n')
  await click('[data-sync-refresh]')
  await cdp.waitFor(`document.querySelector('[data-sync-file="README.md"] input')?.checked`, 10000)
  await click('[data-sync-review]')
  await cdp.waitFor(`document.querySelector('[data-sync-commit-dialog]')`, 10000)
  await setValue('[data-sync-message]', 'Readme')
  await click('[data-sync-commit-confirm]')
  await confirmDialog()
  await until(async () => ((await cdp.evaluate(`document.querySelectorAll('[data-sync-outgoing-commit]').length`)) === 1 ? true : null))
  await click('[data-sync-fetch]')
  const diverged = await until(async () => ((await state()) === 'diverged' ? true : null))
  check('a diverged branch is shown and pull is disabled', !!diverged && (await disabled('[data-sync-pull]')), await state())
  const refused = await inv('sync_review_push', { workspaceId: ws.id }).then((r) => r.behind)
  check('…and a push review reports what the remote has (never forced)', refused === 1, String(refused))

  // A missing remote is explained and recorded; nothing changes locally.
  git(wsDir, 'remote', 'set-url', 'origin', `file://${join(root, 'missing.git').replace(/\\/g, '/')}`)
  await click('[data-sync-fetch]')
  const failedRow = await until(async () => (await cdp.evaluate(`!!document.querySelector('[data-sync-history-row="fetch:failed"]')`)) ? await text('[data-sync-history-row="fetch:failed"]') : null)
  check('a missing remote is explained in the history', !!failedRow && failedRow.includes('remote-missing'), failedRow ?? '')
  git(wsDir, 'remote', 'set-url', 'origin', url)

  // Onboarding: open the workspace from Git into a new folder (as on a second machine).
  await click('[data-sync-clone-open]')
  await cdp.waitFor(`document.querySelector('[data-sync-clone-url]')`, 5000)
  await setValue('[data-sync-clone-url]', url)
  await setValue('[data-sync-clone-parent]', root)
  await setValue('[data-sync-clone-name]', 'second-machine')
  // Same workspace id → NEXUS refuses a second registration on this machine: explained, folder removed.
  await click('[data-sync-clone-start]')
  const cloneError = await cdp.waitFor(`document.querySelector('[data-sync-clone-error]')?.innerText`, 30000).catch(() => '')
  check('opening the same workspace twice on one machine is refused and cleaned up', cloneError.length > 0 && !existsSync(join(root, 'second-machine')), cloneError.slice(0, 120))
  const history = await cdp.evaluate(`[...document.querySelectorAll('[data-sync-history-row]')].map((r) => r.dataset.syncHistoryRow)`)
  check('every operation is in the history', ['commit:ok', 'push:ok', 'fetch:ok', 'pull:ok', 'fetch:failed'].every((h) => history.includes(h)), history.join(' '))
  const restore = await (async () => {
    await cdp.evaluate(`document.querySelector('[data-sync-clone]')?.closest('[role=dialog]')?.querySelector('[data-sync-clone] .btn-ghost')?.click(); true`)
    await click('[data-sync-restore-open]')
    return cdp.waitFor(`document.querySelector('[data-sync-restore]')?.innerText`, 10000).catch(() => '')
  })()
  check('the restore checklist says what stays machine-local', restore.includes('never synced'), restore.slice(0, 120))
  check('no machine-local file reached the remote', !git(remote, 'ls-tree', '-r', '--name-only', 'HEAD').match(/\.db|\.pem|leak\.md|\.nexus-local/), git(remote, 'ls-tree', '-r', '--name-only', 'HEAD').trim().replace(/\n/g, ' '))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp(child).catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nSYNC OK' : `\nSYNC FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
