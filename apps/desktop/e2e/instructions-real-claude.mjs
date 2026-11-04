// The effective-instructions inspector (ADR-0019) against the REAL Claude CLI: build a synthetic profile and project
// tree, ask `nexus instructions` what Claude will load at session start, then run the real `claude -p` (signed out
// is fine — memory loads before authentication) with an `InstructionsLoaded` hook and compare the two sets.
// Only paths are recorded, never file contents. Skips when `claude` is not installed.
//
//   cargo build -p nexus-cli && node e2e/instructions-real-claude.mjs
import { execFileSync, spawnSync } from 'node:child_process'
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const nexus = fileURLToPath(new URL(`../../../target/debug/nexus${process.platform === 'win32' ? '.exe' : ''}`, import.meta.url))
const where = spawnSync(process.platform === 'win32' ? 'where' : 'which', ['claude'], { encoding: 'utf8' })
const claude = (where.stdout || '').split(/\r?\n/).find((l) => (process.platform === 'win32' ? l.toLowerCase().endsWith('.exe') : l.trim()))
if (!claude) {
  console.log('SKIP  claude is not installed')
  process.exit(0)
}

// Hook handler mode: append the loaded file path (only) and exit 0.
if (process.argv[2] === '--hook') {
  let p = {}
  try {
    p = JSON.parse(readFileSync(0, 'utf8'))
  } catch {
    /* ignore */
  }
  if (p.hook_event_name === 'InstructionsLoaded' && p.file_path) appendFileSync(process.argv[3], `${JSON.stringify({ path: p.file_path, reason: p.load_reason })}\n`)
  process.exit(0)
}

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const root = mkdtempSync(join(tmpdir(), 'nexus-instr-'))
const w = (p, t) => {
  mkdirSync(dirname(p), { recursive: true })
  writeFileSync(p, t)
}
const cfg = join(root, 'cfg')
const parent = join(root, 'parent')
const proj = join(parent, 'proj')
w(join(cfg, 'CLAUDE.md'), 'probe: user memory\n@extra.md\n')
w(join(cfg, 'extra.md'), 'probe: user import\n')
w(join(cfg, 'rules', 'user-rule.md'), 'probe: user rule\n')
w(join(parent, 'CLAUDE.md'), 'probe: parent memory\n')
w(join(parent, '.claude', 'CLAUDE.md'), 'probe: parent dot-claude\n')
w(join(parent, '.claude', 'rules', 'parent-rule.md'), 'probe: parent rule\n')
w(join(parent, 'CLAUDE.local.md'), 'probe: parent local (excluded)\n')
w(join(proj, 'CLAUDE.md'), 'probe: project memory\n@docs/imported.md\n`@docs/not-imported.md`\n@../outside.md\n')
w(join(parent, 'outside.md'), 'probe: import from outside the project (needs approval)\n')
w(join(proj, 'docs', 'imported.md'), 'probe: imported\n')
w(join(proj, 'docs', 'not-imported.md'), 'probe: inside a code span\n')
w(join(proj, '.claude', 'CLAUDE.md'), 'probe: dot-claude\n')
w(join(proj, '.claude', 'rules', 'always.md'), 'probe: unconditional rule\n')
w(join(proj, '.claude', 'rules', 'cond.md'), '---\npaths:\n  - "src/**/*.rs"\n---\nprobe: conditional rule\n')
w(join(proj, '.claude', 'settings.json'), JSON.stringify({ claudeMdExcludes: ['**/parent/CLAUDE.local.md'] }))
w(join(proj, 'CLAUDE.local.md'), 'probe: local\n')
w(join(proj, 'AGENTS.md'), 'probe: agents (a CLAUDE.md applies)\n')
w(join(proj, 'sub', 'CLAUDE.md'), 'probe: subdirectory (lazy)\n')
execFileSync('git', ['init', '-q'], { cwd: proj })

const norm = (p) => p.replace(/\\/g, '/').toLowerCase()
const mine = (p) => norm(p).startsWith(norm(root))

// What NEXUS says Claude loads at start.
const data = join(root, 'nexus-data')
const run = (...args) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8' })
run('profiles', 'add', '--name', 'Probe', '--config-dir', cfg)
const inspected = JSON.parse(run('--json', 'instructions', '--profile', 'Probe', '--cwd', proj))
const predicted = new Set(inspected.claude.layers.filter((l) => l.load === 'at-start' && l.path && mine(l.path)).map((l) => norm(l.path)))

// What the real Claude loads.
const out = join(root, 'loaded.jsonl')
const settings = join(root, 'settings.json')
const me = fileURLToPath(import.meta.url)
writeFileSync(settings, JSON.stringify({ hooks: { InstructionsLoaded: [{ hooks: [{ type: 'command', command: process.execPath, args: [me, '--hook', out], timeout: 10 }] }] } }))
const env = { ...process.env }
for (const k of Object.keys(env)) if (/^(ANTHROPIC_|CLAUDE_CODE_|CLAUDECODE|OTEL_)/.test(k)) delete env[k]
env.CLAUDE_CONFIG_DIR = cfg
const version = spawnSync(claude, ['--version'], { encoding: 'utf8', env }).stdout.trim()
const r = spawnSync(claude, ['-p', 'hi', '--settings', settings], { cwd: proj, env, encoding: 'utf8', timeout: 120000 })
check('real Claude ran', r.status !== null, `${version}; exit ${r.status}`)
const loaded = existsSync(out)
  ? new Set(readFileSync(out, 'utf8').trim().split('\n').map((l) => JSON.parse(l)).filter((e) => mine(e.path)).map((e) => norm(e.path)))
  : new Set()
check('InstructionsLoaded reported files', loaded.size > 0, `${loaded.size} file(s)`)

const missing = [...loaded].filter((p) => !predicted.has(p))
const extra = [...predicted].filter((p) => !loaded.has(p))
const rel = (p) => p.slice(norm(root).length)
check('NEXUS predicts every file Claude loads', missing.length === 0, missing.map(rel).join(', '))
check('NEXUS predicts no file Claude does not load', extra.length === 0, extra.map(rel).join(', '))
check('the conditional rule, subdirectory file and excluded file are not loaded at start', ![...loaded].some((p) => /cond\.md$|\/sub\/claude\.md$|parent\/claude\.local\.md$/.test(p)))
const outside = inspected.claude.layers.find((l) => l.path && norm(l.path).endsWith('/parent/outside.md'))
check('an import from outside the project is not loaded without approval — and the inspector says so', !loaded.has(norm(join(parent, 'outside.md'))) && outside?.load === 'needs-approval', outside?.load)

console.log(failures === 0 ? '\nREAL-CLAUDE INSTRUCTIONS OK' : `\nREAL-CLAUDE INSTRUCTIONS FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
try {
  rmSync(root, { recursive: true, force: true })
} catch {
  /* ignore */
}
