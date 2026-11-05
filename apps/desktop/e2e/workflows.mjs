// End-to-end Workflow Engine against the REAL app (ADR-0029), synthetic profile and temporary repository:
//   template → gate defined in the UI → project page → Start workflow (professional) → fake Claude launched for the
//   run's task → Claude reports through `nexus mcp` (AI-reported) → the desktop runner verifies with the gates by
//   itself → review in the UI → completed, notified (recording adapter), click opens the run → a failing gate goes
//   back to Claude → cancel → a NEXUS crash during a running gate is recovered after restart.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/workflows.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('workflows: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9337'
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

const root = mkdtempSync(join(tmpdir(), 'nexus-workflows-'))
const dataDir = join(root, 'data')
const repo = join(root, 'shop')
const report = join(root, 'report.json')
mkdirSync(repo, { recursive: true })
writeFileSync(join(repo, 'README.md'), '# Shop\n')
const git = (...args) => execFileSync('git', ['-c', 'user.name=T', '-c', 'user.email=t@example.invalid', '-c', 'core.autocrlf=false', ...args], { cwd: repo })
git('init', '-q', '-b', 'main')
git('add', '-A')
git('commit', '-qm', 'init')

let log = ''
function startApp() {
  const child = spawn(app, [], {
    env: { ...process.env, NEXUS_DATA_DIR: dataDir, NEXUS_E2E_NOTIFIER: 'record', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_REPORT: report, FAKE_CLAUDE_STAY: '1' },
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
function mcp(sessionId, calls) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, 'mcp', '--session', sessionId], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', () => resolve(out.trim().split('\n').slice(1).map((l) => JSON.parse(l).result)))
    p.stdin.write(`${js({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'wf', version: '1' } } })}\n`)
    calls.forEach(([name, args], i) => p.stdin.write(`${js({ jsonrpc: '2.0', id: i + 1, method: 'tools/call', params: { name, arguments: args } })}\n`))
    p.stdin.end()
  })
}

let child = startApp()
let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const setValue = (sel, value) =>
  cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); return true })()`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.click(); return true })()`)
async function until(fn, ms = 15000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(300)
  }
}
const stepOf = (v, id) => v.steps.find((s) => s.stepId === id)
// Define a gate and approve exactly the command that was saved (what a person does after reviewing it).
async function approvedGate(projectId, input) {
  const g = await inv('gate_upsert', { projectId, input })
  return inv('gate_approve', { projectId, name: g.name, expectedHash: g.commandHash })
}

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const ws = await inv('workspace_create', { path: join(root, 'ws'), name: 'Shop', description: null })
  const project = await inv('project_register', { workspaceId: ws.id, path: repo, name: null })
  const profile = await inv('profile_create', { input: { name: 'Fake', configDir: join(root, 'cfg'), executable: fakeClaude } })
  for (const g of ['lint', 'typecheck']) await approvedGate(project.id, { name: g, program: 'git', args: ['--version'] })
  await cdp.evaluate(`location.hash = '#/workflows'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-new-workflow]')`, 20000)

  // 1. A workflow from a template, through the UI.
  await click('[data-new-workflow]')
  await cdp.waitFor(`document.querySelector('[data-template="feature"]')`, 5000)
  await click('[data-template="feature"]')
  const steps = await cdp.waitFor(`document.querySelectorAll('[data-workflow-detail] [data-def-step]').length`, 10000).catch(() => 0)
  check('the feature template becomes an editable workflow with its steps', steps === 6, `${steps} steps`)

  // 2. A gate defined in the UI (argument vector, no shell).
  await click('[data-tab="gates"]')
  await cdp.waitFor(`document.querySelector('[data-gate-name]')`, 5000)
  await setValue('[data-gate-name]', 'test')
  await setValue('[data-gate-command]', 'git --version')
  await click('[data-gate-save]')
  const gateRow = await cdp.waitFor(`document.querySelector('[data-gate="test"]')?.innerText`, 10000).catch(() => '')
  const runBlocked = await cdp.evaluate(`document.querySelector('[data-run-gate="test"]')?.disabled === true`)
  check('a gate defined in the UI needs its command approved before it can run', gateRow.includes('git --version') && gateRow.includes('v1 needs approval') && runBlocked, gateRow)
  const forged = await inv('gate_approve', { projectId: project.id, name: 'test', expectedHash: 'f'.repeat(64) }).then(() => 'approved', (e) => String(e?.message ?? e))
  check('…an approval for a different command is refused', forged !== 'approved', forged)
  await click('[data-approve-gate="test"]')
  const shown = await cdp.waitFor(`document.querySelector('[data-confirm-message]')?.innerText`, 5000).catch(() => '')
  await click('[data-confirm-ok]')
  const approvedRow = await cdp.waitFor(`document.querySelector('[data-gate="test"][data-approved="yes"]')?.innerText`, 10000).catch(() => '')
  check('…the approval dialog shows the exact command, then the gate is approved', shown.includes('git --version') && shown.includes('does not approve any workflow') && approvedRow.includes('v1 approved'), `${shown.slice(0, 80)} / ${approvedRow}`)
  check('…and says that no waiting workflow step resumes yet', shown.includes('No waiting workflow step resumes with this approval.'))

  // 3. Start from the project page at professional rigor, launching the fake Claude for the run's task.
  await cdp.evaluate(`location.hash = '#/projects/${project.id}', true`)
  await cdp.waitFor(`document.querySelector('[data-project-workflow]')`, 10000)
  await click('[data-project-workflow]')
  await cdp.waitFor(`document.querySelector('[data-start-workflow][data-loaded]')`, 10000)
  await setValue('[data-start-rigor]', 'professional')
  await setValue('[data-start-goal]', 'Add checkout')
  await setValue('[data-start-profile]', profile.id)
  const disabled = await cdp.evaluate(`document.querySelector('[data-start-run]')?.disabled`)
  if (disabled) console.error('start button disabled; dialog:', await cdp.evaluate(`document.querySelector('[role=dialog]')?.innerText`))
  await click('[data-start-run]')
  const r = await waitForFile(report).catch(async (e) => {
    console.error('dialog after start:', await cdp.evaluate(`document.querySelector('[role=dialog]')?.innerText ?? '(closed)'`))
    throw e
  })
  const prompt = r.args[r.args.indexOf('--append-system-prompt') + 1] ?? ''
  check('Claude runs for the run’s task at the run’s rigor', prompt.startsWith('NEXUS work profile: PROFESSIONAL') && prompt.includes('NEXUS workflow run'), prompt.slice(0, 90))
  const runs = await inv('workflow_runs', { workspaceId: ws.id, filter: null })
  const runId = runs[0].id
  const session = (await inv('session_list', { filter: null })).find((s) => s.integrations?.workflow?.runId === runId)
  check('the session is bound to the run', !!session && session.running)
  const detail = await cdp.waitFor(`document.querySelector('[data-run="${runId}"]')?.innerText`, 10000).catch(() => '')
  check('the run detail opens', detail.includes('Feature implementation'))

  // 4. Claude reports (AI-reported); the desktop runner verifies with the gates on its own.
  const rep = await mcp(session.id, [
    ['report_step', { step: 'plan', status: 'done', summary: 'Plan recorded', criteria: [1] }],
    ['report_step', { step: 'implement', status: 'done', summary: 'Implemented with tests' }],
    ['report_step', { step: 'docs', status: 'done', summary: 'too early' }],
  ])
  check('reports are accepted as claims', rep[0]?.structuredContent?.verification === 'ai-reported' && rep[1]?.structuredContent?.status === 'verifying')
  check('…and a step whose dependencies are not done is refused', rep[2]?.isError === true)
  const verified = await until(async () => {
    const v = await inv('workflow_run', { id: runId })
    return stepOf(v, 'implement').verification === 'verified' && stepOf(v, 'checks').status === 'passed' ? v : null
  })
  check('the desktop runner verified the steps with the gates', !!verified, verified ? '' : js((await inv('workflow_run', { id: runId })).steps.map((s) => [s.stepId, s.status])))
  await mcp(session.id, [['report_step', { step: 'docs', status: 'done', summary: 'README updated' }]])
  const badge = await cdp.waitFor(`document.querySelector('[data-step="implement"] [data-verification]')?.innerText`, 10000).catch(() => '')
  const aiBadge = await cdp.evaluate(`document.querySelector('[data-step="plan"] [data-verification]')?.innerText ?? ''`)
  check('the UI shows verified and AI-reported apart', badge === 'Verified by gates' && aiBadge === 'AI-reported — not verified', `${badge} / ${aiBadge}`)
  const reviewNote = await until(async () => (await inv('notifications_list', { filter: { limit: 50 } })).find((n) => n.category === 'review-required' && n.workflowId === runId))
  check('a pending review notifies', !!reviewNote)

  // 5. Review in the UI → completed → notified; a click on the notification opens the run.
  await cdp.waitFor(`document.querySelector('[data-approve-step]')`, 10000)
  await click('[data-approve-step]')
  const done = await until(async () => ((await inv('workflow_run', { id: runId })).status === 'completed' ? true : null))
  const finished = await until(async () => (await inv('e2e_notifications_recorded')).find((n) => n.category === 'workflow-completed'))
  check('approval completes the run and a desktop notification says so', !!done && !!finished, finished?.body)
  await cdp.evaluate(`location.hash = '#/', true`)
  await inv('e2e_notification_activate', { id: finished.id })
  const opened = await cdp.waitFor(`location.hash.startsWith('#/workflows') && document.querySelector('[data-run="${runId}"]')?.dataset.runStatus`, 10000).catch(() => '')
  check('clicking it opens the run', opened === 'completed', opened)

  // 6. A changed command is a new version: the step waits (nothing executes) until it is approved again; the
  //    approval resumes verification on its own. Then the failing gate sends the step back to Claude.
  const changed = await inv('gate_upsert', { projectId: project.id, input: { name: 'test', program: 'git', args: ['rev-parse', '--verify', 'no-such-ref'] } })
  check('changing a gate command makes an unapproved new version', changed.version === 2 && changed.approved === false, `v${changed.version} ${changed.approved}`)
  const wfId = runs[0].workflowId
  const run2 = await inv('workflow_start', { workspaceId: ws.id, input: { workflowId: wfId, projectId: project.id, goal: 'Second', rigor: 'balanced' } })
  const s2 = await inv('session_start_claude', { args: { profileId: profile.id, projectId: project.id, taskId: run2.taskId, workProfile: 'balanced', cols: 100, rows: 30 } })
  await mcp(s2.id, [['report_step', { step: 'plan', status: 'done', summary: 'p' }], ['report_step', { step: 'implement', status: 'done', summary: 'i' }]])
  const waiting = await until(async () => {
    const v = await inv('workflow_run', { id: run2.id })
    const s = stepOf(v, 'implement')
    return s.status === 'blocked' && s.blockedOn === 'gates' ? { s, v } : null
  })
  const approvalNote = await until(async () => (await inv('notifications_list', { filter: { limit: 50 } })).find((n) => n.category === 'input-required' && n.workflowId === run2.id))
  check(
    '…a step needing it waits without executing anything, and NEXUS asks for the approval',
    !!waiting && waiting.s.lastFailure.includes('test (version 2) are not approved') && waiting.s.results.every((g) => g.gateName !== 'test') && !!approvalNote,
    waiting?.s?.lastFailure?.slice(0, 90),
  )
  // The approval dialog's preview: exactly this step in this run resumes.
  await cdp.evaluate(`location.hash = '#/workflows'; true`)
  await cdp.waitFor(`document.querySelector('[data-tab="gates"]')`, 10000)
  await click('[data-tab="gates"]')
  await cdp.waitFor(`document.querySelector('[data-approve-gate="test"]')`, 10000)
  await click('[data-approve-gate="test"]')
  const preview = await cdp.waitFor(`document.querySelector('[data-confirm-message]')?.innerText`, 5000).catch(() => '')
  check('…the approval dialog says which waiting work resumes', preview.includes('Approving resumes 1 waiting workflow step in 1 run automatically.'), preview.split('\n').find((l) => l.includes('resume')))
  await click('[data-confirm-ok]')
  await until(async () => (await inv('gates_list', { projectId: project.id })).find(([g]) => g.name === 'test' && g.approved))
  const back = await until(async () => {
    const v = await inv('workflow_run', { id: run2.id })
    const s = stepOf(v, 'implement')
    return s.status === 'ready' && s.lastFailure ? s : null
  })
  check('after approval the step verifies with version 2 and the failure returns it to Claude', !!back && back.lastFailure.includes('gate test failed') && back.attempts === 1 && back.results.some((g) => g.gateName === 'test' && g.gateVersion === 2), back?.lastFailure?.slice(0, 80))
  const seen = await mcp(s2.id, [['get_workflow_run', {}]])
  check('…and Claude sees the failure through get_workflow_run', (seen[0]?.structuredContent?.steps ?? []).some((s) => s.id === 'implement' && s.lastFailure?.includes('gate test failed') && s.lastFailure?.includes('Needed a single revision')))
  await inv('workflow_cancel', { id: run2.id })
  check('cancelling stops the run', (await inv('workflow_run', { id: run2.id })).status === 'cancelled')

  // 7. Crash recovery: NEXUS is killed while a gate runs; after the restart the step is verified again.
  await approvedGate(project.id, { name: 'slow', program: process.execPath, args: ['-e', 'setTimeout(() => {}, 6000)'], timeoutSecs: 60 })
  const slowWf = await inv('workflow_create', { workspaceId: ws.id, input: { title: 'Slow check', category: 'testing' } })
  const saved = await inv('workflow_save_raw', { workspaceId: ws.id, id: slowWf.id, body: '## Slow check\n<!-- step: id=slow kind=check gates=slow -->\n', expectedHash: slowWf.hash })
  const run3 = await inv('workflow_start', { workspaceId: ws.id, input: { workflowId: saved.id, projectId: project.id, goal: 'crash', rigor: 'balanced' } })
  const runningGate = await until(async () => stepOf(await inv('workflow_run', { id: run3.id }), 'slow').results.find((g) => g.status === 'running'))
  check('the runner started the gate', !!runningGate)
  cdp.close()
  cdp = null
  await stopApp(child)
  child = startApp()
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.shell')`, 20000)
  const recovered = await until(async () => {
    const v = await inv('workflow_run', { id: run3.id })
    const s = stepOf(v, 'slow')
    return s.status === 'passed' ? s : null
  }, 40000)
  check('after the crash the gate is re-run and the step verified', !!recovered && recovered.results.some((g) => g.status === 'interrupted') && recovered.verification === 'verified', js(recovered?.results?.map((g) => g.status)))
  const task = await inv('knowledge_task', { workspaceId: ws.id, id: runs[0].taskId })
  check('the first run’s task is done with its log', task.status === 'done' && task.recentLog.some((l) => l.includes('workflow completed')), task.status)
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp(child).catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nWORKFLOWS OK' : `\nWORKFLOWS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
