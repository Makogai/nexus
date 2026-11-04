// Lifecycle hooks against the REAL Claude Code CLI (requires `claude` on PATH; skipped otherwise).
// Uses a synthetic, empty, signed-out CLAUDE_CONFIG_DIR — never a real profile. Claude starts a print-mode session,
// runs SessionStart/SessionEnd hooks from the NEXUS-generated --settings file and exits ("Not logged in").
//
//   cargo build -p nexus-cli -p fake-claude
//   node e2e/hooks-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, writeFileSync, readFileSync } from 'node:fs'
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
// Spawn the resolved native executable directly (no shell), as NEXUS itself does.
const claudeExe = which.stdout.split(/\r?\n/).map((l) => l.trim()).find((l) => /claude(\.exe)?$/i.test(l)) ?? 'claude'

const root = mkdtempSync(join(tmpdir(), 'nexus-hooks-real-'))
const data = join(root, 'data')
const repo = join(root, 'repo')
const claudeCfg = join(root, 'claude-config') // synthetic, empty, signed out
mkdirSync(repo)
mkdirSync(claudeCfg)
writeFileSync(join(repo, 'README.md'), '# hooks probe\n')
execFileSync('git', ['init', '-q', '-b', 'main'], { cwd: repo })
const cli = (...args) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8' })
cli('workspaces', 'create', join(root, 'ws'), '--name', 'Hooks')
cli('projects', 'add', repo)
cli('profiles', 'add', '--name', 'Fake', '--create', '--config-dir', join(root, 'fake-cfg'), '--executable', fake)

// 1. Create a NEXUS session the normal way and capture the exact argv NEXUS builds (from the test double).
const reportPath = join(root, 'report.json')
execFileSync(nexus, ['--data-dir', data, 'launch', '--project', 'repo', '-q'], { env: { ...process.env, FAKE_CLAUDE_REPORT: reportPath } })
const args = JSON.parse(readFileSync(reportPath, 'utf8')).args
const settingsPath = args[args.indexOf('--settings') + 1]
const claudeSessionId = args[args.indexOf('--session-id') + 1]
const session = JSON.parse(cli('--json', 'sessions'))[0]

// 2. Run the real Claude with NEXUS's hook settings (clean env: no parent Claude markers, no API key).
const env = Object.fromEntries(
  Object.entries(process.env).filter(([k]) => !/^(CLAUDECODE|CLAUDE_CODE_|CLAUDE_PID|CLAUDE_EFFORT|CLAUDE_CONFIG_DIR|ANTHROPIC_|OTEL_)/i.test(k)),
)
env.CLAUDE_CONFIG_DIR = claudeCfg
const run = spawnSync(claudeExe, ['-p', 'say ok', '--session-id', claudeSessionId, '--settings', settingsPath], {
  cwd: repo,
  env,
  encoding: 'utf8',
  timeout: 120_000,
})

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
check('real Claude ran (signed out, as expected)', /not logged in/i.test(`${run.stdout}${run.stderr}`), (run.stdout + run.stderr).trim().slice(0, 80))
const after = JSON.parse(cli('--json', 'sessions')).find((s) => s.id === session.id)
check('SessionStart hook reached NEXUS (transcript path recorded)', typeof after.transcriptPath === 'string' && after.transcriptPath.includes(claudeSessionId))
check('SessionEnd hook reached NEXUS (end reason recorded)', typeof after.endReason === 'string', after.endReason)
check('Claude conversation id unchanged (same session)', after.claudeSessionId === claudeSessionId)

// 3. Attention (ADR-0025): the real CLI accepts NEXUS's attention hooks and reports the signed-out failure.
const hooks = JSON.parse(readFileSync(settingsPath, 'utf8')).hooks
check('settings register the attention hooks', ['PermissionRequest', 'StopFailure', 'Elicitation', 'PreToolUse'].every((e) => hooks[e]) && hooks.PreToolUse[0].matcher === 'AskUserQuestion')
check('real Claude accepted them (no settings error)', !/settings error|invalid settings|unknown hook/i.test(`${run.stdout}${run.stderr}`))
const inbox = JSON.parse(cli('--json', 'notifications', 'list', '--all'))
const issue = inbox.notifications.find((n) => n.sessionId === session.id && n.category === 'process-issue')
check('StopFailure from the real CLI became an action notification', issue?.title === 'Claude is not signed in for this profile' && issue?.priority === 'action', issue?.title)
check('…bound to the session’s workspace and profile', issue?.workspaceId === session.workspaceId && issue?.profileId === session.profileId)
check('…and no model output is stored', !JSON.stringify(inbox).includes('Not logged in'))
console.log(failures === 0 ? '\nREAL-CLAUDE HOOKS OK' : `\nREAL-CLAUDE HOOKS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
