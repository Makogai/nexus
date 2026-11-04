// End-to-end Constitution Manager against the REAL app (ADR-0027):
//   New constitution through the UI (starter) → edit a rule in the tree → saved as Markdown on disk → a synced change
//   (as by `git pull`) is flagged and withheld from Claude → approve → effective preview for the project with a
//   mandatory-override conflict → a fake Claude launch receives the effective rules (mandatory first, no markup)
//   → the review notification opens the Constitutions view.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/constitutions.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('constitutions: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9335'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-constitutions-'))
const dataDir = join(root, 'data')
const wsDir = join(root, 'ws')
const repo = join(root, 'shop')
const report = join(root, 'report.json')
mkdirSync(repo, { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'shop', dependencies: { vue: '3' } }))

let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_REPORT: report, FAKE_CLAUDE_STAY: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))
async function stopApp() {
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
async function waitForFile(path, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    if (existsSync(path)) {
      try {
        return JSON.parse(readFileSync(path, 'utf8'))
      } catch {
        /* partial */
      }
    }
    if (Date.now() > deadline) throw new Error(`${path} was not written`)
    await sleep(200)
  }
}
const rulesDir = join(wsDir, 'rules')
const ruleFiles = () => (existsSync(rulesDir) ? readdirSync(rulesDir).filter((f) => f.endsWith('.md')) : [])

let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const setValue = (sel, value) =>
  cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); return true })()`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.click(); return true })()`)

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const ws = await inv('workspace_create', { path: wsDir, name: 'Shop', description: null })
  const project = await inv('project_register', { workspaceId: ws.id, path: repo, name: null })
  const profile = await inv('profile_create', { input: { name: 'Fake', configDir: join(root, 'cfg'), executable: fakeClaude } })
  await cdp.evaluate(`location.hash = '#/constitutions'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-new-constitution]')`, 20000)

  // 1. Create through the UI from the starter template.
  await click('[data-new-constitution]')
  await cdp.waitFor(`document.querySelector('[data-c-title]')`, 5000)
  await setValue('[data-c-title]', 'Team charter')
  await click('[data-c-create]')
  const created = await cdp.waitFor(`document.querySelector('[data-constitution="Team charter"]') && document.querySelector('[data-constitution-detail] h2')?.innerText`, 10000).catch(() => '')
  check('the new constitution opens in the editor', created === 'Team charter', created)
  const files = ruleFiles()
  check('…stored as a Markdown rule file in the workspace', files.length === 1 && readFileSync(join(rulesDir, files[0]), 'utf8').includes('format: constitution'), files.join(','))
  const charterPath = join(rulesDir, files[0])

  // 2. Edit a rule title in the tree and save.
  const ok = await setValue('[data-rule="TC-3"] input[aria-label="Rule title"]', 'Explain trade-offs in one sentence')
  check('the starter rules have stable ids in the tree', ok)
  await click('[data-save]')
  let text = ''
  for (let i = 0; i < 40 && !text.includes('one sentence'); i++) {
    await sleep(250)
    text = readFileSync(charterPath, 'utf8')
  }
  await cdp.waitFor(`document.querySelector('.save-bar')?.innerText.includes('Saved')`, 10000).catch(() => false)
  check('saving writes the change to the file (ids kept)', text.includes('#### Explain trade-offs in one sentence\n<!-- rule: id=TC-3 enforcement=guideline -->'), text.slice(0, 120))

  // 3. A synced change (as by git pull): flagged, withheld, then approved.
  writeFileSync(charterPath, text.replace('Secrets, tokens and keys never go', 'Secrets, tokens and keys may go'))
  await click('.page-actions .btn-ghost') // Refresh
  const banner = await cdp.waitFor(`document.querySelector('[data-pending-banner]')?.innerText`, 10000).catch(() => '')
  check('a version changed outside NEXUS is flagged', banner.includes('changed outside NEXUS'), banner)
  const pendingEff = await inv('constitution_effective', { workspaceId: ws.id, projectId: project.id, taskId: null, includePending: false })
  check('…and its rules are not in force', pendingEff.rules.length === 0 && pendingEff.pending.length === 1)
  await click('[data-constitution="Team charter"]')
  await cdp.waitFor(`document.querySelector('[data-approve]')`, 10000)
  await click('[data-approve]')
  const approved = await cdp.waitFor(`!document.querySelector('[data-pending-banner]')`, 10000).catch(() => false)
  check('approving that version clears the flag', !!approved)

  // 4. A project constitution tries to disable the mandatory policy → effective preview shows the conflict.
  const proj = await inv('constitution_create', { workspaceId: ws.id, input: { title: 'Shop rules', scope: 'project', projectId: project.id } })
  const structure = { preamble: '', articles: [{ title: 'Exceptions', text: '', sections: [{ title: '', text: '', rules: [{ id: 'TC-2', title: 'Secrets allowed', enforcement: 'rule', enabled: false, overrides: null, text: 'No.' }, { id: '', title: 'Use pnpm', enforcement: 'rule', enabled: true, overrides: null, text: 'This project uses pnpm, not npm.' }] }] }] }
  await inv('constitution_save', { workspaceId: ws.id, id: proj.id, structure, expectedHash: proj.hash })
  await click('[data-tab="effective"]')
  await cdp.waitFor(`document.querySelector('[data-eff-project]')`, 5000)
  await setValue('[data-eff-project]', project.id)
  const conflict = await cdp.waitFor(`document.querySelector('[data-conflict]')?.innerText`, 10000).catch(() => '')
  check('the effective preview reports the mandatory-override conflict', conflict.includes('mandatory polic'), conflict)
  const effRules = await cdp.evaluate(`[...document.querySelectorAll('[data-effective-rule]')].map((e) => e.dataset.effectiveRule)`)
  check('…and keeps the mandatory policy in force, adding the project rule', effRules.includes('TC-2') && effRules.includes('SR-1'), effRules.join(','))

  // 5. Launch: the effective rules reach the fake Claude compactly.
  await inv('session_start_claude', { args: { profileId: profile.id, projectId: project.id, title: 'Rules check', cols: 100, rows: 30 } })
  const r = await waitForFile(report)
  const prompt = r.args[r.args.indexOf('--append-system-prompt') + 1] ?? ''
  check('the launch gets the constitution (mandatory first)', prompt.includes('[TC-2 · mandatory] Never commit credentials') && prompt.indexOf('TC-2') < prompt.indexOf('SR-1'), prompt.slice(0, 160))
  check('…never the raw markup, guidelines by id only', !prompt.includes('<!-- rule') && prompt.includes('TC-3') && !prompt.includes('one sentence'))

  // 6. A review notification for changed rules opens the Constitutions view.
  writeFileSync(charterPath, readFileSync(charterPath, 'utf8').replace('Every behaviour change', 'Most behaviour changes'))
  await inv('session_start_claude', { args: { profileId: profile.id, projectId: project.id, title: 'Second', cols: 100, rows: 30 } })
  const n = await (async () => {
    for (let i = 0; i < 20; i++) {
      const list = await inv('notifications_list', { filter: { limit: 50 } })
      const hit = list.find((x) => x.category === 'review-required' && x.targetKind === 'rules')
      if (hit) return hit
      await sleep(300)
    }
    return null
  })()
  check('changed rules raise a review notification at launch', !!n, n?.title)
  await cdp.evaluate(`location.hash = '#/notifications', true`)
  await cdp.waitFor(`document.querySelector('.page [data-notification="${n?.id}"]')`, 10000)
  await click(`.page [data-notification="${n?.id}"] .body`)
  const opened = await cdp.waitFor(`location.hash.startsWith('#/constitutions') && document.querySelector('[data-review]')`, 10000).catch(() => false)
  check('…which opens the document that needs review', !!opened)
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp().catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nCONSTITUTIONS OK' : `\nCONSTITUTIONS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
