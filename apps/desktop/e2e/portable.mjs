// End-to-end Portable Workspace against the REAL app (docs/PORTABLE_WORKSPACE.md). Grows with each milestone:
//   P1 — a schema 1 workspace shows the migration banner → the plan lists the changes → migrating writes schema 2 and
//        a backup; a workspace from a newer NEXUS opens read-only and refuses changes.
//   P2 — templates are listed → a workspace created from AI Engineering Pro has the library, approved, with the
//        workspace skills as an approved kit → the Constitutions view shows it → applying again changes nothing.
//   P3 — Workspace Doctor: a missing README is reported with a repair → the repair shows its plan → applying it
//        writes the file and the finding disappears; healthy checks (approved skills, template) are listed.
//   P4 — registering a project (local bare remote) creates its portable context; its task lands there; the clone
//        goes missing → Projects offers to clone it from the remote → cloned under projects/ with the same id; a new
//        repository in projects/ is registered only after the confirmation.
//   P5 — environment requirements from the project's files (names of environment variables only, never `.env`) are
//        recorded in the workspace; checking this computer reports a mismatch; an install command is only typed into a
//        terminal; the Claude Code setup view reports the installation.
//   P6 — a Claude session (fake CLI) in a project edits a file (real `nexus hook`) and records a decision (real
//        `nexus mcp`); when it ends, its development record appears in the project's context and in the Development
//        History view with separate Verified / Observed / Reported sections.
//   P7 — the workspace becomes a repository with a local bare remote → trusted sync is turned on in the Sync Center only
//        after confirming the remote is private → the next session's record is committed and pushed on its own (the
//        person's task file stays local) and the automatic run is in the sync history.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/portable.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol). Synthetic fixtures in a temp dir; never touches real Claude profiles.
import { execFileSync, spawn, spawnSync } from 'node:child_process'
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
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const nexusCli = fileURLToPath(new URL('../../../target/debug/nexus.exe', import.meta.url))
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
  env: { ...process.env, NEXUS_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_STAY: '1', FAKE_CLAUDE_REPORT: join(root, 'fake-report.json') },
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

function cli(args, stdin) {
  return new Promise((resolve) => {
    const p = spawn(nexusCli, ['--data-dir', dataDir, ...args], { stdio: ['pipe', 'pipe', 'ignore'] })
    let out = ''
    p.stdout.on('data', (d) => (out += d))
    p.on('exit', () => resolve(out))
    p.stdin.end(stdin)
  })
}

function git(dir, ...args) {
  const r = spawnSync('git', ['-c', 'user.name=E2E', '-c', 'user.email=e2e@example.invalid', '-c', 'init.defaultBranch=main', '-c', 'core.autocrlf=false', ...args], { cwd: dir, encoding: 'utf8' })
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`)
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

  // ---- P3: Workspace Doctor ----
  rmSync(join(root, 'pro-ws', 'README.md'))
  await cdp.evaluate(`location.hash = '#/workspace'; true`)
  const readme = await cdp.waitFor(`document.querySelector('[data-finding="readme"]')?.innerText`, 30000).catch(() => '')
  check('the Doctor reports the missing README with a repair', readme.includes('README') && !!(await cdp.evaluate(`!!document.querySelector('[data-repair="readme"]')`)), readme)
  const okList = await cdp.evaluate(`[...document.querySelectorAll('[data-finding-ok]')].map((e) => e.getAttribute('data-finding-ok')).join(',')`)
  check('…and lists what is healthy (approved workspace skills, template)', okList.includes('workspace-skills') && okList.includes('template'), okList)
  check('…the template card shows the installed template', (await text('[data-template-row="ai-engineering-pro"]')).includes('Check for updates'))
  await click('[data-repair="readme"]')
  const plan = await cdp.waitFor(`document.querySelector('[data-repair-changes]')?.innerText`, 10000).catch(() => '')
  check('the repair shows its plan first', plan.includes('create README.md') && !existsSync(join(root, 'pro-ws', 'README.md')), plan)
  await click('[data-repair-apply]')
  await cdp.waitFor(`!document.querySelector('[data-finding="readme"]')`, 20000).catch(() => undefined)
  check('applying it writes the file and the finding disappears', existsSync(join(root, 'pro-ws', 'README.md')) && !(await cdp.evaluate(`!!document.querySelector('[data-finding="readme"]')`)))

  // ---- P4: project context, discovery, restoration ----
  const bare = join(root, 'remotes', 'invoices.git')
  mkdirSync(bare, { recursive: true })
  git(bare, 'init', '-q', '--bare')
  const work = join(root, 'invoices')
  mkdirSync(work, { recursive: true })
  writeFileSync(join(work, 'README.md'), '# Invoices\n\nSends invoices and reminders.\n')
  writeFileSync(join(work, 'package.json'), js({ dependencies: { vue: '^3' }, scripts: { test: 'vitest' } }))
  git(work, 'init', '-q')
  git(work, 'add', '-A')
  git(work, 'commit', '-q', '-m', 'init')
  git(work, 'remote', 'add', 'origin', `file:///${bare.replace(/\\/g, '/')}`)
  git(work, 'push', '-q', 'origin', 'main')
  const inv1 = await inv('project_register', { workspaceId: proWs.id, path: work, name: null })
  const ctxDir = join(root, 'pro-ws', 'project-context', 'invoices')
  check('registering a project creates its portable context', ['project.json', 'overview.md', 'architecture.md', 'current-state.md'].every((f) => existsSync(join(ctxDir, f))))
  check('…the overview quotes the README with its source', readFileSync(join(ctxDir, 'overview.md'), 'utf8').includes('Sends invoices and reminders.'))
  const task = await inv('knowledge_create', { workspaceId: proWs.id, input: { kind: 'task', title: 'Send reminders', projectId: inv1.id, body: '' } })
  check('…and the project\u2019s task is stored there', task.path.includes(join('project-context', 'invoices', 'tasks')), task.path)
  // The clone goes missing (another computer); a stray repository sits in projects/.
  execFileSync('cmd', ['/c', 'move', work, join(root, 'invoices-elsewhere')], { stdio: 'ignore' })
  const scratch = join(root, 'pro-ws', 'projects', 'scratch')
  mkdirSync(scratch, { recursive: true })
  git(scratch, 'init', '-q')
  await cdp.evaluate(`location.hash = '#/projects'; location.reload(); true`)
  const missingRow = await cdp.waitFor(`document.querySelector('[data-missing="invoices"]')?.innerText`, 20000).catch(() => '')
  check('Projects offers to clone the missing project from its remote', missingRow.includes('projects/invoices'), missingRow)
  await click('[data-discovery-clone]')
  const restored = await cdp.waitFor(`(async () => { const p = await window.__TAURI_INTERNALS__.invoke('project_get', { id: ${js(inv1.id)} }); return p.available ? p.localPath : '' })()`, 60000).catch(() => '')
  check('…cloning restores it under projects/ with the same id', restored.endsWith(join('projects', 'invoices')) && existsSync(join(root, 'pro-ws', 'projects', 'invoices', 'README.md')), restored)
  const newRow = await cdp.waitFor(`(() => { const b = document.querySelector('[data-register="projects/scratch"]'); return b && !b.disabled ? 'yes' : '' })()`, 20000).catch(() => '')
  check('a new repository in projects/ is offered for review', newRow === 'yes')
  const before = (await inv('project_list', { workspaceId: proWs.id })).length
  await click('[data-register="projects/scratch"]')
  const msg = await cdp.waitFor(`document.querySelector('[data-confirm-message]')?.innerText`, 10000).catch(() => '')
  check('…registering asks first and says nothing is run', msg.includes('nothing in the repository is run'), msg)
  check('…nothing is registered before the confirmation', (await inv('project_list', { workspaceId: proWs.id })).length === before)
  await click('[data-confirm-ok]')
  const after = await cdp.waitFor(`(async () => (await window.__TAURI_INTERNALS__.invoke('project_list', { workspaceId: ${js(proWs.id)} })).length)()`, 20000).then(async () => (await inv('project_list', { workspaceId: proWs.id })).length)
  check('…and registered after it', after === before + 1, `${before} → ${after}`)

  // ---- P5: environment and Claude Code setup ----
  const envApp = join(root, 'envapp')
  mkdirSync(envApp, { recursive: true })
  writeFileSync(join(envApp, 'package.json'), js({ name: 'envapp', engines: { node: '>=999' }, packageManager: 'bun@1.1.0', dependencies: { vue: '^3' } }))
  writeFileSync(join(envApp, '.env.example'), 'API_TOKEN=replace-me\n')
  writeFileSync(join(envApp, '.env'), 'LOCAL_ONLY_SECRET=never-read\n')
  writeFileSync(join(envApp, 'compose.yaml'), 'services:\n  db:\n    image: postgres:16\n')
  const envProject = await inv('project_register', { workspaceId: proWs.id, path: envApp, name: null })
  const envFile = join(root, 'pro-ws', 'project-context', 'envapp', 'environment.json')
  const recorded = existsSync(envFile) ? readFileSync(envFile, 'utf8') : ''
  check('the environment is recorded in the workspace from the project\u2019s files', recorded.includes('">=999"') && recorded.includes('package.json engines.node') && recorded.includes('"db"'), recorded.slice(0, 120))
  check('…environment variables by name only, never values and never .env', recorded.includes('API_TOKEN') && !recorded.includes('replace-me') && !recorded.includes('LOCAL_ONLY_SECRET'))
  await cdp.evaluate(`location.hash = '#/projects/${envProject.id}'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-env-check]')`, 20000)
  await click('[data-env-check]')
  const nodeRow = await cdp.waitFor(`document.querySelector('[data-env-row="node"]')?.innerText`, 60000).catch(() => '')
  check('checking this computer reports the unmet Node.js requirement', /mismatch|missing/.test(nodeRow) && nodeRow.includes('>=999'), nodeRow)
  const bunRow = await text('[data-env-row="bun"]')
  if (bunRow.includes('missing') && (await cdp.evaluate(`!!document.querySelector('[data-env-install="bun"]')`))) {
    await click('[data-env-install="bun"]')
    const typedText = await cdp.waitFor(`(() => { const t = [...document.querySelectorAll('.xterm-rows')].map((e) => e.innerText).join('\\n'); return t.includes('Oven-sh.Bun') ? t : '' })()`, 20000).catch(() => '')
    check('an install command is typed into a terminal, not run', typedText.includes('winget install --id Oven-sh.Bun') && !/Found Bun|Successfully installed/i.test(typedText), typedText.slice(-160))
  } else {
    console.log(`      (bun is installed here or has no package-manager command — typing check skipped: ${bunRow.replace(/\s+/g, ' ')})`)
  }
  await cdp.evaluate(`location.hash = '#/setup'; true`)
  const setup = await cdp.waitFor(`document.querySelector('[data-setup-status]')?.innerText`, 30000).catch(() => '')
  check('the Claude Code setup view reports the installation and the next steps', setup.includes('Claude Code'), setup.slice(0, 120))

  // ---- P6: development record and history ----
  const cfg = join(root, 'cfg')
  mkdirSync(cfg, { recursive: true })
  const profile = await inv('profile_create', { input: { name: 'Fake', configDir: cfg, executable: fakeClaude } })
  const repoDir = join(root, 'pro-ws', 'projects', 'invoices')
  const s = await inv('session_start_claude', { args: { profileId: profile.id, projectId: inv1.id, title: 'Reminder emails', cols: 100, rows: 30 } })
  for (let i = 0; i < 60 && !existsSync(join(root, 'fake-report.json')); i++) await sleep(250)
  await cli(['hook', '--session', s.id, 'post-tool-use'], js({ session_id: s.claudeSessionId, tool_name: 'Edit', tool_input: { file_path: join(repoDir, 'src', 'reminders.ts') } }))
  await cli(['mcp', '--session', s.id], [
    js({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } }),
    js({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'checkpoint', arguments: { kind: 'decision', summary: 'Send reminders from a nightly job' } } }),
  ].join('\n') + '\n')
  await inv('session_terminate', { id: s.id })
  const devDir = join(root, 'pro-ws', 'project-context', 'invoices', 'development')
  let recordFile = ''
  for (let i = 0; i < 80 && !recordFile; i++) {
    recordFile = existsSync(devDir) ? (readdirSync(devDir).find((f) => f.endsWith('.md')) ?? '') : ''
    if (!recordFile) await sleep(250)
  }
  const recordText = recordFile ? readFileSync(join(devDir, recordFile), 'utf8') : ''
  check('the ended session leaves a development record in the project context', !!recordFile && recordText.includes('topic: development-record'), recordFile)
  check('…with the observed edit and Claude\u2019s decision kept apart as unverified', recordText.includes('`src/reminders.ts`') && recordText.includes('## Reported by Claude (unverified)') && recordText.includes('Send reminders from a nightly job'), recordText.slice(0, 200))
  await cdp.evaluate(`location.hash = '#/projects/${inv1.id}/history'; location.reload(); true`)
  const rec = await cdp.waitFor(`document.querySelector('[data-history-records] [data-record]') ? document.querySelector('[data-history-records]').innerText : ''`, 20000).catch(() => '')
  check('the Development History view lists the record and its sync state', rec.includes('Reminder emails') || rec.includes('invoices —'), rec.slice(0, 160))
  check('…not yet in Git (the workspace is not a repository here)', !!(await cdp.evaluate(`!!document.querySelector('[data-record-sync="no-repository"]')`)))
  await click('[data-history-records] .record-head')
  const panels = await cdp.waitFor(`[...document.querySelectorAll('[data-record-body] .section h3')].map((e) => e.textContent).join('|')`, 10000).catch(() => '')
  check('…and opens it as Verified / Observed / Reported sections', panels.includes('Verified') && panels.includes('Observed') && panels.includes('Reported by Claude (unverified)'), panels)

  // ---- P7: sync policies ----
  const wsRoot = join(root, 'pro-ws')
  const wsBare = join(root, 'remotes', 'workspace.git')
  mkdirSync(wsBare, { recursive: true })
  git(wsBare, 'init', '-q', '--bare')
  git(wsRoot, 'init', '-q')
  git(wsRoot, 'config', 'user.name', 'E2E')
  git(wsRoot, 'config', 'user.email', 'e2e@example.invalid')
  git(wsRoot, 'add', '-A')
  git(wsRoot, 'commit', '-q', '-m', 'workspace')
  git(wsRoot, 'remote', 'add', 'origin', `file:///${wsBare.replace(/\\/g, '/')}`)
  git(wsRoot, 'push', '-q', '-u', 'origin', 'main')
  await cdp.evaluate(`location.hash = '#/sync'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-sync-policy="" ], [data-sync-policy]')`, 20000)
  check('the Sync Center starts on manual sync', (await cdp.evaluate(`document.querySelector('[data-sync-policy]').getAttribute('data-sync-policy-mode')`)) === 'manual')
  await cdp.evaluate(`(() => { const r = document.querySelector('[data-sync-policy-choose="trusted"]'); r.checked = true; r.dispatchEvent(new Event('change')); return true })()`)
  const allow = await cdp.waitFor(`document.querySelector('[data-trusted-allowlist]')?.innerText`, 10000).catch(() => '')
  check('choosing trusted shows exactly what may be published', allow.includes('project-context/*/development/*.md') && !allow.includes('tasks'), allow)
  check('…and cannot be confirmed without the private-remote confirmation', !!(await cdp.evaluate(`document.querySelector('[data-trusted-confirm]').disabled`)))
  await click('[data-trusted-private]')
  await click('[data-trusted-confirm]')
  const mode = await cdp.waitFor(`document.querySelector('[data-sync-policy]')?.getAttribute('data-sync-policy-mode') === 'trusted' ? 'trusted' : ''`, 10000).catch(() => '')
  check('trusted sync is on for this computer', mode === 'trusted')
  // A personal task file next to the records must not travel.
  writeFileSync(join(wsRoot, 'project-context', 'invoices', 'tasks', 'mine-local.md'), 'not a record\n')
  const s2 = await inv('session_start_claude', { args: { profileId: profile.id, projectId: inv1.id, title: 'Reminder retries', cols: 100, rows: 30 } })
  await sleep(1500)
  await cli(['hook', '--session', s2.id, 'post-tool-use'], js({ session_id: s2.claudeSessionId, tool_name: 'Edit', tool_input: { file_path: join(repoDir, 'src', 'retry.ts') } }))
  await inv('session_terminate', { id: s2.id })
  const onRemote = (rel) => spawnSync('git', ['--git-dir', wsBare, 'cat-file', '-e', `HEAD:${rel}`]).status === 0
  let published = ''
  for (let i = 0; i < 120 && !published; i++) {
    const files = readdirSync(devDir).filter((f) => f.endsWith('.md'))
    published = files.find((f) => f !== recordFile && onRemote(`project-context/invoices/development/${f}`)) ?? ''
    if (!published) await sleep(250)
  }
  check('the next record is committed and pushed on its own', !!published, published)
  check('…the person\u2019s task file stays local', !onRemote('project-context/invoices/tasks/mine-local.md'))
  const hist = await inv('sync_status', { workspaceId: proWs.id })
  check('…and the automatic run is in the sync history', hist.history.some((h) => h.op === 'auto-sync' && h.status === 'ok'), js(hist.history.slice(0, 3).map((h) => [h.op, h.status, h.message])))
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
