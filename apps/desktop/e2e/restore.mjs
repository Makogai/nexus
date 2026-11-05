// End-to-end new-computer restoration (Portable Workspace P8) against the REAL app, twice, with separate data folders:
//   Computer A — a workspace from AI Engineering Pro, a project with a remote, a Claude session (fake CLI, real hook and
//                MCP) that leaves a development record, a task; the workspace is pushed to a local bare remote.
//   Computer B — a fresh NEXUS opens the workspace from Git in onboarding → the Restore view lists every step, what came
//                with the workspace and what cannot be recovered → missing projects are cloned from their remotes with
//                their ids → the workspace skills are registered for review and approved → synced constitutions are
//                approved → a profile is added → the project's history from computer A is here.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/restore.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Temporary folders and local bare remotes; never touches real profiles.
import { execFileSync, spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('restore: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
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
const root = mkdtempSync(join(tmpdir(), 'nexus-restore-'))
const url = (p) => `file:///${p.replace(/\\/g, '/')}`
function git(dir, ...args) {
  const r = spawnSync('git', ['-c', 'user.name=E2E', '-c', 'user.email=e2e@example.invalid', '-c', 'init.defaultBranch=main', '-c', 'core.autocrlf=false', ...args], { cwd: dir, encoding: 'utf8' })
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`)
}

/** One NEXUS desktop with its own data folder and DevTools port. */
async function start(dataDir, port) {
  let log = ''
  const child = spawn(app, [], {
    env: { ...process.env, NEXUS_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_STAY: '1', FAKE_CLAUDE_REPORT: join(dataDir, '..', `fake-${port}.json`) },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  child.stdout.on('data', (d) => (log += d))
  child.stderr.on('data', (d) => (log += d))
  let cdp
  try {
    await waitForPort(String(port), 120_000)
    cdp = await connect(String(port))
    await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 60000)
  } catch (e) {
    // Never leave the app running (its pipes would keep this script alive): stop it, then report.
    cdp?.close()
    try {
      execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
    } catch {}
    throw new Error(`${e.message}\n--- app log ---\n${log.slice(-2000)}`)
  }
  const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
  const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el || el.disabled) return false; el.click(); return true })()`)
  const setValue = (sel, value) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event('input', { bubbles: true })); return true })()`)
  const text = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
  const stop = async () => {
    cdp.close()
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
  return { cdp, inv, click, setValue, text, stop, log: () => log, dataDir }
}

function cli(dataDir, args, stdin) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, ...args], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', () => resolve(out))
    p.stdin.end(stdin)
  })
}

let a = null
let b = null
try {
  // ---------------- Computer A ----------------
  const projBare = join(root, 'remotes', 'shop.git')
  const wsBare = join(root, 'remotes', 'workspace.git')
  for (const d of [projBare, wsBare]) {
    mkdirSync(d, { recursive: true })
    git(d, 'init', '-q', '--bare')
  }
  const shopA = join(root, 'a-shop')
  mkdirSync(join(shopA, 'src'), { recursive: true })
  writeFileSync(join(shopA, 'README.md'), '# Shop\n\nA small web shop.\n')
  writeFileSync(join(shopA, 'package.json'), js({ name: 'shop', dependencies: { vue: '^3' } }))
  git(shopA, 'init', '-q')
  git(shopA, 'add', '-A')
  git(shopA, 'commit', '-q', '-m', 'init')
  git(shopA, 'remote', 'add', 'origin', url(projBare))
  git(shopA, 'push', '-q', '-u', 'origin', 'main')

  a = await start(join(root, 'a-data'), 9344)
  const wsA = await a.inv('workspace_create', { path: join(root, 'a-ws'), name: 'Portable', description: null, template: 'ai-engineering-pro' })
  const shop = await a.inv('project_register', { workspaceId: wsA.id, path: shopA, name: 'shop' })
  await a.inv('knowledge_create', { workspaceId: wsA.id, input: { kind: 'task', title: 'Add a discount code field', projectId: shop.id, body: '' } })
  mkdirSync(join(root, 'a-cfg'), { recursive: true })
  const profA = await a.inv('profile_create', { input: { name: 'Fake', configDir: join(root, 'a-cfg'), executable: fakeClaude } })
  const s = await a.inv('session_start_claude', { args: { profileId: profA.id, projectId: shop.id, title: 'Cart work', cols: 100, rows: 30 } })
  await sleep(1500)
  await cli(a.dataDir, ['hook', '--session', s.id, 'post-tool-use'], js({ session_id: s.claudeSessionId, tool_name: 'Edit', tool_input: { file_path: join(shopA, 'src', 'cart.ts') } }))
  await cli(a.dataDir, ['mcp', '--session', s.id], [
    js({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } }),
    js({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'checkpoint', arguments: { kind: 'decision', summary: 'Keep the cart in the URL for sharing' } } }),
  ].join('\n') + '\n')
  await a.inv('session_terminate', { id: s.id })
  const devA = join(root, 'a-ws', 'project-context', 'shop', 'development')
  for (let i = 0; i < 80 && !(existsSync(devA) && readdirSync(devA).length); i++) await sleep(250)
  check('computer A: the session left a development record', existsSync(devA) && readdirSync(devA).length === 1)
  git(join(root, 'a-ws'), 'init', '-q')
  git(join(root, 'a-ws'), 'add', '-A')
  git(join(root, 'a-ws'), 'commit', '-q', '-m', 'workspace')
  git(join(root, 'a-ws'), 'remote', 'add', 'origin', url(wsBare))
  git(join(root, 'a-ws'), 'push', '-q', '-u', 'origin', 'main')
  await a.stop()
  a = null

  // ---------------- Computer B ----------------
  b = await start(join(root, 'b-data'), 9345)
  check('computer B starts at the onboarding screen', !!(await b.cdp.evaluate(`!!document.querySelector('[data-onboarding-clone]')`)))
  await b.click('[data-onboarding-clone]')
  await b.cdp.waitFor(`document.querySelector('[data-sync-clone-url]')`, 10000)
  await b.setValue('[data-sync-clone-url]', url(wsBare))
  await b.setValue('[data-sync-clone-parent]', root)
  await b.setValue('[data-sync-clone-name]', 'b-ws')
  await b.click('[data-sync-clone-start]')
  const steps = await b.cdp.waitFor(`(() => { const s = [...document.querySelectorAll('[data-restore-step]')]; return s.length > 5 ? s.map((e) => e.dataset.restoreStep + ':' + e.dataset.restoreStatus).join(' ') : '' })()`, 90000).catch(() => '')
  const where = steps ? '' : await b.cdp.evaluate(`location.hash + ' | ' + (document.querySelector('[data-sync-clone-error]')?.innerText ?? '') + ' | ' + document.body.innerText.slice(0, 300).replace(/\s+/g, ' ')`)
  check('opening the workspace from Git leads to the Restore view', steps.length > 0, steps || where)
  check('…schema and index are done; approvals, skills, projects and profiles are to do', ['schema:done', 'index:done', 'approvals:todo', 'skills:todo', 'projects:todo', 'profiles:todo'].every((x) => steps.includes(x)), steps)
  const restored = await b.text('[data-restore-restored]')
  check('…it says what came with the workspace', restored.includes('27 constitutions') && restored.includes('1 development records') && restored.includes('11 skills'), restored.replace(/\s+/g, ' '))
  const lost = await b.text('[data-restore-not-recoverable]')
  check('…and what cannot be recovered from it', lost.includes('transcripts') && lost.includes('Uncommitted'), lost.slice(0, 120))

  // Projects: clone the missing one from its remote — same id as on computer A.
  await b.click('[data-restore-repair="clone-missing"]')
  const plan = await b.cdp.waitFor(`document.querySelector('[data-restore-plan]')?.innerText`, 15000).catch(() => '')
  check('cloning the projects shows its plan first', plan.includes('git clone') && plan.includes('projects/shop'), plan)
  await b.click('[data-restore-apply]')
  await b.cdp.waitFor(`document.querySelector('[data-restore-step="projects"]')?.dataset.restoreStatus === 'done'`, 90000).catch(() => undefined)
  const wsB = (await b.inv('workspace_active'))
  const shopB = (await b.inv('project_list', { workspaceId: wsB.id })).find((p) => p.name === 'shop')
  check('…the project is cloned under projects/ and keeps its id', shopB?.id === shop.id && shopB?.available && existsSync(join(root, 'b-ws', 'projects', 'shop', 'README.md')), js(shopB && [shopB.id === shop.id, shopB.localPath]))

  // Skills: register for review, then approve (as in AI Kits).
  await b.click('[data-restore-repair="register-workspace-skills"]')
  await b.cdp.waitFor(`document.querySelector('[data-restore-plan]')`, 10000)
  await b.click('[data-restore-apply]')
  await sleep(1500)
  const kit = (await b.inv('kit_list')).find((k) => k.workspaceId === wsB.id)
  check('…the workspace skills are registered but not approved yet', kit?.status === 'needs-review', kit?.status)
  await b.inv('kit_approve', { id: kit.id })
  // Approvals: the person reviews the synced constitutions and workflows.
  const pending = (await b.inv('knowledge_overview', { workspaceId: wsB.id, rebuild: false })).items?.filter((i) => i.valid && i.scope !== 'global' && (i.kind === 'rule' || i.kind === 'workflow')) ?? []
  for (const i of pending) await b.inv('knowledge_approve', { workspaceId: wsB.id, id: i.id })
  mkdirSync(join(root, 'b-cfg'), { recursive: true })
  await b.inv('profile_create', { input: { name: 'Work', configDir: join(root, 'b-cfg'), executable: fakeClaude } })
  await b.click('[data-restore-refresh]')
  const after = await b.cdp.waitFor(`(() => { const s = [...document.querySelectorAll('[data-restore-step]')].map((e) => e.dataset.restoreStep + ':' + e.dataset.restoreStatus).join(' '); return s.includes('profiles:done') ? s : '' })()`, 60000).catch(() => '')
  check('after the steps, approvals, skills, projects and profiles are done', ['approvals:done', 'skills:done', 'projects:done', 'profiles:done'].every((x) => after.includes(x)), after)

  // Continue: the project's history from computer A is here.
  const h = await b.inv('project_history', { workspaceId: wsB.id, id: shop.id })
  check('the development record from computer A is in the project history', h.records.length === 1 && h.tasks.some((t) => t[1] === 'Add a discount code field'), js(h.records.map((r) => r.title)))
  await b.cdp.evaluate(`location.hash = '#/projects/${shop.id}/history'; true`)
  await b.cdp.waitFor(`document.querySelector('[data-history-records] [data-record]')`, 20000).catch(() => undefined)
  await b.click('[data-history-records] .record-head')
  const body = await b.cdp.waitFor(`document.querySelector('[data-record-body]')?.textContent`, 15000).catch(() => '')
  check('…and opens with what was decided there (reported, unverified)', body.includes('Keep the cart in the URL for sharing'), body.slice(0, 160))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  if (a) await a.stop().catch(() => undefined)
  if (b) await b.stop().catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log (B) ---\n${(b?.log() ?? '').slice(-3000)}`)
console.log(failures === 0 ? '\nRESTORE OK' : `\nRESTORE FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
