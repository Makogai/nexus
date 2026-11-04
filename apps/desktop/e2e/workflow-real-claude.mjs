// Workflow skills against the REAL Claude Code CLI (ADR-0029; requires `claude`; skipped otherwise).
// Synthetic Work and Personal profiles and a synthetic kit with three skills; the kit is enabled for Work with one
// skill (alpha). A workflow names `team-kit:beta`. The real init event must show: a normal Work session → alpha only;
// a Work session for the workflow run → alpha and beta; a Personal session for a run of the same workflow → nothing
// from the kit (profile isolation, the skill is refused with a reason). No real profile is touched.
//
//   cargo build -p nexus-cli -p fake-claude
//   node e2e/workflow-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const exe = process.platform === 'win32' ? '.exe' : ''
const nexus = fileURLToPath(new URL(`../../../target/debug/nexus${exe}`, import.meta.url))
const fakeClaude = fileURLToPath(new URL(`../../../target/debug/fake-claude${exe}`, import.meta.url))
const where = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['claude'], { encoding: 'utf8' })
const claude = (where.stdout || '').split(/\r?\n/).find((l) => (process.platform === 'win32' ? l.toLowerCase().endsWith('.exe') : l.trim()))
if (!claude) {
  console.log('SKIP  claude is not installed')
  process.exit(0)
}
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const w = (p, t) => {
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, t)
}
const root = mkdtempSync(join(tmpdir(), 'nexus-wf-real-'))
const home = join(root, 'home')
const data = join(root, 'nexus-data')
const proj = join(root, 'proj')
const cfgWork = join(root, 'cfg-work')
const cfgPersonal = join(root, 'cfg-personal')
for (const d of [home, proj, cfgWork, cfgPersonal]) mkdirSync(d, { recursive: true })
execFileSync('git', ['init', '-q'], { cwd: proj })
const baseEnv = { ...process.env, USERPROFILE: home, HOME: home }
for (const k of Object.keys(baseEnv)) if (/^(ANTHROPIC_|CLAUDE_CODE_|CLAUDECODE|OTEL_)/.test(k)) delete baseEnv[k]
const cli = (args, env = {}) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8', env: { ...baseEnv, ...env } })
function init(cfg, extra) {
  const r = spawnSync(claude, ['-p', 'hi', '--verbose', '--output-format', 'stream-json', ...extra], { cwd: proj, env: { ...baseEnv, CLAUDE_CONFIG_DIR: cfg }, encoding: 'utf8', timeout: 120000 })
  return (r.stdout || '').split(/\r?\n/).filter((l) => l.startsWith('{')).map((l) => {
    try {
      return JSON.parse(l)
    } catch {
      return null
    }
  }).find((e) => e?.type === 'system' && e.subtype === 'init')
}
const kitSkills = (e) => (e?.skills ?? []).filter((s) => s.startsWith('team-kit:')).sort()

cli(['workspaces', 'create', join(root, 'ws'), '--name', 'Wf'])
cli(['projects', 'add', proj])
cli(['profiles', 'add', '--name', 'Work', '--config-dir', cfgWork, '--executable', fakeClaude])
cli(['profiles', 'add', '--name', 'Personal', '--config-dir', cfgPersonal, '--executable', fakeClaude])
const kit = join(root, 'team-kit')
w(join(kit, '.claude-plugin', 'plugin.json'), JSON.stringify({ name: 'team-kit', version: '1.0.0' }))
for (const s of ['alpha', 'beta', 'gamma']) w(join(kit, 'skills', s, 'SKILL.md'), `---\nname: ${s}\ndescription: Team skill ${s}\n---\n`)
cli(['kits', 'add', kit])
cli(['kits', 'approve', 'team-kit'])
cli(['kits', 'enable', 'team-kit', '-p', 'Work', '--skills', 'alpha'])
const wf = JSON.parse(cli(['--json', 'workflow', 'new', '--template', 'testing', '--skill', 'team-kit:beta']))

/** NEXUS launch through fake-claude → the exact --plugin-dir arguments, plus the session row. */
function launch(profile, task) {
  const report = join(root, `report-${Math.random().toString(36).slice(2)}.json`)
  const args = ['launch', '--profile', profile, '--project', 'proj', '-q']
  if (task) args.push('--task', task)
  cli(args, { FAKE_CLAUDE_REPORT: report })
  const a = JSON.parse(readFileSync(report, 'utf8')).args
  const session = JSON.parse(cli(['--json', 'sessions']))[0]
  return { dirs: a.flatMap((x, i) => (a[i - 1] === '--plugin-dir' ? ['--plugin-dir', x] : [])), session }
}
const plain = launch('Work')
const run = JSON.parse(cli(['--json', 'workflow', 'start', wf.id, '--project', 'proj', '--goal', 'More tests']))
const workRun = launch('Work', run.taskId)
const run2 = JSON.parse(cli(['--json', 'workflow', 'start', wf.id, '--project', 'proj', '--goal', 'Personal attempt']))
const personalRun = launch('Personal', run2.taskId)

const plainInit = init(cfgWork, plain.dirs)
const workInit = init(cfgWork, workRun.dirs)
const personalInit = init(cfgPersonal, personalRun.dirs)
check('real Claude reports its skills in the init event', !!plainInit && !!workInit && !!personalInit)
check('a normal Work session gets only the profile’s selection', JSON.stringify(kitSkills(plainInit)) === '["team-kit:alpha"]', kitSkills(plainInit).join(','))
check('the Work session of the workflow run also gets the workflow’s skill', JSON.stringify(kitSkills(workInit)) === '["team-kit:alpha","team-kit:beta"]', kitSkills(workInit).join(','))
check('…from its own overlay folder (the profile’s overlay is untouched)', workRun.dirs[1] !== plain.dirs[1], `${plain.dirs[1]} vs ${workRun.dirs[1]}`)
check('a Personal session for the same workflow gets nothing from the kit', kitSkills(personalInit).length === 0 && personalRun.dirs.length === 0, kitSkills(personalInit).join(','))
const refused = personalRun.session.integrations?.workflow?.refused ?? []
check('…and the refusal is recorded with the reason', refused.some((r) => r.includes('not enabled for this profile')), refused.join(' | '))
check('gamma (selected nowhere) reaches no session', ![...kitSkills(plainInit), ...kitSkills(workInit)].includes('team-kit:gamma'))

console.log(failures === 0 ? '\nREAL-CLAUDE WORKFLOW OK' : `\nREAL-CLAUDE WORKFLOW FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
