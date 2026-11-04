// Desktop E2E for profile diagnostics (M14) and AI Kits (M21) against the REAL app (real IPC, real UI):
// kit review → approve → enable one skill for Work in the UI → Work diagnostics show it → launching Work passes the
// kit overlay (only that skill), launching Personal passes nothing. Synthetic profiles; fake-claude records args.
//
//   cargo build -p fake-claude -p nexus-cli
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/profiles-kits.mjs [path/to/nexus-desktop(.exe)]
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, unlinkSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('profiles-kits: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9333'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const w = (p, t) => {
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, t)
}

const root = mkdtempSync(join(tmpdir(), 'nexus-pk-'))
const dataDir = join(root, 'data')
const repo = join(root, 'repo')
const kit = join(root, 'team-kit')
const report = join(root, 'report.json')
mkdirSync(repo, { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'shop' }))
execFileSync('git', ['init', '-q'], { cwd: repo })
w(join(kit, '.claude-plugin', 'plugin.json'), js({ name: 'team-kit', version: '1.0.0', description: 'Team conventions' }))
for (const s of ['alpha', 'beta', 'gamma']) w(join(kit, 'skills', s, 'SKILL.md'), `---\nname: ${s}\ndescription: Team skill ${s}\n---\nBody\n`)
w(join(kit, 'hooks', 'hooks.json'), js({ hooks: { SessionStart: [{ hooks: [{ type: 'command', command: 'node start.js' }] }] } }))

let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: dataDir, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_REPORT: report },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))
async function launchArgs(profileId, projectId) {
  if (existsSync(report)) unlinkSync(report)
  await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_start_claude', { args: { profileId: ${js(profileId)}, projectId: ${js(projectId)}, cols: 100, rows: 30 } })`)
  const deadline = Date.now() + 20000
  for (;;) {
    try {
      return JSON.parse(readFileSync(report, 'utf8')).args
    } catch {
      if (Date.now() > deadline) throw new Error('fake-claude did not report')
      await new Promise((r) => setTimeout(r, 200))
    }
  }
}
const pluginDirs = (args) => args.flatMap((a, i) => (args[i - 1] === '--plugin-dir' ? [a] : []))

let cdp
try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const setup = await cdp.evaluate(`(async () => {
    const inv = window.__TAURI_INTERNALS__.invoke
    const ws = await inv('workspace_create', { path: ${js(join(root, 'ws'))}, name: 'Shop', description: null })
    const project = await inv('project_register', { workspaceId: ws.id, path: ${js(repo)}, name: null })
    const work = await inv('profile_create', { input: { name: 'Work', configDir: ${js(join(root, 'cfg-work'))}, executable: ${js(fakeClaude)} } })
    const personal = await inv('profile_create', { input: { name: 'Personal', configDir: ${js(join(root, 'cfg-personal'))}, executable: ${js(fakeClaude)} } })
    const kit = await inv('kit_register', { source: ${js(kit)}, git: false, gitRef: null, subdir: null, plugin: null })
    return { project, work, personal, kit }
  })()`)
  check('kit registered for review', setup.kit.status === 'needs-review' && setup.kit.inventory.skills.length === 3)

  // Review and approve in the UI.
  await cdp.evaluate(`location.hash = '#/kits'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('[data-kit-detail]')`, 20000)
  const detail = await cdp.evaluate(`document.querySelector('[data-kit-detail]').innerText`)
  check('the kit view shows its contents and that hooks are off by default', detail.includes('team-kit:alpha') && detail.includes('off unless enabled per profile'))
  await cdp.evaluate(`document.querySelector('[data-approve]').click(), true`)
  const approved = await cdp.waitFor(`!document.querySelector('[data-review]')`, 10000).catch(() => false)
  check('approving removes the review notice', !!approved)

  // Enable only "alpha" for Work in the UI.
  const card = `[data-profile="${setup.work.id}"]`
  await cdp.waitFor(`document.querySelector('${card} [data-skill="alpha"]')`, 10000)
  const preselected = await cdp.evaluate(`[...document.querySelectorAll('${card} [data-skill]')].filter((c) => c.checked).length`)
  await cdp.evaluate(`(() => { for (const c of document.querySelectorAll('${card} [data-skill]')) { if (c.checked !== (c.dataset.skill === 'alpha')) c.click() } return true })()`)
  await cdp.evaluate(`document.querySelector('${card} [data-save]').click(), true`)
  const saved = await cdp.waitFor(`document.querySelector('${card}')?.classList.contains('on')`, 10000).catch(() => false)
  check('Work is enabled with one skill', !!saved, `${preselected} preselected (small kit → all)`)

  // Diagnostics view for Work shows the kit skill.
  await cdp.evaluate(`location.hash = '#/profiles', true`)
  await cdp.waitFor(`document.querySelector('[data-diagnose="${setup.work.id}"]')`, 10000)
  await cdp.evaluate(`document.querySelector('[data-diagnose="${setup.work.id}"]').click(), true`)
  const skills = await cdp.waitFor(`document.querySelector('[data-section="skills"]')?.innerText`, 15000).catch(() => '')
  check('Work diagnostics list the kit skill under NEXUS kits', skills.includes('From NEXUS kits') && skills.includes('team-kit:alpha') && !skills.includes('team-kit:beta'))
  const version = await cdp.evaluate(`document.querySelector('[data-section="overview"]')?.innerText ?? ''`)
  check('…and the executable version', version.includes('9.9.9'))

  // Launches: Work gets the overlay with only alpha; Personal gets nothing.
  const workArgs = await launchArgs(setup.work.id, setup.project.id)
  const dirs = pluginDirs(workArgs)
  const overlaySkills = dirs[0] ? readdirSync(join(dirs[0], 'skills')) : []
  check('a Work session gets the kit overlay with only the selected skill', dirs.length === 1 && overlaySkills.join() === 'alpha' && !existsSync(join(dirs[0], 'hooks')), overlaySkills.join())
  const personalArgs = await launchArgs(setup.personal.id, setup.project.id)
  check('a Personal session gets no kit', pluginDirs(personalArgs).length === 0)
  check('Personal diagnostics show no kit', !(await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('profile_diagnostics', { profileId: ${js(setup.personal.id)} })`)).skills.some((s) => s.name.startsWith('team-kit')))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  spawn('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nPROFILES & KITS OK' : `\nPROFILES & KITS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
await new Promise((r) => setTimeout(r, 1500))
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* WebView2 may still hold files briefly */
}
