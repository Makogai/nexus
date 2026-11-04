// Context Inspector against the REAL Claude Code CLI (ADR-0028; requires `claude` on PATH; skipped otherwise).
// A synthetic, signed-out CLAUDE_CONFIG_DIR with a user CLAUDE.md, and a project with CLAUDE.md, a rule and a
// CLAUDE.local.md. NEXUS prepares the session (exact argv captured from the test double), the real CLI runs it in
// print mode, its InstructionsLoaded events reach NEXUS through the session-scoped hook, and `nexus context inspect`
// must show every observed file as predicted — and nothing predicted as missing. Never touches a real profile.
//
//   cargo build -p nexus-cli -p fake-claude
//   node e2e/context-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ext = process.platform === 'win32' ? '.exe' : ''
const nexus = fileURLToPath(new URL(`../../../target/debug/nexus${ext}`, import.meta.url))
const fake = fileURLToPath(new URL(`../../../target/debug/fake-claude${ext}`, import.meta.url))
const which = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['claude'], { encoding: 'utf8' })
if (which.status !== 0) {
  console.log('SKIP  claude is not installed')
  process.exit(0)
}
const claudeExe = which.stdout.split(/\r?\n/).map((l) => l.trim()).find((l) => /claude(\.exe)?$/i.test(l)) ?? 'claude'

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const root = mkdtempSync(join(tmpdir(), 'nexus-context-real-'))
const data = join(root, 'data')
const repo = join(root, 'repo')
const cfg = join(root, 'claude-config') // synthetic, signed out
mkdirSync(join(repo, '.claude', 'rules'), { recursive: true })
mkdirSync(join(repo, 'docs'), { recursive: true })
mkdirSync(cfg)
writeFileSync(join(cfg, 'CLAUDE.md'), 'User memory of the synthetic profile.\n')
writeFileSync(join(repo, 'CLAUDE.md'), 'Project instructions.\n')
writeFileSync(join(repo, 'CLAUDE.local.md'), 'Local notes.\n')
writeFileSync(join(repo, '.claude', 'rules', 'style.md'), 'Use tabs.\n')
writeFileSync(join(repo, 'README.md'), '# Repo\n\n## Payments\n\nHosted checkout page.\n')
execFileSync('git', ['init', '-q', '-b', 'main'], { cwd: repo })
const cli = (...args) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8', env: { ...process.env, USERPROFILE: join(root, 'home'), HOME: join(root, 'home') } })
mkdirSync(join(root, 'home'))
cli('workspaces', 'create', join(root, 'ws'), '--name', 'Ctx')
cli('projects', 'add', repo)
// The profile points at the synthetic config dir; the test double captures the exact launch NEXUS prepares.
cli('profiles', 'add', '--name', 'Synthetic', '--config-dir', cfg, '--executable', fake)

const reportPath = join(root, 'report.json')
execFileSync(nexus, ['--data-dir', data, 'launch', '--project', 'repo', '-q'], { env: { ...process.env, USERPROFILE: join(root, 'home'), HOME: join(root, 'home'), FAKE_CLAUDE_REPORT: reportPath } })
const args = JSON.parse(readFileSync(reportPath, 'utf8')).args
const settingsPath = args[args.indexOf('--settings') + 1]
const sessionId = args[args.indexOf('--session-id') + 1]
const settings = JSON.parse(readFileSync(settingsPath, 'utf8'))
check('the balanced default subscribes InstructionsLoaded for the inspector', Array.isArray(settings.hooks?.InstructionsLoaded))

const env = Object.fromEntries(
  Object.entries(process.env).filter(([k]) => !/^(CLAUDECODE|CLAUDE_CODE_|CLAUDE_PID|CLAUDE_EFFORT|CLAUDE_CONFIG_DIR|ANTHROPIC_|OTEL_)/i.test(k)),
)
env.CLAUDE_CONFIG_DIR = cfg
env.USERPROFILE = join(root, 'home')
env.HOME = join(root, 'home')
const run = spawnSync(claudeExe, ['-p', 'say ok', '--session-id', sessionId, '--settings', settingsPath], { cwd: repo, env, encoding: 'utf8', timeout: 120_000 })
check('real Claude ran (signed out, as expected)', /not logged in/i.test(`${run.stdout}${run.stderr}`), (run.stdout + run.stderr).trim().slice(0, 60))

const report = JSON.parse(cli('--json', 'context', 'inspect', '--project', 'repo', '--profile', 'Synthetic'))
const observed = report.observed.instructions
const names = observed.map((f) => f.path.replace(/\\/g, '/').split('/').slice(-2).join('/')).sort()
check('NEXUS recorded what the real CLI loaded', observed.length >= 3, names.join(', '))
check('every observed file was predicted by the inspector', observed.every((f) => f.predicted), observed.filter((f) => !f.predicted).map((f) => f.path).join(', '))
check('nothing predicted at start is missing', report.observed.missing.length === 0, report.observed.missing.join(', '))
check('…including the profile’s user memory and the project rule', names.includes('claude-config/CLAUDE.md') && names.includes('rules/style.md'), names.join(', '))
check('the configured strategy is the balanced default', report.configured.strategy === 'balanced' && report.configured.configured === false)

console.log(failures === 0 ? '\nREAL-CLAUDE CONTEXT OK' : `\nREAL-CLAUDE CONTEXT FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
