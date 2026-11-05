// End-to-end Portable Workspace against the REAL app (docs/PORTABLE_WORKSPACE.md). Grows with each milestone:
//   P1 — a schema 1 workspace shows the migration banner → the plan lists the changes → migrating writes schema 2 and
//        a backup; a workspace from a newer NEXUS opens read-only and refuses changes.
//   P2 — templates are listed → a workspace created from AI Engineering Pro has the library, approved, with the
//        workspace skills as an approved kit → the Constitutions view shows it → applying again changes nothing.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/portable.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('portable: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9341'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-portable-'))
const dataDir = join(root, 'data')
const oldWs = join(root, 'old-ws')
const futureWs = join(root, 'future-ws')
mkdirSync(join(oldWs, 'repos'), { recursive: true })
writeFileSync(join(oldWs, '.gitignore'), 'repos/\n')
writeFileSync(join(oldWs, 'nexus-workspace.json'), js({ schemaVersion: 1, id: '0192f2a4-0000-7000-8000-0000000e2e01', name: 'Old workspace', projects: [] }))
mkdirSync(futureWs, { recursive: true })
writeFileSync(join(futureWs, 'nexus-workspace.json'), js({ schemaVersion: 9, id: '0192f2a4-0000-7000-8000-0000000e2e02', name: 'Future workspace', projects: [], somethingNew: { a: 1 } }))
const fixtureRepo = join(root, 'app')
mkdirSync(fixtureRepo, { recursive: true })

let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
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

let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const invErr = (cmd, args = {}) =>
  cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)}).then(() => null, (e) => (typeof e === 'string' ? e : JSON.stringify(e)))`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.click(); return true })()`)
const text = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
async function reload() {
  await cdp.evaluate(`location.hash = '#/'; location.reload(); true`)
  await sleep(500)
  await cdp.waitFor(`document.querySelector('.shell')`, 20000)
}

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)

  // ---- P1: schema 1 → 2 through the banner ----
  const old = await inv('workspace_import', { path: oldWs })
  await reload()
  const banner = await cdp.waitFor(`document.querySelector('[data-workspace-migrate]')?.innerText`, 10000).catch(() => '')
  check('a schema 1 workspace shows the migration banner', banner.includes('schema 1'), banner)
  await click('[data-workspace-migrate-review]')
  const changes = await cdp.waitFor(`document.querySelector('[data-migration-changes]')?.innerText`, 10000).catch(() => '')
  check('the plan lists what changes before anything is written', changes.includes('nexus-workspace.json') && changes.includes('.gitignore'), changes)
  check('…and nothing changed yet', JSON.parse(readFileSync(join(oldWs, 'nexus-workspace.json'), 'utf8')).schemaVersion === 1)
  await click('[data-migration-apply]')
  await cdp.waitFor(`!document.querySelector('[data-workspace-migrate]')`, 10000).catch(() => undefined)
  const m = JSON.parse(readFileSync(join(oldWs, 'nexus-workspace.json'), 'utf8'))
  check('migrating writes schema 2 and keeps the clone folder', m.schemaVersion === 2 && m.layout?.projects === 'repos' && !!m.migratedWith, js(m))
  const backups = join(dataDir, 'workspace-backups', old.id)
  const copies = existsSync(backups) ? readdirSync(backups) : []
  check('…after copying the previous files to the NEXUS data folder', copies.length === 1 && existsSync(join(backups, copies[0], 'nexus-workspace.json')), copies.join(', '))
  check('…the .gitignore keeps the person’s lines and gains the new ones', /^repos\/\n/.test(readFileSync(join(oldWs, '.gitignore'), 'utf8')) && readFileSync(join(oldWs, '.gitignore'), 'utf8').includes('projects/'))
  check('the banner is gone after the migration', !(await cdp.evaluate(`!!document.querySelector('[data-workspace-migrate]')`)))

  // ---- P1: a workspace from a newer NEXUS is read-only ----
  const future = await inv('workspace_import', { path: futureWs })
  await reload()
  const ro = await cdp.waitFor(`document.querySelector('[data-workspace-readonly]')?.innerText`, 10000).catch(() => '')
  check('a workspace from a newer NEXUS opens read-only and says why', ro.includes('Read-only') && ro.includes('schema 9'), ro)
  const refused = await invErr('project_register', { workspaceId: future.id, path: fixtureRepo, name: null })
  check('…and refuses changes', !!refused && refused.includes('read-only'), refused ?? 'accepted')
  check('…without touching its manifest', JSON.parse(readFileSync(join(futureWs, 'nexus-workspace.json'), 'utf8')).somethingNew?.a === 1)
  check('the workspace list still opens it (not refused as before schema 2)', (await text('.shell')).length > 0)

  // ---- P2: templates ----
  const tpls = await inv('template_list')
  const pro = tpls.find((t) => t.id === 'ai-engineering-pro')
  check('three templates are offered, Pro with the full library', tpls.map((t) => t.id).join(',') === 'minimal,developer,ai-engineering-pro' && pro.stackRuleSets === 13 && pro.skills === 11 && pro.workflows === 7, js(tpls.map((t) => [t.id, t.constitutions, t.stackRuleSets, t.skills, t.workflows])))
  check('…and only a small core is always on', pro.alwaysOnChars > 0 && pro.alwaysOnChars < 1000, String(pro.alwaysOnChars))
  const proWs = await inv('workspace_create', { path: join(root, 'pro-ws'), name: 'Pro', description: null, template: 'ai-engineering-pro' })
  check('a workspace created from a template records it', proWs.template?.id === 'ai-engineering-pro', js(proWs.template))
  check('…with its lock file', existsSync(join(root, 'pro-ws', 'nexus.lock.json')))
  const constitutions = await inv('constitution_list', { workspaceId: proWs.id })
  check('…its constitutions are indexed', constitutions.length === 27, String(constitutions.length))
  const kits = await inv('kit_list')
  const wk = kits.find((k) => k.workspaceId === proWs.id)
  check('…its skills are the approved workspace kit', wk?.name === 'workspace' && wk.status === 'approved' && wk.inventory.skills.length === 11, js(wk && [wk.name, wk.status]))
  await reload()
  await cdp.evaluate(`location.hash = '#/constitutions'; true`)
  const view = await cdp.waitFor(`(() => { const t = document.querySelector('main')?.innerText ?? ''; return t.includes('Core working agreement') && t.includes('Security') ? t : '' })()`, 15000).catch(() => '')
  check('the Constitutions view shows the installed library', !!view)
  const again = await inv('template_plan', { workspaceId: proWs.id, template: 'ai-engineering-pro' })
  check('applying the same template again changes nothing', again.items.every((i) => i.action === 'unchanged'), js(again.items.filter((i) => i.action !== 'unchanged').slice(0, 3)))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp().catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nPORTABLE OK' : `\nPORTABLE FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
