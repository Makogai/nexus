// Desktop E2E for Agents & Dev Environment (ADR-0051, ADR-0052) against the REAL app (real IPC, real UI):
// a Codex profile is added on the Profiles page (temporary home, fake-codex as the executable), Codex is launched from
// the project page, fake-codex parses NEXUS's -c overrides like Codex (asks the NEXUS MCP server for its tools, runs the
// notify program after a "turn"), the session shows as Codex and becomes resumable, Resume continues the thread; the Dev
// Environment page lists this computer's tools with the project's needs. Never a real Codex or Claude profile.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/agents.mjs [path/to/nexus-desktop(.exe)]
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('agents: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9333'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeCodex = fileURLToPath(new URL('../../../target/debug/fake-codex.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
if (!existsSync(fakeCodex)) {
  console.error(`agents: ${fakeCodex} is missing — cargo build -p fake-claude`)
  process.exit(2)
}

const root = mkdtempSync(join(tmpdir(), 'nexus-agents-'))
const dataDir = join(root, 'data')
const repo = join(root, 'shop')
const codexHome = join(root, 'codex-home')
const report = join(root, 'codex-report.json')
mkdirSync(repo, { recursive: true })
mkdirSync(codexHome, { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'shop', engines: { node: '>=18' } }))
writeFileSync(join(repo, '.nvmrc'), '1.0.0\n') // a Node.js version no computer has: an unmet need
execFileSync('git', ['init', '-q'], { cwd: repo })

let log = ''
const child = spawn(app, [], {
  env: {
    ...process.env,
    NEXUS_DATA_DIR: dataDir,
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
    FAKE_CODEX_REPORT: report,
    FAKE_CODEX_MCP: '1',
    FAKE_CODEX_TURN: '1',
    OPENAI_API_KEY: 'sk-e2e-ambient',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))

async function readReport() {
  const deadline = Date.now() + 30000
  for (;;) {
    try {
      return JSON.parse(readFileSync(report, 'utf8'))
    } catch {
      if (Date.now() > deadline) throw new Error('fake-codex did not report')
      await new Promise((r) => setTimeout(r, 200))
    }
  }
}
const setValue = (sel, v) => `(() => { const el = document.querySelector(${js(sel)}); el.value = ${js(v)}; el.dispatchEvent(new Event('input', { bubbles: true })); return true })()`

let cdp
try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const setup = await cdp.evaluate(`(async () => {
    const inv = window.__TAURI_INTERNALS__.invoke
    const ws = await inv('workspace_create', { path: ${js(join(root, 'ws'))}, name: 'Shop', description: null })
    const project = await inv('project_register', { workspaceId: ws.id, path: ${js(repo)}, name: null })
    return { project }
  })()`)

  // 1. Profiles page → Other agents → add a Codex profile through the form.
  await cdp.evaluate(`location.hash = '#/profiles'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-agent="codex"] [data-capability="tools"]')`, 30000)
  const caps = await cdp.evaluate(`[...document.querySelectorAll('[data-agent="codex"] [data-capability]')].map((c) => c.dataset.capability + ':' + c.dataset.support).join(' ')`)
  check('Codex says what its sessions get', caps.includes('tools:full') && caps.includes('attention:partial') && caps.includes('usage:unavailable'), caps)
  await cdp.evaluate(`document.querySelector('[data-agent-add="codex"]').click(), true`)
  await cdp.waitFor(`document.querySelector('[data-agent-form]')`, 5000)
  await cdp.evaluate(setValue('[data-agent-field="name"]', 'Fake Codex'))
  await cdp.evaluate(setValue('[data-agent-field="home"]', codexHome))
  await cdp.evaluate(setValue('[data-agent-field="executable"]', fakeCodex))
  await cdp.evaluate(setValue('[data-agent-field="args"]', '--sandbox danger-full-access'))
  await cdp.evaluate(`document.querySelector('[data-agent-save]').click(), true`)
  const refusal = await cdp.waitFor(`document.querySelector('[data-agent-error]')?.innerText`, 10000).catch(() => '')
  check('a sandbox bypass is refused and the reason stays visible', refusal.includes('danger-full-access'), refusal)
  await cdp.evaluate(setValue('[data-agent-field="args"]', '--search'))
  await cdp.evaluate(`document.querySelector('[data-agent-save]').click(), true`)
  const added = await cdp.waitFor(`document.querySelector('[data-agent-profile="Fake Codex"]')?.innerText`, 10000).catch(() => '')
  check('the Codex profile is registered with its home', added.includes('codex-home'), added)

  // 2. Launch Codex from the project page (no Claude profile exists: the dialog starts with Codex).
  if (existsSync(report)) unlinkSync(report)
  await cdp.evaluate(`location.hash = '#/projects/${setup.project.id}'; true`)
  await cdp.waitFor(`document.querySelector('[data-project-launch]')`, 15000)
  await cdp.evaluate(`document.querySelector('[data-project-launch]').click(), true`)
  await cdp.waitFor(`document.querySelector('[data-launch-agent-profile]')`, 10000)
  const title = await cdp.evaluate(`document.querySelector('.modal h2, .modal-title, [role="dialog"] h2')?.innerText ?? ''`)
  check('the launch dialog names Codex', title.includes('Codex'), title)
  await cdp.evaluate(`[...document.querySelectorAll('footer .btn-primary')].pop().click(), true`)
  const r = await readReport()
  const o = r.overrides ?? {}
  check('Codex gets the NEXUS MCP server through -c overrides and sees its tools', typeof o['mcp_servers.nexus.command'] === 'string' && (r.mcpTools ?? []).includes('get_task'), r.mcpError ?? `${(r.mcpTools ?? []).length} tools`)
  check('…NEXUS context as developer instructions', String(o.developer_instructions ?? '').includes('propose_commit_message'))
  check('…its profile home and options, and a clean environment', r.env.CODEX_HOME === codexHome && r.args[0] === '--search' && r.env.OPENAI_API_KEY === null, js(r.env))
  check('…and the notify program ran after the turn', r.notified === true)

  // 3. The session shows as Codex, with the turn counted and Resume offered.
  await cdp.evaluate(`location.hash = '#/sessions'; true`)
  const row = await cdp.waitFor(`(() => { const r = [...document.querySelectorAll('[data-session]')].find((x) => x.querySelector('[data-session-agent="codex"]')); return r && r.querySelector('[data-session-resume]') ? r.dataset.session : null })()`, 20000).catch(() => null)
  check('the Codex session is listed as Codex and resumable', !!row)
  const sessions = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })`)
  const first = sessions.find((s) => s.id === row)
  check('its conversation id came from Codex', first?.claudeSessionId === '0199a213-81c0-7800-8aa1-bbab2a035a53' && first?.turns === 1, js({ id: first?.claudeSessionId, turns: first?.turns }))

  // 4. Resume continues the thread.
  if (existsSync(report)) unlinkSync(report)
  await cdp.evaluate(`document.querySelector('[data-session="${row}"] [data-session-resume]').click(), true`)
  const r2 = await readReport()
  check('Resume runs `codex resume <thread>` with fresh overrides', r2.args[0] === 'resume' && r2.args.at(-1) === first?.claudeSessionId && !!r2.overrides?.['mcp_servers.nexus.args'], r2.args.join(' '))
  const tab = await cdp.waitFor(`document.querySelector('[data-tab-agent="codex"]') ? true : null`, 10000).catch(() => null)
  check('the terminal tab says Codex', !!tab)

  // 5. Dev Environment: this computer's tools with the project's needs.
  await cdp.evaluate(`location.hash = '#/environment'; true`)
  await cdp.waitFor(`document.querySelector('[data-devenv-tool="git"]')`, 60000)
  const git = await cdp.evaluate(`document.querySelector('[data-devenv-tool="git"]').dataset.installed`)
  check('Git is found on this computer', git === 'yes')
  await cdp.evaluate(`document.querySelector('[data-devenv-filter="needed"]').click(), true`)
  const needs = await cdp.evaluate(`[...document.querySelectorAll('[data-devenv-tool="node"] [data-devenv-need]')].map((n) => n.dataset.devenvNeed + ' ' + n.innerText.replace(/\\s+/g, ' ')).join(' | ')`)
  check('the project needs Node.js 1.0.0 from .nvmrc, which no computer has', /^(mismatch|missing) .*shop needs 1\.0\.0 \(\.nvmrc\)/.test(needs), needs)
  const shown = await cdp.evaluate(`[...document.querySelectorAll('[data-devenv-tool]')].map((t) => t.dataset.devenvTool).join(',')`)
  check('the Needed filter shows only what the project uses', shown.split(',').includes('node') && !shown.split(',').includes('kubectl'), shown)
  await cdp.evaluate(`document.querySelector('[data-devenv-filter="all"]').click(), true`)
  await cdp.waitFor(`document.querySelector('[data-devenv-check="git"]')`, 5000)
  await cdp.evaluate(`document.querySelector('[data-devenv-check="git"]').click(), true`)
  const again = await cdp.waitFor(`document.querySelector('[data-devenv-check="git"]') && !document.querySelector('[data-devenv-check="git"]').disabled && document.querySelector('[data-devenv-tool="git"]').dataset.installed === 'yes'`, 30000).catch(() => false)
  check('Check again probes one tool and keeps the view', !!again)
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nAGENTS & DEV ENVIRONMENT OK' : `\nAGENTS & DEV ENVIRONMENT FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
await new Promise((r) => setTimeout(r, 1500))
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
