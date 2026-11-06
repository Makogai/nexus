// End-to-end Context Manager against the REAL app (ADR-0028):
//   project page → Context → analysis + recommendation → choose Deep in the wizard → a fake Claude launch records the
//   strategy → SessionStart (real `nexus hook`) brings the deep project index → InstructionsLoaded and MCP retrieval
//   (as Claude would run them) → the Inspector shows observed files against the prediction and the retrievals.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/context.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('context: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9336'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const nexusCli = fileURLToPath(new URL('../../../target/debug/nexus.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-context-'))
const dataDir = join(root, 'data')
const repo = join(root, 'shop')
const cfg = join(root, 'cfg')
const report = join(root, 'report.json')
mkdirSync(join(repo, 'docs'), { recursive: true })
mkdirSync(cfg, { recursive: true })
writeFileSync(join(repo, 'README.md'), '# Shop\n\n## Payments\n\nCheckout uses the hosted payment page.\n')
writeFileSync(join(repo, 'docs', 'deploy.md'), '# Deploy\n\nProduction deploys run from main.\n')
writeFileSync(join(repo, 'CLAUDE.md'), 'Shop instructions.\n')

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
function run(args, stdin) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, ...args], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', () => resolve(out))
    p.stdin.end(stdin)
  })
}

let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.click(); return true })()`)
const textOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
async function until(fn, ms = 15000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(250)
  }
}

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const ws = await inv('workspace_create', { path: join(root, 'ws'), name: 'Shop', description: null })
  const project = await inv('project_register', { workspaceId: ws.id, path: repo, name: null })
  const profile = await inv('profile_create', { input: { name: 'Fake', configDir: cfg, executable: fakeClaude } })
  for (let i = 0; i < 3; i++) {
    await inv('knowledge_create', { workspaceId: ws.id, input: { kind: 'decision', title: `Decision ${i + 1}`, projectId: project.id, status: 'accepted', body: 'x' } })
  }
  await inv('knowledge_create', { workspaceId: ws.id, input: { kind: 'task', title: 'Ship <checkout>', projectId: project.id, body: '' } })

  // 1. From the project page to the wizard.
  await cdp.evaluate(`location.hash = '#/projects/${project.id}'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-project-context]')`, 20000)
  await click('[data-project-context]')
  const rec = await cdp.waitFor(`document.querySelector('[data-recommendation]')?.innerText`, 15000).catch(() => '')
  check('the Context view analyses the project and recommends a strategy', rec.includes('Recommended'), rec)
  check('…it is not configured yet (default)', !!(await cdp.evaluate(`!!document.querySelector('[data-not-configured]')`)))
  const docsListed = await cdp.evaluate(`document.querySelector('[data-context-strategy]')?.innerText.includes('docs/deploy.md')`)
  check('…and lists the project documents available on demand', !!docsListed)
  await click('[data-use="deep"]')
  const inUse = await cdp.waitFor(`document.querySelector('[data-strategy="deep"]')?.innerText.includes('in use')`, 10000).catch(() => false)
  check('choosing Deep saves it for the project', !!inUse && (await inv('context_get', { projectId: project.id })).strategy === 'deep')

  // 2. Launch: the session records the strategy; SessionStart brings the deep project index.
  const s = await inv('session_start_claude', { args: { profileId: profile.id, projectId: project.id, title: 'Deep session', cols: 100, rows: 30 } })
  await waitForFile(report)
  check('the session records its context strategy', s.integrations?.context?.strategy === 'deep' && s.integrations?.context?.observeInstructions === true)
  const out = await run(['hook', '--session', s.id, 'session-start'], js({ hook_event_name: 'SessionStart', source: 'startup', session_id: s.claudeSessionId }))
  const ctx = (() => {
    try {
      return JSON.parse(out).hookSpecificOutput.additionalContext
    } catch {
      return ''
    }
  })()
  check('SessionStart brings the project index as escaped data', ctx.includes('open tasks: Ship ‹checkout›') && ctx.includes('Decision 3'), ctx.slice(0, 200))

  // 3. What Claude loads and retrieves (as the real CLI would report and call it).
  await run(['hook', '--session', s.id, 'instructions-loaded'], js({ file_path: join(repo, 'CLAUDE.md'), memory_type: 'Project', load_reason: 'session_start' }))
  await run(['mcp', '--session', s.id], [
    js({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } }),
    js({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'search_knowledge', arguments: { query: 'hosted payment' } } }),
    js({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'get_document', arguments: { path: 'docs/deploy.md' } } }),
  ].join('\n') + '\n')

  // 4. The Inspector.
  await click('[data-tab="inspector"]')
  const observed = await cdp.waitFor(`document.querySelector('[data-col="observed"] [data-observed="predicted"]')?.innerText`, 15000).catch(() => '')
  check('the Inspector shows the observed CLAUDE.md as predicted', observed.includes('CLAUDE.md'), observed)
  const retrieved = await cdp.evaluate(`document.querySelector('[data-col="retrieved"]')?.innerText ?? ''`)
  check('…and the retrievals with their documents', retrieved.includes('search_knowledge') && retrieved.includes('get_document') && retrieved.includes('docs/deploy.md'), retrieved.slice(0, 200))
  await click('[data-tab="metrics"]')
  const metrics = await cdp.waitFor(`document.querySelector('[data-context-metrics]')?.innerText`, 10000).catch(() => '')
  check('retrieval metrics count calls, not queries', /2\s*retrievals/.test(metrics) && !metrics.includes('hosted payment'), metrics.slice(0, 160))

  // 5. A task handed to the running session (it already has the project's context) instead of a new session.
  const t2 = await inv('knowledge_create', { workspaceId: ws.id, input: { kind: 'task', title: 'Fix rounding', projectId: project.id, body: 'Round cents.\n' } })
  await cdp.evaluate(`location.hash = '#/projects/${project.id}'; true`)
  await cdp.waitFor(`!!document.querySelector('[data-task="${t2.id}"]')`, 15000)
  await cdp.evaluate(`[...document.querySelectorAll('[data-task="${t2.id}"] .btn')].find((b) => b.innerText.includes('Launch'))?.click(); true`)
  const startIn = await cdp.waitFor(`document.querySelector('[data-launch-start-in]')?.innerText`, 10000).catch(() => '')
  check('a task launch offers the running session of its project', startIn.includes('A running session') && startIn.includes('New session'), startIn.replace(/\n/g, ' ').slice(0, 120))
  await click('[data-start-in="continue"]')
  const hint = await textOf('[data-start-in-hint]')
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(); true`)
  const received = await cdp
    .waitFor(`(() => { const t = [...document.querySelectorAll('.xterm-rows')].map((r) => r.innerText.replace(/\\n/g, '')).join(''); return t.includes('received: Next, please work on NEXUS task ${t2.id}') })()`, 15000)
    .catch(() => false)
  const linked = await until(async () => {
    const x = await inv('session_get', { id: s.id })
    return x.taskId === t2.id ? x : null
  })
  check('Hand over types NEXUS’s message with the task id into that session and links it', !!received && !!linked && hint.includes('Deep session'), hint.slice(0, 120))
  const t2File = readFileSync(t2.path, 'utf8')
  check('…and starts the task', t2File.includes('status: in-progress'))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp().catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nCONTEXT OK' : `\nCONTEXT FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
