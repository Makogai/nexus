// End-to-end: the project workbench milestone (W2–W4) against the REAL app — temporary workspace and repositories only.
//   W2 Knowledge: a constitution opens as rule cards (no raw markers); a note opens as rendered Markdown with raw HTML
//      escaped; Edit → Save writes the file (approved here); a stale save is a conflict; Constitutions open in Read mode.
//
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/workbench.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol).
import { execFileSync, spawn } from 'node:child_process'
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('workbench: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9343'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-workbench-'))
let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: join(root, 'data'), NEXUS_E2E_NOTIFIER: 'record', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))

let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el || el.disabled) return false; el.click(); return true })()`)
const textOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
const htmlOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerHTML ?? ''`)
async function until(fn, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(250)
  }
}
/** Replace the CodeMirror document the way a person does: focus, Ctrl+A, type. */
async function setEditor(sel, text) {
  const focused = await cdp.evaluate(`(() => { const c = document.querySelector(${js(sel)} + ' .cm-content'); if (!c) return false; c.focus(); return true })()`)
  if (!focused) return false
  const key = { modifiers: 2, key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65 }
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', ...key })
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...key })
  await cdp.send('Input.insertText', { text })
  await sleep(200)
  return cdp.evaluate(`(document.querySelector(${js(sel)} + ' .cm-content')?.innerText ?? '').includes(${js(text.split('\n').pop())})`)
}
const closeDialog = () => cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn')].find((b) => b.innerText.trim() === 'Close')?.click(); true`)

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 60000)
  const ws = await inv('workspace_create', { path: join(root, 'ws'), name: 'Bench', description: null, template: 'ai-engineering-pro' })
  const note = await inv('knowledge_create', {
    workspaceId: ws.id,
    input: { kind: 'note', title: 'Release notes', body: '## Plan\n\nShip **on Tuesday**.\n\n<script>window.__pwned = 1</script>\n\n[docs](https://example.com)' },
  })

  // --- W2: reading ---------------------------------------------------------------------------------------------------
  await cdp.evaluate(`location.hash = '#/knowledge'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell') && document.querySelector('.filter')`, 30000)
  await cdp.evaluate(`[...document.querySelectorAll('.filter')].find((f) => /^Rules/.test(f.innerText.trim()))?.click(); true`)
  await cdp.waitFor(`[...document.querySelectorAll('.list-row.doc')].some((r) => r.innerText.includes('Security'))`, 15000)
  await cdp.evaluate(`[...document.querySelectorAll('.list-row.doc')].find((r) => r.innerText.includes('Security'))?.click(); true`)
  const cards = await until(async () => ((await cdp.evaluate(`document.querySelectorAll('[role=dialog] [data-constitution-rules] [data-rule]').length`)) || null))
  const reading = await textOf('[role=dialog] [data-doc-reading]')
  check('a constitution opens as rule cards with ids and enforcement', cards >= 10 && reading.includes('SEC-1') && reading.includes('Mandatory'), `${cards} cards`)
  check('…without raw rule markers', !reading.includes('<!--') && !reading.includes('enforcement='))
  await closeDialog()

  await cdp.evaluate(`[...document.querySelectorAll('.filter')].find((f) => /^Notes/.test(f.innerText.trim()))?.click(); true`)
  await cdp.waitFor(`document.querySelector('[data-doc="${note.id}"]')`, 15000)
  await click(`[data-doc="${note.id}"]`)
  await cdp.waitFor(`document.querySelector('[role=dialog] [data-markdown]')`, 10000)
  const html = await htmlOf('[role=dialog] [data-markdown]')
  check('a note opens as rendered Markdown', html.includes('<h2>Plan</h2>') && html.includes('<strong>on Tuesday</strong>'), html.slice(0, 80))
  check('…raw HTML in a document is shown as text, never run', !html.includes('<script') && (await cdp.evaluate(`window.__pwned === undefined`)))

  // --- W2: editing ---------------------------------------------------------------------------------------------------
  await click('[data-doc-edit-start]')
  await cdp.waitFor(`!!document.querySelector('[data-doc-edit] .cm-content')`, 10000)
  await cdp.evaluate(`(() => { const i = document.querySelector('[data-doc-edit-title]'); i.value = 'Release plan'; i.dispatchEvent(new Event('input', { bubbles: true })); return true })()`)
  const typed = await setEditor('[data-doc-edit]', '## Plan\n\nShip **on Wednesday**.')
  await click('[data-doc-save]')
  const saved = await until(async () => ((await cdp.evaluate(`!!document.querySelector('[role=dialog] [data-markdown]')`)) ? await htmlOf('[role=dialog] [data-markdown]') : null))
  const file = readFileSync(note.path, 'utf8')
  check('Edit → Save writes the document and shows it rendered again', typed && !!saved && saved.includes('on Wednesday') && file.includes('title: Release plan') && file.includes('Ship **on Wednesday**.'), file.split('\n').slice(0, 4).join(' | '))
  const doc = await inv('knowledge_read', { workspaceId: ws.id, id: note.id })
  check('…and the saved version is approved on this machine', doc.approved === true)

  // A change on disk after opening: saving is refused as a conflict and the file keeps the other change.
  await click('[data-doc-edit-start]')
  await cdp.waitFor(`!!document.querySelector('[data-doc-edit] .cm-content')`, 10000)
  writeFileSync(note.path, readFileSync(note.path, 'utf8').replace('on Wednesday', 'on Thursday'))
  await setEditor('[data-doc-edit]', '## Plan\n\nShip **on Friday**.')
  await click('[data-doc-save]')
  const conflict = await cdp.waitFor(`document.querySelector('[role=dialog] .notice-error')?.innerText`, 10000).catch(() => '')
  check('a stale save is refused as a conflict, nothing is overwritten', conflict.includes('changed on disk') && readFileSync(note.path, 'utf8').includes('on Thursday'), conflict.slice(0, 80))
  await cdp.evaluate(`document.querySelector('[role=dialog] footer .btn-ghost')?.click(); true`) // Cancel
  await cdp.evaluate(`document.querySelector('[data-confirm-ok]')?.click(); true`) // discard
  await closeDialog()

  // Constitutions open formatted; Edit is one click away.
  await cdp.evaluate(`location.hash = '#/constitutions'; true`)
  await cdp.waitFor(`document.querySelector('[data-constitution-detail] [data-constitution-rules]')`, 15000)
  check('the Constitution Manager opens a document in Read mode', (await textOf('[data-constitution-detail] [data-mode="read"]')) === 'Read' && !(await cdp.evaluate(`!!document.querySelector('[data-save]')`)))
  await click('[data-mode="structured"]')
  check('…and Edit shows the tree editor', !!(await until(async () => ((await cdp.evaluate(`!!document.querySelector('[data-save]')`)) ? true : null), 5000)))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  try {
    execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  } catch {}
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nWORKBENCH OK' : `\nWORKBENCH FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
