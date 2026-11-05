// M14 + M21 against the REAL Claude CLI with synthetic profiles (signed out is enough: the init event lists skills,
// agents, plugins and MCP servers before authentication).
//
//  A. Diagnostics: a profile populated through Claude's own commands (`claude mcp add`, `claude plugin install`) plus
//     user skills/agents — NEXUS's `profiles diagnose` must report exactly the non-built-in skills, agents, plugins and
//     MCP servers the real Claude reports, and none of the planted secret values.
//  B. Kits: a kit enabled for Work with one selected skill reaches only Work sessions, only with that skill, without
//     its MCP server until that is opted in; Personal never sees it; NEXUS never touches either config dir.
//
//   cargo build -p nexus-cli -p fake-claude && node e2e/kits-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, relative } from 'node:path'
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
const root = mkdtempSync(join(tmpdir(), 'nexus-kits-'))
const home = join(root, 'home')
const data = join(root, 'nexus-data')
const proj = join(root, 'proj')
const cfgWork = join(root, 'cfg-work')
const cfgPersonal = join(root, 'cfg-personal')
const cfgEmpty = join(root, 'cfg-empty')
for (const d of [home, proj, cfgWork, cfgPersonal, cfgEmpty]) mkdirSync(d, { recursive: true })
execFileSync('git', ['init', '-q'], { cwd: proj })

const baseEnv = { ...process.env, USERPROFILE: home, HOME: home }
for (const k of Object.keys(baseEnv)) if (/^(ANTHROPIC_|CLAUDE_CODE_|CLAUDECODE|OTEL_)/.test(k)) delete baseEnv[k]
const cli = (args, env = {}) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8', env: { ...baseEnv, ...env } })
const claudeIn = (cfg, args, cwd = proj) => spawnSync(claude, args, { cwd, env: { ...baseEnv, CLAUDE_CONFIG_DIR: cfg }, encoding: 'utf8', timeout: 120000 })
function init(cfg, extra = []) {
  const r = claudeIn(cfg, ['-p', 'hi', '--verbose', '--output-format', 'stream-json', ...extra])
  return (r.stdout || '').split(/\r?\n/).filter((l) => l.startsWith('{')).map((l) => {
    try {
      return JSON.parse(l)
    } catch {
      return null
    }
  }).find((e) => e?.type === 'system' && e.subtype === 'init')
}
const snapshot = (dir) => {
  const out = []
  const walk = (d) => {
    for (const e of readdirSync(d)) {
      const p = join(d, e)
      if (statSync(p).isDirectory()) walk(p)
      else out.push(`${relative(dir, p)}:${statSync(p).size}`)
    }
  }
  walk(dir)
  return out.sort().join('|')
}
const same = (a, b) => a.length === b.length && [...a].sort().every((x, i) => x === [...b].sort()[i])

// Claude's built-ins (to compare only what the profile adds).
const builtin = init(cfgEmpty)
check('real Claude reports its configuration in the init event', !!builtin, builtin ? builtin.claude_code_version : 'no init event')
const builtinSkills = new Set(builtin?.skills ?? [])
const builtinAgents = new Set(builtin?.agents ?? [])

// ---- A. diagnostics -------------------------------------------------------------------------------------------
const market = join(root, 'market')
const installed = join(market, 'installed-kit')
w(join(installed, '.claude-plugin', 'plugin.json'), JSON.stringify({ name: 'installed-kit', version: '2.0.0' }))
w(join(installed, 'skills', 'lint', 'SKILL.md'), '---\nname: lint\ndescription: Run the linters\n---\n')
w(join(installed, 'agents', 'auditor.md'), '---\nname: auditor\ndescription: Audits\n---\n')
w(join(market, '.claude-plugin', 'marketplace.json'), JSON.stringify({ name: 'synthetic-market', owner: { name: 'T' }, plugins: [{ name: 'installed-kit', source: './installed-kit' }] }))
for (const args of [
  ['plugin', 'marketplace', 'add', market],
  ['plugin', 'install', 'installed-kit@synthetic-market'],
  ['mcp', 'add', '--scope', 'user', 'tickets', '--env', 'TICKETS_TOKEN=SECRET-ENV', '--', 'node', 'tickets.js', '--key', 'SECRET-ARG'],
]) {
  const r = claudeIn(cfgWork, args)
  check(`claude ${args.slice(0, 2).join(' ')} (synthetic Work profile)`, r.status === 0, (r.stderr || '').trim().slice(0, 120))
}
w(join(cfgWork, 'skills', 'deploy', 'SKILL.md'), '---\nname: deploy\ndescription: Deploy the app\n---\n')
w(join(cfgWork, 'agents', 'reviewer.md'), '---\nname: reviewer\ndescription: Reviews diffs\n---\n')
cli(['profiles', 'add', '--name', 'Work', '--config-dir', cfgWork, '--executable', fakeClaude])
cli(['profiles', 'add', '--name', 'Personal', '--config-dir', cfgPersonal, '--executable', fakeClaude])

const diag = JSON.parse(cli(['--json', 'profiles', 'diagnose', 'Work']))
const real = init(cfgWork)
const realSkills = (real?.skills ?? []).filter((s) => !builtinSkills.has(s))
const realAgents = (real?.agents ?? []).filter((a) => !builtinAgents.has(a))
const realPlugins = (real?.plugins ?? []).filter((p) => p.path !== 'builtin').map((p) => p.name)
const realMcp = (real?.mcp_servers ?? []).map((m) => m.name)
check('diagnostics list exactly the skills Claude loads', same(diag.skills.map((s) => s.name), realSkills), `nexus ${diag.skills.map((s) => s.name)} · claude ${realSkills}`)
check('diagnostics list exactly the agents Claude loads', same(diag.agents.map((a) => a.name), realAgents), `nexus ${diag.agents.map((a) => a.name)} · claude ${realAgents}`)
check('diagnostics list exactly the plugins Claude loads', same(diag.plugins.filter((p) => p.enabled && p.installed).map((p) => p.name), realPlugins), `claude ${realPlugins}`)
check('diagnostics list exactly the MCP servers Claude reports', same(diag.mcpServers.map((m) => m.qualifiedName), realMcp), `nexus ${diag.mcpServers.map((m) => m.qualifiedName)} · claude ${realMcp}`)
check('no planted secret value appears in the diagnostics', !JSON.stringify(diag).includes('SECRET'))

// ---- B. kits per profile ----------------------------------------------------------------------------------------
const kit = join(root, 'team-kit')
w(join(kit, '.claude-plugin', 'plugin.json'), JSON.stringify({ name: 'team-kit', version: '1.0.0', description: 'Team kit' }))
for (const s of ['alpha', 'beta', 'gamma']) w(join(kit, 'skills', s, 'SKILL.md'), `---\nname: ${s}\ndescription: Team skill ${s}\n---\n`)
w(join(kit, '.mcp.json'), JSON.stringify({ mcpServers: { teamdb: { command: process.execPath, args: ['-e', 'setTimeout(()=>{},50)'] } } }))
const before = [snapshot(cfgWork), snapshot(cfgPersonal)]
cli(['kits', 'add', kit])
cli(['kits', 'approve', 'team-kit'])
cli(['kits', 'enable', 'team-kit', '-p', 'Work', '--skills', 'alpha'])
check('NEXUS kit operations did not touch either profile', before[0] === snapshot(cfgWork) && before[1] === snapshot(cfgPersonal))

function nexusPluginDirs(profileName) {
  const report = join(root, `report-${profileName}-${Math.random().toString(36).slice(2)}.json`)
  cli(['launch', '--profile', profileName, '--cwd', proj, '-q'], { FAKE_CLAUDE_REPORT: report })
  const args = JSON.parse(readFileSync(report, 'utf8')).args
  return args.flatMap((a, i) => (args[i - 1] === '--plugin-dir' ? ['--plugin-dir', a] : []))
}
const workDirs = nexusPluginDirs('Work')
const personalDirs = nexusPluginDirs('Personal')
check('NEXUS passes the kit only to Work sessions', workDirs.length === 2 && personalDirs.length === 0, `${workDirs.length / 2} / ${personalDirs.length / 2}`)
const workInit = init(cfgWork, workDirs)
const personalInit = init(cfgPersonal, personalDirs)
const kitSkills = (e) => (e?.skills ?? []).filter((s) => s.startsWith('team-kit:'))
check('the real Work session has only the selected kit skill', same(kitSkills(workInit), ['team-kit:alpha']), JSON.stringify(kitSkills(workInit)))
check('…from NEXUS’s overlay, not the kit folder', (workInit?.plugins ?? []).some((p) => p.name === 'team-kit' && p.path.startsWith(data)))
check('…and without the kit’s MCP server (not opted in)', !(workInit?.mcp_servers ?? []).some((m) => m.name.includes('team-kit')))
check('the real Personal session has nothing from the kit', kitSkills(personalInit).length === 0 && !(personalInit?.plugins ?? []).some((p) => p.name === 'team-kit'))
cli(['kits', 'enable', 'team-kit', '-p', 'Work', '--skills', 'alpha', '--mcp'])
const withMcp = init(cfgWork, nexusPluginDirs('Work'))
check('opting in to MCP makes the kit’s server appear in Work only', (withMcp?.mcp_servers ?? []).some((m) => m.name === 'plugin:team-kit:teamdb'))
w(join(kit, 'skills', 'alpha', 'SKILL.md'), '---\nname: alpha\ndescription: changed upstream\n---\n')
check('a changed kit is withheld until it is reviewed again', nexusPluginDirs('Work').length === 0)

// ---- C. the workspace's own skills (Portable Workspace P2, ADR-0037) ------------------------------------------------
cli(['workspaces', 'create', join(root, 'ws'), '--name', 'Portable', '--template', 'developer'])
cli(['projects', 'add', proj])
const wsInit = init(cfgPersonal, nexusPluginDirs('Personal'))
const wsSkills = (wsInit?.skills ?? []).filter((s) => s.startsWith('workspace:'))
check('a real session in the workspace gets the template skills as workspace:<skill>', wsSkills.length === 5 && wsSkills.includes('workspace:feature-development'), JSON.stringify(wsSkills))
check('…from NEXUS’s overlay', (wsInit?.plugins ?? []).some((p) => p.name === 'workspace' && p.path.startsWith(data)))

console.log(failures === 0 ? '\nREAL-CLAUDE KITS OK' : `\nREAL-CLAUDE KITS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* ignore */
}
