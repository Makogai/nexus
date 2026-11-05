// End-to-end token usage analytics against the REAL app (ADR-0030), synthetic profile and temporary repository:
//   the fake CLI writes transcripts in Claude Code's format into the synthetic profile → the desktop scanner reads
//   them → the Usage dashboard shows measured counts, Claude's cost estimate and unavailable sessions apart, by day,
//   project, context strategy; filters scope everything; the project page links to it; no text is stored; the
//   transcripts stay unchanged; ambient telemetry variables never reach Claude.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/usage.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('usage: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9338'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-usage-'))
const dataDir = join(root, 'data')
const repo = join(root, 'shop')
const cfg = join(root, 'cfg') // synthetic Claude config dir
const report = join(root, 'report.json')
const fixture = join(root, 'fixture.jsonl')
mkdirSync(repo, { recursive: true })
writeFileSync(join(repo, 'README.md'), '# Shop\n')
const git = (...args) => execFileSync('git', ['-c', 'user.name=T', '-c', 'user.email=t@example.invalid', '-c', 'core.autocrlf=false', ...args], { cwd: repo })
git('init', '-q', '-b', 'main')
git('add', '-A')
git('commit', '-qm', 'init')

const resp = (id, offset, [input, output, cw, cr], blocks = 1) =>
  Array.from({ length: blocks }, () =>
    js({
      type: 'assistant', isSidechain: false, sessionId: '{SESSION}', version: '2.1.311', timestamp: `{TS+${offset}}`, requestId: `req_${id}`,
      message: { id: `msg_${id}`, type: 'message', role: 'assistant', model: 'claude-opus-5-5', content: [{ type: 'text', text: 'SECRET-RESPONSE' }],
        usage: { input_tokens: input, output_tokens: output, cache_creation_input_tokens: cw, cache_read_input_tokens: cr } },
    }),
  ).join('\n')
const user = js({ type: 'user', sessionId: '{SESSION}', timestamp: '{TS}', message: { role: 'user', content: 'SECRET-PROMPT' } })
const cost = (usd) => js({ type: 'cost-state', sessionId: '{SESSION}', totalCostUSD: usd, startTime: '{START}', modelUsage: {}, hasUnknownModelCost: false }).replace('"{START}"', '{START}')
// A: balanced, two responses (the first written as two content blocks); B: deep, one larger response.
const A = [user, resp('a1', 1, [10, 200, 3000, 0], 2), resp('a2', 2, [5, 100, 0, 3100]), cost(0.05)].join('\n') + '\n'
const B = [user, resp('b1', 1, [40, 300, 9000, 0]), cost(0.1)].join('\n') + '\n'
const totalA = 10 + 200 + 3000 + 5 + 100 + 3100
const totalB = 40 + 300 + 9000

let log = ''
function startApp() {
  const child = spawn(app, [], {
    env: {
      ...process.env,
      NEXUS_DATA_DIR: dataDir,
      NEXUS_E2E_NOTIFIER: 'record',
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      FAKE_CLAUDE_REPORT: report,
      FAKE_CLAUDE_TRANSCRIPT: fixture,
      FAKE_CLAUDE_AUTH_METHOD: 'api_key', // the profile signs in with an API key (billed per use)
      // Ambient telemetry (as inside an organisation session): must never reach a NEXUS-launched Claude.
      CLAUDE_CODE_ENABLE_TELEMETRY: '1',
      OTEL_EXPORTER_OTLP_ENDPOINT: 'https://collector.example.invalid',
    },
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
const walk = (d) => (existsSync(d) ? readdirSync(d).flatMap((e) => (statSync(join(d, e)).isDirectory() ? walk(join(d, e)) : [join(d, e)])) : [])

let child = startApp()
let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const setValue = (sel, value) =>
  cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', { bubbles: true })); return true })()`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.click(); return true })()`)
const text = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
async function until(fn, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(300)
  }
}
async function launch(projectId, profileId) {
  const s = await inv('session_start_claude', { args: { profileId, projectId, workProfile: 'balanced', cols: 100, rows: 30 } })
  await until(async () => !(await inv('session_list', { filter: null })).find((x) => x.id === s.id)?.running)
  return s
}

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const ws = await inv('workspace_create', { path: join(root, 'ws'), name: 'Shop', description: null })
  const project = await inv('project_register', { workspaceId: ws.id, path: repo, name: null })
  const profile = await inv('profile_create', { input: { name: 'Fake', configDir: cfg, executable: fakeClaude } })

  writeFileSync(fixture, A)
  const a = await launch(project.id, profile.id)
  const env = JSON.parse(readFileSync(report, 'utf8')).env
  check('ambient telemetry never reaches Claude (NEXUS sets and forwards none)', env.CLAUDE_CODE_ENABLE_TELEMETRY === null && env.OTEL_EXPORTER_OTLP_ENDPOINT === null, js([env.CLAUDE_CODE_ENABLE_TELEMETRY, env.OTEL_EXPORTER_OTLP_ENDPOINT]))
  await inv('context_set', { projectId: project.id, strategy: 'deep', settings: null })
  writeFileSync(fixture, B)
  const b = await launch(project.id, profile.id)
  rmSync(fixture)
  const c = await launch(project.id, profile.id) // writes no transcript
  const transcripts = walk(join(cfg, 'projects')).filter((p) => p.endsWith('.jsonl'))
  check('the fake CLI wrote two transcripts in the profile (Claude Code layout)', transcripts.length === 2, transcripts.map((p) => p.slice(cfg.length)).join(', '))
  const before = transcripts.map((p) => readFileSync(p))

  await cdp.evaluate(`location.hash = '#/usage'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-usage-view]')`, 20000)
  const tokens = await until(async () => {
    const t = await text('[data-usage-tile="coverage"]')
    return t.includes('2 / 3') ? await text('[data-usage-tile="tokens"]') : null
  }, 30000)
  check('the scanner measured both sessions; the one without a transcript is unavailable, not zero', !!tokens && (await text('[data-usage-tile="coverage"]')).includes('1 unavailable'), await text('[data-usage-tile="coverage"]'))
  const r = await inv('usage_report', { filter: { workspaceId: ws.id }, by: 'strategy' })
  check('measured totals are deduplicated per response', r.report.totals.requests === 3 && r.report.totals.input + r.report.totals.output + r.report.totals.cacheWrite + r.report.totals.cacheRead === totalA + totalB, js(r.report.totals))
  check('before the profile’s billing is known no spend is shown', (await text('[data-usage-tile="cost"]')).includes('billing unknown'), (await text('[data-usage-tile="cost"]')).replace(/\n/g, ' '))
  const auth = await inv('profile_auth_status', { id: profile.id })
  await click('[data-usage-scan]')
  const costTile = await until(async () => {
    const t = await text('[data-usage-tile="cost"]')
    return t.includes('≈ $0.15') ? t : null
  })
  check('after an API-key sign-in check, Claude Code’s cost estimate is shown as an estimate', auth.authMethod === 'api_key' && !!costTile && costTile.includes('Estimate') && costTile.includes('not a bill'), (costTile ?? '').replace(/\n/g, ' '))
  const today = await cdp.evaluate(`(() => { const cols = [...document.querySelectorAll('[data-usage-trend] [data-bucket]')]; return Number(cols[cols.length - 1]?.dataset.total ?? -1) })()`)
  check('the daily trend shows today’s measured tokens', today === totalA + totalB, String(today))
  await click('[data-usage-by="strategy"]')
  const rows = await until(async () => {
    const keys = await cdp.evaluate(`[...document.querySelectorAll('[data-usage-breakdown] [data-usage-group]')].map((r) => r.dataset.usageGroup).join(',')`)
    return keys.includes('balanced') && keys.includes('deep') ? keys : null
  })
  check('breakdown by context strategy (recorded at launch)', !!rows, rows ?? '')
  const deep = r.report.groups.find((g) => g.key === 'deep')
  const balanced = r.report.groups.find((g) => g.key === 'balanced')
  check('efficiency compares strategies (context per request, measured)', deep?.efficiency.contextPerRequest === 9040 && balanced?.efficiency.contextPerRequest === (10 + 3000 + 5 + 3100) / 2, js([deep?.efficiency, balanced?.efficiency]))
  check('…with NEXUS’s own share as an estimate', (await text('[data-usage-strategy-row="deep"]')).includes('(est.)'))
  const statuses = await cdp.evaluate(`Object.fromEntries([...document.querySelectorAll('[data-usage-session]')].map((r) => [r.dataset.usageSession, r.dataset.usageStatus]))`)
  check('sessions list: measured vs unavailable with the reason', statuses[a.id] === 'measured' && statuses[b.id] === 'measured' && statuses[c.id] === 'missing' && (await text(`[data-usage-session="${c.id}"]`)).includes('no transcript'), js(statuses))

  // A filter scopes everything below it.
  await setValue('[data-usage-strategy]', 'deep')
  // Deep: session b (measured) and session c (launched after the switch, no transcript); a is balanced.
  const scoped = await until(async () => {
    const ids = await cdp.evaluate(`[...document.querySelectorAll('[data-usage-session]')].map((r) => r.dataset.usageSession)`)
    return ids.length === 2 && ids.includes(b.id) && ids.includes(c.id) ? await text('[data-usage-tile="requests"]') : null
  })
  const filtered = await inv('usage_report', { filter: { workspaceId: ws.id, strategy: 'deep' }, by: 'project' })
  check('filtering by strategy scopes tiles, trend and tables', !!scoped && scoped.includes('in 1 session') && filtered.report.totals.requests === 1, (scoped ?? '').replace(/\n/g, ' '))
  check('…and the filter is kept in the address', (await cdp.evaluate('location.hash')).includes('strategy=deep'))

  // The project page summarises and links to the dashboard with the project filter.
  await cdp.evaluate(`location.hash = '#/projects/${project.id}'; true`)
  const summary = await cdp.waitFor(`document.querySelector('[data-usage-summary]')?.innerText`, 15000).catch(() => '')
  check('the project page shows its usage', summary.includes('parsed') && summary.includes('3 requests') && summary.includes('2 of 3 sessions with usage'), summary.replace(/\n/g, ' '))
  await click('[data-usage-open]')
  const hash = await until(async () => {
    const h = await cdp.evaluate('location.hash')
    return h.startsWith('#/usage') && h.includes(`project=${project.id}`) ? h : null
  })
  check('…and links to the dashboard for that project', !!hash, hash ?? '')

  check('transcripts are only read', transcripts.every((p, i) => readFileSync(p).equals(before[i])))
  const db = ['nexus.db', 'nexus.db-wal'].map((f) => join(dataDir, f)).filter(existsSync).map((p) => readFileSync(p))
  check('no prompt or response text reaches the NEXUS database', db.every((b) => !b.includes(Buffer.from('SECRET'))))
  check('the page says how values are obtained and that telemetry is untouched', (await text('[data-usage-privacy]')).includes('never sets or forwards telemetry') && (await text('[data-usage-legend]')).includes('undocumented format'))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp(child).catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nUSAGE OK' : `\nUSAGE FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
