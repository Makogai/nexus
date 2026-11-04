// End-to-end smoke test against the REAL running app (real IPC, real PTY, real UI).
//
// Prerequisites (see docs/TESTING.md):
//   * `cargo build -p fake-claude`
//   * NEXUS running with an EMPTY isolated database and remote debugging, e.g. (Git Bash):
//       NEXUS_DATA_DIR=$TMP/nexus-smoke WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 npm run tauri dev
//
// Uses only synthetic fixtures in a temp directory; never touches real Claude profiles.
// Native folder pickers cannot be automated, so setup calls the same IPC commands those dialogs feed.
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

const exe = process.platform === 'win32' ? 'fake-claude.exe' : 'fake-claude'
const fakeClaude = fileURLToPath(new URL(`../../../target/debug/${exe}`, import.meta.url))
const root = mkdtempSync(join(tmpdir(), 'nexus-smoke-'))
const js = (v) => JSON.stringify(v)
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

// Fixture repository with a secret embedded in its remote URL.
const repo = join(root, 'repo')
mkdirSync(join(repo, 'src'), { recursive: true })
writeFileSync(join(repo, 'package.json'), js({ name: 'smoke', dependencies: { vue: '3' }, scripts: { dev: 'vite' } }))
writeFileSync(join(repo, 'src', 'main.ts'), 'x\n')
const git = (...args) => execFileSync('git', ['-c', 'user.name=S', '-c', 'user.email=s@example.invalid', ...args], { cwd: repo })
git('init', '-q', '-b', 'main')
git('add', '-A')
git('commit', '-qm', 'init')
git('remote', 'add', 'origin', 'https://user:smoke-secret@example.invalid/acme/smoke.git')

await waitForPort()
const cdp = await connect()
try {
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 20000)
  const setup = await cdp.evaluate(`(async () => {
    const inv = window.__TAURI_INTERNALS__.invoke
    const ws = await inv('workspace_create', { path: ${js(join(root, 'ws'))}, name: 'Smoke', description: null })
    const project = await inv('project_register', { workspaceId: ws.id, path: ${js(repo)}, name: null })
    const profile = await inv('profile_create', { input: { name: 'Smoke Fake', configDir: ${js(join(root, 'cfg'))},
      executable: ${js(fakeClaude)}, defaultWorkProfile: 'prototype' } })
    return { project, profile }
  })()`)
  check('remote URL credentials are redacted', setup.project.remoteUrl === 'https://example.invalid/acme/smoke.git', setup.project.remoteUrl)
  check('stack detected deterministically', setup.project.stack.stacks.includes('vue'))

  // Drive the real UI: reload, open the launch dialog, launch.
  await cdp.evaluate(`location.hash = '#/'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell .topbar .btn-primary')`, 20000)
  await cdp.evaluate(`document.querySelector('.topbar .btn-primary').click(), true`)
  const selected = await cdp.waitFor(
    `document.querySelector('#launch-profile') && document.querySelector('.segment.selected')?.innerText`,
    5000,
  )
  check('launch dialog applies the profile default rigor', selected === 'Prototype', selected)
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(), true`)
  const raw = await cdp.waitFor(
    `(() => { const r = [...document.querySelectorAll('.xterm-rows')].pop(); const t = r?.innerText.replace(/\\n/g, '') ?? ''; const m = t.match(/\\{"args".*"fakeClaude":true\\}/); return m && m[0] })()`,
    20000,
  )
  const report = JSON.parse(raw)
  const env = report.env
  check('CLAUDE_CONFIG_DIR is the profile directory', env.CLAUDE_CONFIG_DIR === join(root, 'cfg'), env.CLAUDE_CONFIG_DIR)
  for (const k of ['CLAUDECODE', 'CLAUDE_CODE_SESSION_ID', 'CLAUDE_CODE_CHILD_SESSION', 'CLAUDE_CODE_MESSAGING_TOKEN', 'ANTHROPIC_API_KEY', 'OTEL_EXPORTER_OTLP_ENDPOINT']) {
    check(`${k} does not reach Claude`, env[k] === null, String(env[k]))
  }
  check('NEXUS assigns the Claude session id', report.args[0] === '--session-id' && /^[0-9a-f-]{36}$/.test(report.args[1]))
  check('work profile prompt appended', report.args.some((a) => a.startsWith('NEXUS work profile: PROTOTYPE')))

  // Exit is recorded; a conversation without a transcript is not offered for resume.
  const session = await cdp.waitFor(`(async () => {
    const l = await window.__TAURI_INTERNALS__.invoke('session_list', { filter: null })
    const s = l.find((x) => x.profileId === ${js(setup.profile.id)})
    return s && !s.running && s.state === 'exited' ? s : null
  })()`, 15000)
  check('exit code recorded', session.exitCode === 0)
  check('no transcript → not resumable', session.resumable === false && session.conversationRecorded === false)

  // With a transcript, Resume continues the same conversation.
  mkdirSync(join(root, 'cfg', 'projects', 'smoke'), { recursive: true })
  writeFileSync(join(root, 'cfg', 'projects', 'smoke', `${session.claudeSessionId}.jsonl`), '{}\n')
  const resumed = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('session_resume', { id: ${js(session.id)}, cols: 120, rows: 30 })`)
  check('resume creates a linked session', resumed.resumedFrom === session.id && resumed.claudeSessionId === session.claudeSessionId)
} finally {
  cdp.close()
}
console.log(failures === 0 ? '\nSMOKE OK' : `\nSMOKE FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
