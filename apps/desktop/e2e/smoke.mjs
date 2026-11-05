// End-to-end smoke test against the REAL running app (real IPC, real PTY, real UI).
//
// Prerequisites (see docs/TESTING.md):
//   * `cargo build -p fake-claude`
//   * NEXUS running with an EMPTY isolated database and remote debugging, e.g. (Git Bash):
//       NEXUS_DATA_DIR=$TMP/nexus-smoke WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 npm run tauri dev
//
// Uses only synthetic fixtures in a temp directory; never touches real Claude profiles.
// Native folder pickers cannot be automated, so setup calls the same IPC commands those dialogs feed.
import { execFileSync, spawn } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

const exe = process.platform === 'win32' ? 'fake-claude.exe' : 'fake-claude'
const fakeClaude = fileURLToPath(new URL(`../../../target/debug/${exe}`, import.meta.url))
const nexusCli = fileURLToPath(new URL(`../../../target/debug/nexus${process.platform === 'win32' ? '.exe' : ''}`, import.meta.url))
const js = (v) => JSON.stringify(v)

/** Runs every check against the already-running app; resolves to the number of failures. */
export async function runSmoke() {
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const root = mkdtempSync(join(tmpdir(), 'nexus-smoke-'))

// Fixture repository with a secret embedded in its remote URL.
const repo = join(root, 'repo')
mkdirSync(join(repo, 'src'), { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'smoke', dependencies: { vue: '3' }, scripts: { dev: 'vite' } }))
writeFileSync(join(repo, 'src', 'main.ts'), 'x\n')
const git = (...args) => execFileSync('git', ['-c', 'user.name=S', '-c', 'user.email=s@example.invalid', '-c', 'core.autocrlf=false', ...args], { cwd: repo })
git('init', '-q', '-b', 'main')
git('add', '-A')
git('commit', '-qm', 'init')
git('remote', 'add', 'origin', 'https://user:smoke-secret@example.invalid/acme/smoke.git')

await waitForPort()
const cdp = await connect()
try {
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const setup = await cdp.evaluate(`(async () => {
    const inv = window.__TAURI_INTERNALS__.invoke
    const ws = await inv('workspace_create', { path: ${js(join(root, 'ws'))}, name: 'Smoke', description: null })
    const project = await inv('project_register', { workspaceId: ws.id, path: ${js(repo)}, name: null })
    const profile = await inv('profile_create', { input: { name: 'Smoke Fake', configDir: ${js(join(root, 'cfg'))},
      executable: ${js(fakeClaude)}, defaultWorkProfile: 'prototype' } })
    return { project, profile }
  })()`)
  check('remote URL credentials are redacted', setup.project.remoteUrl === 'https://example.invalid/acme/smoke.git', setup.project.remoteUrl)
  check('stack detected deterministically', setup.project.stack.stacks.includes('vue'))

  // Knowledge (ADR-0013/0017): a rule arriving through sync must be approved in the UI before Claude sees it.
  const wsDir = join(root, 'ws')
  const ruleId = randomUUID()
  mkdirSync(join(wsDir, 'rules'), { recursive: true })
  writeFileSync(
    join(wsDir, 'rules', `${ruleId}.md`),
    `---\nnexus: 1\nid: ${ruleId}\nkind: rule\ntitle: Smoke synced rule\napplies: always\ncreated: 2026-01-01T00:00:00Z\nupdated: 2026-01-01T00:00:00Z\nsource: human\n---\nSMOKE-RULE-BODY: keep the smoke test green.\n`,
  )
  const setInput = (sel, value, event = 'input') =>
    cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); el.value = ${js(value)}; el.dispatchEvent(new Event(${js(event)}, { bubbles: true })); return true })()`)
  await cdp.evaluate(`location.hash = '#/knowledge'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell .page-actions')`, 20000)
  const flagged = await cdp.waitFor(`document.querySelector('.approvals')?.innerText.includes('Smoke synced rule')`, 15000).catch(() => false)
  check('unapproved synced rule is flagged in the Knowledge view', !!flagged)
  await cdp.evaluate(`document.querySelector('.approvals button').click(), true`)
  const reviewed = await cdp.waitFor(`document.querySelector('[role=dialog] .body')?.innerText.includes('SMOKE-RULE-BODY')`, 10000).catch(() => false)
  check('approval dialog shows the rule text before approving', !!reviewed)
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer button')].find((b) => b.innerText.includes('Approve this version')).click(), true`)
  const approved = await cdp.waitFor(`!document.querySelector('[role=dialog]') && !document.querySelector('.approvals')`, 10000).catch(() => false)
  check('approving clears the pending-approval banner', !!approved)

  await cdp.evaluate(`document.querySelector('.page-actions .btn-primary').click(), true`)
  await cdp.waitFor(`document.querySelector('#k-title')`, 5000)
  await setInput('#k-title', 'Smoke task from the UI')
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(), true`)
  const card = await cdp.waitFor(`document.querySelector('[data-status="backlog"]')?.innerText.includes('Smoke task from the UI')`, 10000).catch(() => false)
  const taskFiles = () => readdirSync(join(wsDir, 'tasks')).map((f) => readFileSync(join(wsDir, 'tasks', f), 'utf8'))
  check('task created in the UI is a Markdown file in the workspace', !!card && taskFiles().some((t) => t.includes('title: Smoke task from the UI')))
  await setInput('[data-status="backlog"] select', 'in-progress', 'change')
  const moved = await cdp.waitFor(`document.querySelector('[data-status="in-progress"]')?.innerText.includes('Smoke task from the UI')`, 10000).catch(() => false)
  check('moving a task on the board rewrites its file', !!moved && taskFiles().some((t) => t.includes('status: in-progress')))

  // Drive the real UI: reload, open the launch dialog, launch.
  await cdp.evaluate(`location.hash = '#/'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell .topbar .btn-primary')`, 20000)
  await cdp.evaluate(`document.querySelector('.topbar .btn-primary').click(), true`)
  const selected = await cdp.waitFor(
    `document.querySelector('#launch-profile') && document.querySelector('.segment.selected')?.innerText`,
    5000,
  )
  check('launch dialog applies the profile default rigor', selected === 'Prototype', selected)
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(), true`)
  const raw = await cdp.waitFor(
    `(() => { const r = [...document.querySelectorAll('.xterm-rows')].pop(); const t = r?.innerText.replace(/\\n/g, '') ?? ''; const m = t.match(/\\{"args".*"fakeClaude":true\\}/); return m && m[0] })()`,
    20000,
  )
  const report = JSON.parse(raw)
  const env = report.env
  // Same folder, not the same spelling: CI temp paths use 8.3 short names (C:\Users\RUNNER~1) that NEXUS stores in long form.
  const real = (p) => { try { return realpathSync.native(p).toLowerCase() } catch { return String(p).toLowerCase() } }
  check('CLAUDE_CONFIG_DIR is the profile directory', !!env.CLAUDE_CONFIG_DIR && real(env.CLAUDE_CONFIG_DIR) === real(join(root, 'cfg')), env.CLAUDE_CONFIG_DIR)
  for (const k of ['CLAUDECODE', 'CLAUDE_CODE_SESSION_ID', 'CLAUDE_CODE_CHILD_SESSION', 'CLAUDE_CODE_MESSAGING_TOKEN', 'ANTHROPIC_API_KEY', 'OTEL_EXPORTER_OTLP_ENDPOINT']) {
    check(`${k} does not reach Claude`, env[k] === null, String(env[k]))
  }
  check('NEXUS assigns the Claude session id', report.args[0] === '--session-id' && /^[0-9a-f-]{36}$/.test(report.args[1]))
  check('work profile prompt appended', report.args.some((a) => a.startsWith('NEXUS work profile: PROTOTYPE')))
  check('session-scoped NEXUS MCP config passed', report.args.includes('--mcp-config'))
  check('session-scoped lifecycle hook settings passed', report.args.includes('--settings'))
  const appended = report.args[report.args.indexOf('--append-system-prompt') + 1] ?? ''
  check('approved always-on rule reaches the system prompt', appended.includes('SMOKE-RULE-BODY'))

  // Exit is recorded; a conversation without a transcript is not offered for resume.
  const session = await cdp.waitFor(`(async () => {
    const l = await window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })
    const s = l.find((x) => x.profileId === ${js(setup.profile.id)})
    return s && !s.running && s.state === 'exited' ? s : null
  })()`, 15000)
  check('exit code recorded', session.exitCode === 0)
  check('no transcript → not resumable', session.resumable === false && session.conversationRecorded === false)

  // With a transcript, Resume continues the same conversation.
  mkdirSync(join(root, 'cfg', 'projects', 'smoke'), { recursive: true })
  writeFileSync(join(root, 'cfg', 'projects', 'smoke', `${session.claudeSessionId}.jsonl`), '{}\n')
  const resumed = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_resume', { id: ${js(session.id)}, cols: 120, rows: 30 })`)
  check('resume creates a linked session', resumed.resumedFrom === session.id && resumed.claudeSessionId === session.claudeSessionId)

  // Cross-process: a checkpoint recorded through `nexus mcp` (as Claude would) appears in the running desktop.
  const { dataDir } = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('app_info')`)
  const mcp = spawn(nexusCli, ['--data-dir', dataDir, 'mcp', '--session', session.id], { stdio: ['pipe', 'pipe', 'ignore'] })
  for (const m of [
    { jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'smoke', version: '1' } } },
    { jsonrpc: '2.0', method: 'notifications/initialized' },
    { jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'checkpoint', arguments: { kind: 'decision', summary: 'Smoke checkpoint from MCP' } } },
    { jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'create_task', arguments: { title: 'Smoke task from Claude', priority: 'high' } } },
  ]) {
    mcp.stdin.write(`${js(m)}\n`)
  }
  mcp.stdin.end()
  await new Promise((r) => mcp.on('exit', r))
  await cdp.evaluate(`location.hash = '#/projects/${setup.project.id}', true`)
  const shown = await cdp.waitFor(`document.body.innerText.includes('Smoke checkpoint from MCP')`, 15000).catch(() => false)
  check('MCP checkpoint appears in the desktop project view', !!shown)
  await cdp.evaluate(`location.hash = '#/knowledge', true`)
  const claudeTask = await cdp.waitFor(`document.querySelector('[data-status="backlog"]')?.innerText.includes('Smoke task from Claude')`, 15000).catch(() => false)
  check('task Claude created through MCP appears on the desktop board', !!claudeTask)

  // Cross-process: a `nexus launch` session in another terminal is shown live (owned by the CLI host).
  const cliRun = spawn(nexusCli, ['--data-dir', dataDir, 'launch', '--project', setup.project.name, '--name', 'From a terminal', '-q'], {
    stdio: ['pipe', 'ignore', 'pipe'],
    env: { ...process.env, FAKE_CLAUDE_STAY: '1', FAKE_CLAUDE_REPORT: join(root, 'cli-report.json') },
  })
  let cliErr = ''
  cliRun.stderr.on('data', (d) => (cliErr += d))
  await cdp.evaluate(`location.hash = '#/sessions', true`)
  const live = await cdp
    .waitFor(`[...document.querySelectorAll('.page .session')].some((r) => r.innerText.includes('From a terminal') && r.innerText.includes('Running in a terminal'))`, 15000)
    .catch(() => false)
  const rowsNow = live ? '' : await cdp.evaluate(`[...document.querySelectorAll('.page .session')].map((r) => r.innerText.replace(/\\s+/g, ' ')).join(' | ')`)
  let diag = ''
  if (!live) {
    const fromCli = execFileSync(nexusCli, ['--data-dir', dataDir, '--json', 'sessions', '--all'], { encoding: 'utf8' })
    const titles = JSON.parse(fromCli).map((x) => `${x.title}:${x.state}:${x.host}:${x.workspaceId?.slice(0, 8)}`)
    diag = `${rowsNow} || cli alive=${cliRun.exitCode === null} stderr=${cliErr.trim()} || db: ${titles.join(', ')}`
  }
  check('terminal session visible live in the desktop', !!live, diag)
  // Cross-process: a Notification hook (as Claude would run it) marks the terminal session as waiting for the user.
  const cliSession = JSON.parse(execFileSync(nexusCli, ['--data-dir', dataDir, '--json', 'sessions'], { encoding: 'utf8' })).find(
    (x) => x.title === 'From a terminal',
  )
  if (cliSession) {
    const hookRun = spawn(nexusCli, ['--data-dir', dataDir, 'hook', '--session', cliSession.id, 'notification'], { stdio: ['pipe', 'ignore', 'ignore'] })
    hookRun.stdin.end(js({ hook_event_name: 'Notification', message: 'Claude needs your permission to use Bash' }))
    await new Promise((r) => hookRun.on('exit', r))
  }
  const waiting = await cdp
    .waitFor(`[...document.querySelectorAll('.page .session')].some((r) => r.innerText.includes('From a terminal') && r.innerText.includes('Permission required'))`, 15000)
    .catch(() => false)
  check('Notification hook shows “Permission required” live in the desktop', !!waiting)
  cliRun.stdin.end('bye\n') // fake-claude reads one line and exits
  await new Promise((r) => cliRun.on('exit', r))
} finally {
  cdp.close()
}
return failures
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const failures = await runSmoke()
  console.log(failures === 0 ? '\nSMOKE OK' : `\nSMOKE FAILED (${failures})`)
  process.exitCode = failures === 0 ? 0 : 1
}
