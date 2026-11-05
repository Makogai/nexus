// End-to-end: setting up a NEW workspace where the person already has a place for it — against the REAL app, with
// temporary folders and local bare repositories only (never real repositories or remotes).
//   onboarding → Open from Git with a project repository: refused, no setup offered →
//   Open from Git with an EMPTY repository: explained, "set up a new workspace in it" → create form prefilled →
//   AI Engineering Pro → the workspace opens in the Sync Center → reviewed commit + first push go to that repository →
//   Settings → New workspace → existing folder with project files: refused; a host-scaffolded clone (README,
//   .gitignore): accepted, files kept, ignore lines added, standard folders created.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/workspace-setup.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol).
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('workspace-setup: the CDP-based E2E currently requires Windows (WebView2).')
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

const root = mkdtempSync(join(tmpdir(), 'nexus-wssetup-'))
const dataDir = join(root, 'data')
const git = (cwd, ...args) => execFileSync('git', ['-c', 'init.defaultBranch=main', '-c', 'core.autocrlf=false', ...args], { cwd, encoding: 'utf8' })
const fileUrl = (p) => `file://${p.replace(/\\/g, '/')}`
const ident = (dir) => {
  git(dir, 'config', 'user.name', 'Owner')
  git(dir, 'config', 'user.email', 'owner@example.invalid')
}
/** A bare repository, optionally with one commit holding `files`. */
function bareRepo(name, files = {}) {
  const bare = join(root, `${name}.git`)
  mkdirSync(bare)
  git(bare, 'init', '-q', '--bare')
  if (Object.keys(files).length) {
    const work = join(root, `${name}-src`)
    mkdirSync(work)
    git(work, 'init', '-q')
    ident(work)
    for (const [p, t] of Object.entries(files)) {
      mkdirSync(join(work, p, '..'), { recursive: true })
      writeFileSync(join(work, p), t)
    }
    git(work, 'add', '-A')
    git(work, 'commit', '-qm', 'initial')
    git(work, 'push', '-q', fileUrl(bare), 'HEAD:main')
  }
  return bare
}

let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: dataDir, NEXUS_E2E_NOTIFIER: 'record', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
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
const setValue = (sel, value) =>
  cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event('input', { bubbles: true })); return true })()`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el || el.disabled) return false; el.click(); return true })()`)
const value = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.value ?? null`)
const textOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
const state = () => cdp.evaluate(`document.querySelector('[data-sync-view]')?.dataset.syncState ?? ''`)
async function until(fn, ms = 30000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(250)
  }
}
async function confirmDialog() {
  const m = await cdp.waitFor(`document.querySelector('[data-confirm-message]')?.innerText`, 8000).catch(() => '')
  await click('[data-confirm-ok]')
  return m
}

try {
  const project = bareRepo('project', { 'README.md': '# app\n', 'package.json': '{}\n', 'src/main.ts': 'export {}\n' })
  const empty = bareRepo('personal')
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding')`, 30000)

  // A project repository is not a workspace and is never offered as a new one.
  await click('[data-onboarding-clone]')
  await cdp.waitFor(`document.querySelector('[data-sync-clone-url]')`, 5000)
  await setValue('[data-sync-clone-url]', fileUrl(project))
  await setValue('[data-sync-clone-parent]', root)
  await setValue('[data-sync-clone-name]', 'from-project')
  await click('[data-sync-clone-start]')
  const projectError = await cdp.waitFor(`document.querySelector('[data-sync-clone-error]')?.innerText`, 30000).catch(() => '')
  check('a project repository is refused as a workspace', projectError.includes('not a NEXUS workspace'), projectError.slice(0, 120))
  check('…without offering to turn it into one, and the clone is removed', !(await cdp.evaluate(`!!document.querySelector('[data-sync-clone-empty]')`)) && !existsSync(join(root, 'from-project')))

  // The empty repository the person prepared: explained, and offered as the place for a new workspace.
  await setValue('[data-sync-clone-url]', fileUrl(empty))
  await setValue('[data-sync-clone-name]', 'opened')
  await click('[data-sync-clone-start]')
  const offer = await cdp.waitFor(`document.querySelector('[data-sync-clone-empty]')?.innerText`, 30000).catch(() => '')
  check('opening an empty repository explains it and offers a new workspace in it', offer.includes('new') && (await textOf('[data-sync-clone-error]')).includes('empty'), offer.slice(0, 120))
  check('…and leaves no folder behind', !existsSync(join(root, 'opened')))
  await click('[data-sync-clone-setup]')
  await cdp.waitFor(`document.querySelector('[data-new-ws-url]')`, 5000)
  check('the create form starts from that repository', (await value('[data-new-ws-url]')) === fileUrl(empty) && (await value('[data-new-ws-name]')) === 'Personal', `${await value('[data-new-ws-url]')} / ${await value('[data-new-ws-name]')}`)
  await setValue('[data-new-ws-parent]', root)
  await click('[data-new-ws] [data-template="ai-engineering-pro"]')
  const target = await textOf('[data-new-ws-target]')
  check('the target folder is shown before anything happens', target.includes(join(root, 'personal')), target)
  await click('[data-new-ws-create]')

  // Set up, opened in the Sync Center, and the first reviewed commit + push go to the prepared repository.
  const wsDir = join(root, 'personal')
  const opened = await until(async () => {
    const s = await state()
    return s && s !== 'loading' ? s : null
  }, 60000)
  check('the new workspace opens in the Sync Center, not pushed yet', opened === 'no-upstream', opened ?? (await textOf('[data-new-ws-error]')))
  const ws = await inv('workspace_active')
  check('…with its manifest, template and standard folders', ws?.template?.id === 'ai-engineering-pro' && existsSync(join(wsDir, 'nexus-workspace.json')) && ['constitutions', 'knowledge', 'project-context', 'workflows'].every((d) => existsSync(join(wsDir, d, 'README.md'))), ws?.rootPath)
  check('…and the prepared repository as its remote', git(wsDir, 'remote', 'get-url', 'origin').trim() === fileUrl(empty))
  ident(wsDir)
  await click('[data-sync-refresh]')
  await cdp.waitFor(`document.querySelector('[data-sync-file="nexus-workspace.json"]')`, 15000)
  await click('[data-sync-review]')
  await cdp.waitFor(`document.querySelector('[data-sync-commit-dialog]') && !document.querySelector('[data-sync-finding$=":block"]')`, 15000)
  await setValue('[data-sync-message]', 'Set up the workspace')
  await click('[data-sync-commit-confirm]')
  await confirmDialog()
  await until(async () => ((await cdp.evaluate(`document.querySelectorAll('[data-sync-outgoing-commit]').length`)) === 1 ? true : null))
  await click('[data-sync-push]')
  const pushDialog = await cdp.waitFor(`document.querySelector('[data-sync-push-dialog]')?.innerText`, 10000).catch(() => '')
  await click('[data-sync-push-confirm]')
  await confirmDialog()
  const upToDate = await until(async () => ((await state()) === 'up-to-date' ? true : null))
  const published = git(empty, 'ls-tree', '-r', '--name-only', 'HEAD')
  check('the reviewed first push publishes the workspace to the prepared repository', !!upToDate && pushDialog.includes('whole history travels') && published.includes('nexus-workspace.json') && published.includes('knowledge/README.md'), published.split('\n').length + ' files')
  check('…and nothing machine-local or ignored travels', !/\.db|\.nexus-local|^projects\//m.test(published))

  // Projects → Add project → Clone from Git: cloned into projects/<name>, registered, its page opens.
  await cdp.evaluate(`location.hash = '#/projects'; true`)
  await cdp.waitFor(`document.querySelector('[data-add-project-open]')`, 15000)
  await click('[data-add-project-open]')
  await cdp.waitFor(`document.querySelector('[data-add-project-mode="git"]')`, 5000)
  await click('[data-add-project-mode="git"]')
  await setValue('[data-add-project-url]', fileUrl(project))
  const cloneTarget = await textOf('[data-add-project-target]')
  check('the clone target is shown before cloning (the workspace projects/ folder)', cloneTarget === join(wsDir, 'projects', 'project'), cloneTarget)
  await click('[data-add-project-clone]')
  const onPage = await until(async () => ((await cdp.evaluate(`location.hash`)).startsWith('#/projects/') ? true : null), 60000)
  const projectsNow = await inv('project_list', { workspaceId: ws.id }).catch(() => [])
  check(
    'a project cloned from Git is registered with its context and its page opens',
    !!onPage && existsSync(join(wsDir, 'projects', 'project', 'package.json')) && existsSync(join(wsDir, 'project-context', 'project', 'overview.md')) && projectsNow.some((p) => p.name === 'project'),
    projectsNow.map((p) => p.name).join(','),
  )
  check('…and the clone itself stays out of the workspace repository', !git(wsDir, 'status', '--porcelain').includes('projects/'))

  // Settings → New workspace in an existing folder: project files are refused, a host-scaffolded clone is accepted.
  const code = join(root, 'code')
  mkdirSync(code)
  writeFileSync(join(code, 'package.json'), '{}\n')
  const scaffold = join(root, 'scaffolded')
  mkdirSync(scaffold)
  writeFileSync(join(scaffold, 'README.md'), '# Notes\n')
  writeFileSync(join(scaffold, '.gitignore'), 'node_modules/\n')
  await cdp.evaluate(`location.hash = '#/settings'; true`)
  await cdp.waitFor(`document.querySelector('[data-settings-create-ws]')`, 15000)
  await click('[data-settings-create-ws]')
  await cdp.waitFor(`document.querySelector('[data-settings-new-ws] [data-new-ws-source="existing"]')`, 5000)
  await click('[data-settings-new-ws] [data-new-ws-source="existing"]')
  await setValue('[data-settings-new-ws] [data-new-ws-folder]', code)
  await click('[data-settings-new-ws] [data-template="none"]')
  await click('[data-settings-new-ws] [data-new-ws-create]')
  const refused = await cdp.waitFor(`document.querySelector('[data-settings-new-ws] [data-new-ws-error]')?.innerText`, 15000).catch(() => '')
  check('a folder with project files never becomes a workspace', refused.includes('package.json') && !existsSync(join(code, 'nexus-workspace.json')), refused.slice(0, 140))
  await setValue('[data-settings-new-ws] [data-new-ws-folder]', scaffold)
  await setValue('[data-settings-new-ws] [data-new-ws-name]', 'Notes')
  await click('[data-settings-new-ws] [data-new-ws-create]')
  const created = await until(async () => ((await inv('workspace_active'))?.name === 'Notes' ? true : null))
  const gi = existsSync(join(scaffold, '.gitignore')) ? readFileSync(join(scaffold, '.gitignore'), 'utf8') : ''
  check(
    'a host-scaffolded folder becomes the workspace: README kept, ignore lines added, standard folders created',
    !!created && readFileSync(join(scaffold, 'README.md'), 'utf8') === '# Notes\n' && gi.startsWith('node_modules/\n') && gi.includes('\nprojects/\n') && existsSync(join(scaffold, 'knowledge', 'README.md')),
    gi.replace(/\n/g, ' | ').slice(0, 160),
  )
  check('the form closes after creating', !!(await until(async () => ((await cdp.evaluate(`!document.querySelector('[data-settings-new-ws]')`)) ? true : null), 10000)))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  await stopApp().catch(() => undefined)
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nWORKSPACE SETUP OK' : `\nWORKSPACE SETUP FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
