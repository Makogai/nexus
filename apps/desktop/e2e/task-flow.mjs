// End-to-end task flow against the REAL app, including a NEXUS restart (ADR-0018):
//   select project → select task → launch Claude (fake) → Claude updates the task through `nexus mcp`
//   → state persists in the task file → NEXUS is killed and started again → Resume continues the same
//   conversation for the same task with its current state.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/task-flow.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('task-flow: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9333'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const nexusCli = fileURLToPath(new URL('../../../target/debug/nexus.exe', import.meta.url))
const js = (v) => JSON.stringify(v)

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-taskflow-'))
const dataDir = join(root, 'data')
const cfg = join(root, 'cfg')
const repo = join(root, 'repo')
const wsDir = join(root, 'ws')
mkdirSync(join(repo, 'src'), { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'shop', dependencies: { vue: '3' } }))
const git = (...args) => execFileSync('git', ['-c', 'user.name=T', '-c', 'user.email=t@example.invalid', '-c', 'core.autocrlf=false', ...args], { cwd: repo })
git('init', '-q', '-b', 'main')
git('add', '-A')
git('commit', '-qm', 'init')

let log = ''
function startApp(report) {
  const child = spawn(app, [], {
    env: {
      ...process.env,
      NEXUS_DATA_DIR: dataDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      FAKE_CLAUDE_REPORT: report,
      FAKE_CLAUDE_STAY: '1', // the fake Claude keeps running like a real interactive session
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  child.stdout.on('data', (d) => (log += d))
  child.stderr.on('data', (d) => (log += d))
  return child
}
async function stopApp(child) {
  // Hard kill of the whole tree (app, WebView2, fake Claude): the worst case a restart must recover from.
  execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  const deadline = Date.now() + 20000
  for (;;) {
    try {
      await fetch(`http://127.0.0.1:${port}/json`)
    } catch {
      return
    }
    if (Date.now() > deadline) throw new Error('debugging port still open after stopping the app')
    await new Promise((r) => setTimeout(r, 300))
  }
}
async function waitForFile(path, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    if (existsSync(path)) {
      try {
        return JSON.parse(readFileSync(path, 'utf8'))
      } catch {
        /* partially written */
      }
    }
    if (Date.now() > deadline) throw new Error(`${path} was not written`)
    await new Promise((r) => setTimeout(r, 200))
  }
}
const appended = (report) => report.args[report.args.indexOf('--append-system-prompt') + 1] ?? ''
function mcp(sessionId, calls) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, 'mcp', '--session', sessionId], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', () => resolve(out.trim().split('\n').slice(1).map((l) => JSON.parse(l).result)))
    p.stdin.write(`${js({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'taskflow', version: '1' } } })}\n`)
    calls.forEach(([name, args], i) => p.stdin.write(`${js({ jsonrpc: '2.0', id: i + 1, method: 'tools/call', params: { name, arguments: args } })}\n`))
    p.stdin.end()
  })
}

const report1 = join(root, 'report-1.json')
const report2 = join(root, 'report-2.json')
let child = startApp(report1)
let cdp
try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const setup = await cdp.evaluate(`(async () => {
    const inv = window.__TAURI_INTERNALS__.invoke
    const ws = await inv('workspace_create', { path: ${js(wsDir)}, name: 'Shop', description: null })
    const project = await inv('project_register', { workspaceId: ws.id, path: ${js(repo)}, name: null })
    const profile = await inv('profile_create', { input: { name: 'Fake', configDir: ${js(cfg)}, executable: ${js(fakeClaude)}, defaultWorkProfile: 'balanced' } })
    const task = await inv('knowledge_create', { workspaceId: ws.id, input: { kind: 'task', title: 'Ship checkout', priority: 'high',
      projectId: project.id, body: 'Checkout page with card payment.\\n- [ ] Cart summary\\n- [ ] Payment form\\n' } })
    return { ws, project, profile, task }
  })()`)
  const taskFile = setup.task.path

  // 1. Select the project, then the task, then launch Claude from the UI.
  await cdp.evaluate(`location.hash = '#/projects/${setup.project.id}'; location.reload(); true`)
  const listed = await cdp.waitFor(`document.querySelector('[data-project-tasks]')?.innerText.includes('Ship checkout')`, 20000).catch(() => false)
  check('the project page lists its open task', !!listed)
  await cdp.evaluate(`document.querySelector('[data-task="${setup.task.id}"] .btn-primary').click(), true`)
  const dialogTask = await cdp.waitFor(`document.querySelector('[data-launch-task]')?.innerText`, 5000).catch(() => '')
  check('the launch dialog is for that task', dialogTask.includes('Ship checkout'), dialogTask)
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(), true`)
  const r1 = await waitForFile(report1)
  const brief = appended(r1)
  check('Claude runs under the selected profile', r1.env.CLAUDE_CONFIG_DIR === cfg, r1.env.CLAUDE_CONFIG_DIR)
  check('…with the profile’s work profile', brief.startsWith('NEXUS work profile: BALANCED'))
  check('…in the task’s project', r1.cwd.toLowerCase() === repo.toLowerCase(), r1.cwd)
  check('…with a short task brief (status moved to in-progress)', brief.includes(`NEXUS task for this session (id ${setup.task.id}): Ship checkout\nStatus: in-progress`))
  check('…and never the task history', !brief.includes('## Log') && brief.length < 2500, `${brief.length} chars`)
  const sessions = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })`)
  const s1 = sessions.find((s) => s.taskId === setup.task.id)
  check('the session is linked to the task and running', !!s1 && s1.running, s1 ? `${s1.state}` : 'missing')

  // 2. Claude updates the task through the NEXUS MCP server (as the real Claude would).
  const r = await mcp(s1.id, [
    ['update_task', { status: 'review', log: 'Cart summary done; payment form next', check: [1], linkBranch: true }],
    ['record_memory', { kind: 'note', title: 'Checkout uses the payment sandbox', body: 'Sandbox keys come from the developer portal.', scope: 'branch' }],
  ])
  check('MCP update_task succeeded', r[0]?.isError === false && r[0].structuredContent.updated.status === 'review', JSON.stringify(r[0]).slice(0, 200))
  check('MCP record_memory stored a branch note', r[1]?.structuredContent?.recorded?.branch === 'main')
  await cdp.evaluate(`location.hash = '#/knowledge', true`)
  const inReview = await cdp.waitFor(`document.querySelector('[data-status="review"]')?.innerText.includes('Ship checkout')`, 15000).catch(() => false)
  check('the desktop board shows Claude’s status change', !!inReview)
  const onDisk = readFileSync(taskFile, 'utf8')
  check('state persists in the task file', onDisk.includes('status: review') && onDisk.includes('- [x] Cart summary') && onDisk.includes('[claude] Cart summary done'))

  // 3. Claude recorded a conversation; NEXUS is killed and started again.
  mkdirSync(join(cfg, 'projects', 'repo'), { recursive: true })
  writeFileSync(join(cfg, 'projects', 'repo', `${s1.claudeSessionId}.jsonl`), '{}\n')
  cdp.close()
  cdp = null
  await stopApp(child)
  child = startApp(report2)
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.shell')`, 20000)
  const after = (await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })`)).find((s) => s.id === s1.id)
  check('after the restart the session is interrupted and resumable', after?.state === 'interrupted' && after?.resumable === true, `${after?.state} resumable=${after?.resumable}`)

  // 4. Resume work from the UI: same conversation, same task, current state.
  await cdp.evaluate(`location.hash = '#/sessions', true`)
  await cdp.waitFor(`[...document.querySelectorAll('.page .session')].some((r) => r.innerText.includes('Ship checkout') && [...r.querySelectorAll('button')].some((b) => b.innerText.includes('Resume')))`, 15000)
  await cdp.evaluate(`(() => { const row = [...document.querySelectorAll('.page .session')].find((r) => r.innerText.includes('Ship checkout')); [...row.querySelectorAll('button')].find((b) => b.innerText.includes('Resume')).click(); return true })()`)
  const r2 = await waitForFile(report2)
  check('Resume continues the same Claude conversation', r2.args.includes('--resume') && r2.args[r2.args.indexOf('--resume') + 1] === s1.claudeSessionId)
  check('…with the task’s current status in the brief', appended(r2).includes('Status: review'))
  const chip = await cdp.waitFor(`[...document.querySelectorAll('.page .session .task-chip')].some((c) => c.innerText.includes('Ship checkout'))`, 10000).catch(() => false)
  check('session rows show the task they work on', !!chip)
  const resumed = (await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })`)).find((s) => s.resumedFrom === s1.id)
  check('the resumed session keeps the task', resumed?.taskId === setup.task.id && resumed?.running === true)
  check('resuming does not reset the task', readFileSync(taskFile, 'utf8').includes('status: review'))
  await cdp.evaluate(`location.hash = '#/projects/${setup.project.id}', true`)
  const note = await cdp.waitFor(`document.querySelector('[data-project-notes]')?.innerText.includes('Checkout uses the payment sandbox')`, 15000).catch(() => false)
  check('the branch note appears for the checked-out branch', !!note)
  const inspector = await cdp.waitFor(`document.querySelector('[data-instructions]')?.innerText.includes('NEXUS appends')`, 15000).catch(() => false)
  check('the effective-instructions panel renders', !!inspector)
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp(child).catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nTASK FLOW OK' : `\nTASK FLOW FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
