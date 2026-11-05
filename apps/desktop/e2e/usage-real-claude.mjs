// Usage analytics against the REAL Claude Code CLI (requires `claude` on PATH; skipped otherwise), ADR-0030.
// A synthetic, empty, signed-out CLAUDE_CONFIG_DIR — never a real profile. Claude starts a print-mode session with
// NEXUS's session id and hook settings, writes its real transcript and exits ("Not logged in"). NEXUS must find
// that transcript in the profile, parse the real format, report "no API requests" with the reason (no tokens and no
// cost invented), record the Claude Code version, and leave the transcript unchanged.
//
//   cargo build -p nexus-cli -p fake-claude
//   node e2e/usage-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
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

const root = mkdtempSync(join(tmpdir(), 'nexus-usage-real-'))
const data = join(root, 'data')
const repo = join(root, 'repo')
const claudeCfg = join(root, 'claude-config') // synthetic, empty, signed out — the NEXUS profile's config dir
mkdirSync(repo)
mkdirSync(claudeCfg)
writeFileSync(join(repo, 'README.md'), '# usage probe\n')
execFileSync('git', ['init', '-q', '-b', 'main'], { cwd: repo })
const home = join(root, 'home')
mkdirSync(home)
const cliEnv = { ...process.env, USERPROFILE: home, HOME: home }
const cli = (...args) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8', env: cliEnv })
cli('workspaces', 'create', join(root, 'ws'), '--name', 'Usage')
cli('projects', 'add', repo)
// The profile points at the synthetic config dir; the test double captures the argv NEXUS builds.
cli('profiles', 'add', '--name', 'Synthetic', '--config-dir', claudeCfg, '--executable', fake)

const reportPath = join(root, 'report.json')
execFileSync(nexus, ['--data-dir', data, 'launch', '--project', 'repo', '-q'], { env: { ...cliEnv, FAKE_CLAUDE_REPORT: reportPath } })
const args = JSON.parse(readFileSync(reportPath, 'utf8')).args
const settingsPath = args[args.indexOf('--settings') + 1]
const claudeSessionId = args[args.indexOf('--session-id') + 1]
const session = JSON.parse(cli('--json', 'sessions'))[0]

// The real CLI, clean environment (no parent markers, no API key, no telemetry), same config dir and session id.
const env = Object.fromEntries(
  Object.entries(process.env).filter(([k]) => !/^(CLAUDECODE|CLAUDE_CODE_|CLAUDE_PID|CLAUDE_EFFORT|CLAUDE_CONFIG_DIR|ANTHROPIC_|OTEL_)/i.test(k)),
)
env.CLAUDE_CONFIG_DIR = claudeCfg
const run = spawnSync(claudeExe, ['-p', 'say ok', '--session-id', claudeSessionId, '--settings', settingsPath], { cwd: repo, env, encoding: 'utf8', timeout: 120_000 })

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const walk = (d) => (existsSync(d) ? readdirSync(d).flatMap((e) => (statSync(join(d, e)).isDirectory() ? walk(join(d, e)) : [join(d, e)])) : [])
check('real Claude ran (signed out, as expected)', /not logged in/i.test(`${run.stdout}${run.stderr}`), (run.stdout + run.stderr).trim().slice(0, 60))
const transcript = walk(join(claudeCfg, 'projects')).find((p) => p.endsWith(`${claudeSessionId}.jsonl`))
check('the real CLI wrote its transcript where NEXUS looks (projects/<cwd>/<id>.jsonl)', !!transcript, transcript?.slice(claudeCfg.length) ?? 'none')
const before = transcript ? readFileSync(transcript) : null

const sessions = JSON.parse(cli('--json', 'usage', 'sessions', '--days', '0'))
const mine = sessions.find((s) => s.sessionId === session.id)
check('NEXUS parsed the real format: no API request, with the reason', mine?.status === 'no-requests' && /local error message/.test(mine?.detail ?? ''), `${mine?.status}: ${mine?.detail}`)
check('…no tokens and no cost are invented', mine?.tokens.requests === 0 && mine?.cost.usd === null, JSON.stringify([mine?.tokens.requests, mine?.cost]))
const report = JSON.parse(cli('--json', 'usage', '--days', '0')).report
check('the report counts the session as covered, not as zero usage', report.coverage.sessions === 1 && report.coverage.noRequests === 1 && report.totals.requests === 0, JSON.stringify(report.coverage))
const db = readFileSync(join(data, 'nexus.db'))
check('the transcript stays unchanged (read-only)', !!before && readFileSync(transcript).equals(before))
check('no prompt text reaches the NEXUS database', !db.includes(Buffer.from('say ok')))
const version = execFileSync(claudeExe, ['--version'], { encoding: 'utf8', env }).trim()
console.log(`\n(Claude Code ${version})`)
console.log(failures === 0 ? '\nREAL-CLAUDE USAGE OK' : `\nREAL-CLAUDE USAGE FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
