// End-to-end notifications and multi-session attention against the REAL app (ADR-0025):
//   three synthetic Claude sessions (two workspaces, two profiles) → attention reported by real `nexus hook`
//   processes (exactly what Claude runs) → in-app center, session badges, desktop delivery through the recording
//   adapter (no OS toasts), duplicate suppression, muting, quiet hours, click navigation (incl. a workspace switch
//   and a session that no longer exists), a session exit, and a NEXUS crash + restart.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/attention.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { DatabaseSync } from 'node:sqlite'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('attention: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9334'
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

const root = mkdtempSync(join(tmpdir(), 'nexus-attention-'))
const dataDir = join(root, 'data')
for (const d of ['shop', 'lab', 'cfg-work', 'cfg-personal']) mkdirSync(join(root, d), { recursive: true })
for (const p of ['shop', 'lab']) writeFileSync(join(root, p, 'README.md'), `# ${p}\n`)

let log = ''
function startApp() {
  const child = spawn(app, [], {
    env: {
      ...process.env,
      NEXUS_DATA_DIR: dataDir,
      NEXUS_E2E_NOTIFIER: 'record',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      FAKE_CLAUDE_REPORT: join(root, 'report.json'),
      FAKE_CLAUDE_STAY: '1',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  child.stdout.on('data', (d) => (log += d))
  child.stderr.on('data', (d) => (log += d))
  return child
}
async function stopApp(child) {
  execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  const deadline = Date.now() + 20000
  for (;;) {
    try {
      await fetch(`http://127.0.0.1:${port}/json`)
    } catch {
      return
    }
    if (Date.now() > deadline) throw new Error('debugging port still open after stopping the app')
    await sleep(300)
  }
}
/** Run `nexus hook` exactly as Claude does (payload on stdin). Returns its stdout. */
function hook(sessionId, event, payload) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, 'hook', '--session', sessionId, event], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', (code) => resolve({ code, out }))
    p.stdin.end(js(payload))
  })
}

let child = startApp()
let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const notifications = (filter = {}) => inv('notifications_list', { filter: { includeDismissed: true, limit: 500, ...filter } })
const recorded = () => inv('e2e_notifications_recorded')
async function until(fn, ms = 10000) {
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
  const ws2 = await inv('workspace_create', { path: join(root, 'ws-lab'), name: 'Lab', description: null })
  const lab = await inv('project_register', { workspaceId: ws2.id, path: join(root, 'lab'), name: null })
  const ws1 = await inv('workspace_create', { path: join(root, 'ws-shop'), name: 'Shop', description: null })
  const shop = await inv('project_register', { workspaceId: ws1.id, path: join(root, 'shop'), name: null })
  const work = await inv('profile_create', { input: { name: 'Work', configDir: join(root, 'cfg-work'), executable: fakeClaude } })
  const personal = await inv('profile_create', { input: { name: 'Personal', configDir: join(root, 'cfg-personal'), executable: fakeClaude } })
  const start = (profileId, projectId, title) => inv('session_start_claude', { args: { profileId, projectId, title, cols: 100, rows: 30 } })
  const s1 = await start(work.id, shop.id, 'Checkout')
  const s2 = await start(personal.id, shop.id, 'Blog')
  const s3 = await start(work.id, lab.id, 'Lab experiment')
  await cdp.evaluate(`location.hash = '#/'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell')`, 20000)
  check('three synthetic sessions run with hooks enabled', [s1, s2, s3].every((s) => s.running && s.integrations?.hooks?.state === 'enabled'))
  const cs = (s) => s.claudeSessionId

  // 1. Background sessions report attention through the real hook handler.
  const outs = [
    await hook(s1.id, 'permission-request', { session_id: cs(s1), tool_name: 'Bash', tool_input: { command: 'deploy --token s3cr3t' } }),
    await hook(s1.id, 'notification', { session_id: cs(s1), notification_type: 'permission_prompt', message: 'Claude needs your permission to use Bash' }),
    await hook(s2.id, 'stop', { session_id: cs(s2) }),
    await hook(s3.id, 'ask-user', { session_id: cs(s3), tool_name: 'AskUserQuestion', tool_input: { questions: [] } }),
  ]
  check('hooks exit 0 and print nothing (NEXUS never decides or answers)', outs.every((o) => o.code === 0 && o.out === ''))
  const rec = await until(async () => {
    const r = await recorded()
    return r.length >= 3 ? r : null
  })
  const cats = (rec ?? []).map((n) => `${n.category}:${n.sessionId === s1.id ? 's1' : n.sessionId === s2.id ? 's2' : 's3'}`).sort()
  check('desktop notifications for the three background sessions', js(cats) === js(['agent-finished:s2', 'input-required:s3', 'permission-requested:s1']), cats.join(' '))
  const perm = (await notifications()).find((n) => n.category === 'permission-requested')
  check('the permission prompt is one notification (×2), delivered once', perm?.occurrences === 2 && rec.filter((n) => n.category === 'permission-requested').length === 1)
  check('no tool input is stored or shown', !js(await notifications()).includes('s3cr3t'))
  const byS = Object.fromEntries((await notifications()).map((n) => [n.sessionId, n]))
  check('identity comes from the session: workspace and profile', byS[s2.id]?.profileId === personal.id && byS[s3.id]?.workspaceId === ws2.id && byS[s1.id]?.workspaceId === ws1.id)

  // 2. In-app: bell counts, attention board, session badges.
  const bell = await cdp.waitFor(`document.querySelector('[data-testid="bell-action"]')?.innerText`, 10000).catch(() => '')
  check('the bell shows 2 notifications that need action', bell === '2', bell)
  await cdp.evaluate(`location.hash = '#/notifications', true`)
  const board = await cdp
    .waitFor(`(() => { const t = (id) => document.querySelector('[data-board-session="' + id + '"] .tile-status')?.innerText; const r = [t(${js(s1.id)}), t(${js(s2.id)}), t(${js(s3.id)})]; return r.every(Boolean) && r.join('|') })()`, 10000)
    .catch(() => '')
  check('the attention board shows each session’s reported state', board.toLowerCase() === 'permission required|your turn|waiting for input', board)
  await cdp.evaluate(`location.hash = '#/sessions', true`)
  const badge = await cdp.waitFor(`document.querySelector('[data-session="${s1.id}"] .attention')?.innerText`, 10000).catch(() => '')
  check('the session row shows “Permission required”', badge === 'Permission required', badge)

  // 3. Click on a desktop notification of the other workspace: focus → workspace → session terminal.
  const n3 = (await notifications()).find((n) => n.sessionId === s3.id && n.category === 'input-required')
  await inv('e2e_notification_activate', { id: n3.id })
  const switched = await until(async () => (await inv('workspace_active'))?.id === ws2.id)
  const tab3 = await cdp.waitFor(`document.querySelector('[data-tab-session="${s3.id}"]')?.classList.contains('active')`, 10000).catch(() => false)
  check('clicking a desktop notification switches workspace and shows that terminal', !!switched && !!tab3)
  check('…and marks it read', !!(await notifications()).find((n) => n.id === n3.id)?.readAt)

  // 4. Click in the notification center: back to the first workspace's session.
  await cdp.evaluate(`document.querySelector('[data-testid="bell"]').click(), true`)
  await cdp.waitFor(`document.querySelector('.panel [data-notification="${perm.id}"]')`, 5000)
  await cdp.evaluate(`document.querySelector('.panel [data-notification="${perm.id}"] .body').click(), true`)
  const tab1 = await cdp.waitFor(`document.querySelector('[data-tab-session="${s1.id}"]')?.classList.contains('active')`, 10000).catch(() => false)
  check('clicking in the center opens the session in its workspace', !!tab1 && (await inv('workspace_active'))?.id === ws1.id)

  // 4b. A click from the OS notification center after a restart arrives as a second start with
  //     `nexus://notification/<id>` (installed builds register the protocol): the running NEXUS routes it.
  const n2 = (await notifications()).find((n) => n.sessionId === s2.id && n.category === 'agent-finished')
  await new Promise((resolve) => {
    const second = spawn(app, [`nexus://notification/${n2.id}`], { env: { ...process.env, NEXUS_DATA_DIR: dataDir, NEXUS_E2E_NOTIFIER: 'record' }, stdio: 'ignore' })
    second.on('exit', resolve)
    setTimeout(resolve, 15000)
  })
  const tab2 = await cdp.waitFor(`document.querySelector('[data-tab-session="${s2.id}"]')?.classList.contains('active')`, 10000).catch(() => false)
  check('a nexus:// activation from a second start opens that session', !!tab2)

  // 5. Focus suppression: the visible session in a focused window gets no toast.
  await inv('notifications_set_view', { visibleSession: s1.id, utcOffsetMinutes: new Date().getTimezoneOffset() })
  await hook(s1.id, 'post-tool-use', { session_id: cs(s1), tool_name: 'Bash', tool_input: { command: 'ls' } })
  const resolved = (await notifications()).find((n) => n.id === perm.id)
  check('Claude continuing resolves the permission notification', !!resolved?.resolvedAt)
  const withdrawn = await until(async () => ((await inv('e2e_notifications_withdrawn')).includes(perm.id) ? true : null))
  check('…and its desktop notification is withdrawn from the notification center', !!withdrawn)
  // The OS decides whether the window has focus (a background test run usually has none): check the rule for
  // the state the backend actually sees.
  const { windowFocused: focused, visibleSession } = await inv('e2e_focus_state')
  check('the backend knows which terminal is visible', visibleSession === s1.id, visibleSession)
  await hook(s1.id, 'stop', { session_id: cs(s1) })
  const fin1 = await until(async () => (await notifications()).find((n) => n.sessionId === s1.id && n.category === 'agent-finished' && n.desktopState !== 'pending'))
  check(
    focused ? 'visible session in the focused window: suppressed' : 'window not focused: the visible session still notifies',
    focused ? fin1?.desktopState === 'suppressed' && fin1?.desktopReason === 'the user is looking at it' : fin1?.desktopState === 'sent',
    `${fin1?.desktopState} ${fin1?.desktopReason ?? ''}`,
  )

  // 6. Muting a profile and quiet hours keep notifications in the inbox only.
  const prefs = await inv('notifications_prefs')
  await inv('notifications_set_prefs', { prefs: { ...prefs, mutedProfiles: [personal.id] } })
  await hook(s2.id, 'user-prompt', { session_id: cs(s2) })
  await hook(s2.id, 'permission-request', { session_id: cs(s2), tool_name: 'Write' })
  const muted = await until(async () => (await notifications()).find((n) => n.sessionId === s2.id && n.category === 'permission-requested' && n.desktopState !== 'pending'))
  check('a muted profile: inbox yes, desktop no', muted?.desktopState === 'suppressed' && muted?.desktopReason === 'muted workspace or profile', muted?.desktopReason)
  const now = new Date()
  const hhmm = (d) => `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
  await inv('notifications_set_prefs', { prefs: { ...prefs, quietHours: [hhmm(new Date(now - 3600e3)), hhmm(new Date(+now + 3600e3))] } })
  await hook(s3.id, 'stop-failure', { session_id: cs(s3), error: 'rate_limit' })
  const quiet = await until(async () => (await notifications()).find((n) => n.sessionId === s3.id && n.category === 'process-issue' && n.desktopState !== 'pending'))
  check('quiet hours hold desktop notifications back', quiet?.desktopState === 'suppressed' && quiet?.desktopReason === 'quiet hours', quiet?.desktopReason)
  await inv('notifications_set_prefs', { prefs })

  // 6b. Lock-screen policy "always generic": the desktop text names no project, session or tool.
  await inv('notifications_set_view', { visibleSession: null, utcOffsetMinutes: new Date().getTimezoneOffset() })
  await inv('notifications_set_prefs', { prefs: { ...prefs, lockScreen: 'always-generic' } })
  await hook(s1.id, 'permission-request', { session_id: cs(s1), tool_name: 'WebFetch' })
  const generic = await until(async () => (await recorded()).find((n) => n.sessionId === s1.id && n.category === 'permission-requested' && n.title === 'NEXUS'))
  check('generic desktop text names nothing about the work', !!generic && !/WebFetch|Checkout|Shop/.test(`${generic.title} ${generic.body}`), generic ? `${generic.title}: ${generic.body}` : '')
  const inApp = (await notifications()).find((n) => n.id === generic?.id)
  check('…while the in-app notification keeps the details', inApp?.title === 'Permission requested: WebFetch')
  await inv('notifications_set_prefs', { prefs })

  // 7. A notification whose session no longer exists.
  const db = new DatabaseSync(join(dataDir, 'nexus.db'))
  const t = Date.now()
  db.prepare(
    `INSERT INTO notifications(id, category, priority, source, title, body, workspace_id, target_kind, target_id, created_at, updated_at, desktop_state)
     VALUES ('gone-1', 'agent-finished', 'success', 'hook', 'Old session finished', '', ?, 'session', 'no-such-session', ?, ?, 'suppressed')`,
  ).run(ws1.id, t, t)
  db.close()
  await cdp.evaluate(`location.hash = '#/notifications', true`)
  await cdp.waitFor(`document.querySelector('.page [data-notification="gone-1"]')`, 10000)
  await cdp.evaluate(`document.querySelector('.page [data-notification="gone-1"] .body').click(), true`)
  const toast = await cdp.waitFor(`[...document.querySelectorAll('.toast')].map((t) => t.innerText).find((x) => x.includes('no longer exists'))`, 5000).catch(() => '')
  check('a vanished session is explained, not an error', !!toast, toast)

  // 8. A session ends: its waiting notifications are resolved and the end is recorded.
  await inv('session_write', { id: s2.id, data: 'bye\r', binary: false })
  const ended = await until(async () => (await notifications()).find((n) => n.sessionId === s2.id && n.category === 'session-finished'))
  const s2open = (await notifications()).filter((n) => n.sessionId === s2.id && ['permission-requested', 'agent-finished', 'input-required'].includes(n.category) && !n.resolvedAt)
  check('the finished session is recorded and nothing of it waits any more', !!ended && s2open.length === 0, `${s2open.length} open`)

  // 9. Crash and restart: notifications persist, nothing is delivered twice, interrupted sessions are announced.
  const before = (await recorded()).map((n) => n.id)
  const readBefore = (await notifications()).filter((n) => n.readAt).map((n) => n.id).sort()
  cdp.close()
  cdp = null
  await stopApp(child)
  child = startApp()
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.shell')`, 20000)
  const after = await until(async () => {
    const r = await recorded()
    return r.filter((n) => n.category === 'session-interrupted').length >= 2 ? r : null
  }, 15000)
  const interrupted = (after ?? []).filter((n) => n.category === 'session-interrupted').map((n) => n.sessionId).sort()
  check('after a crash the interrupted sessions are announced', js(interrupted) === js([s1.id, s3.id].sort()), interrupted.join(','))
  check('…and nothing from before the restart is delivered again', (after ?? []).every((n) => !before.includes(n.id)))
  const persisted = await notifications()
  check('notifications and their read state persist across the restart', readBefore.every((id) => persisted.find((n) => n.id === id)?.readAt) && persisted.some((n) => n.id === perm.id))
  check('…and the waiting states of the dead sessions are resolved', persisted.filter((n) => [s1.id, s3.id].includes(n.sessionId) && ['permission-requested', 'input-required', 'agent-finished'].includes(n.category) && !n.resolvedAt).length === 0)
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp(child).catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nATTENTION OK' : `\nATTENTION FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
