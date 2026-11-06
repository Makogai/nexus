// End-to-end: the project workbench milestone (W2–W4) against the REAL app — temporary workspace and repositories only.
//   W2 Knowledge: a constitution opens as rule cards (no raw markers); a note opens as rendered Markdown with raw HTML
//      escaped; Edit → Save writes the file (approved here); a stale save is a conflict; Constitutions open in Read mode.
//   W3 Explorer: project tree without .git and ignored output, read-only viewer, secrets never shown; commits with
//      changed files, renames and diffs.
//   W4 Learn: NEXUS's own map, Graphify optional, the learning task with the map as brief, drafts applied or set aside.
//
//   npx tauri build --debug --no-bundle --features e2e-webview-debugging
//   node e2e/workbench.mjs [path/to/nexus-desktop(.exe)]
//
// Windows only (WebView2 DevTools protocol).
import { execFileSync, spawn } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('workbench: the CDP-based E2E currently requires Windows (WebView2).')
  process.exit(2)
}
const port = process.env.CDP_PORT ?? '9343'
const app = process.argv[2] ?? fileURLToPath(new URL('../../../target/debug/nexus-desktop.exe', import.meta.url))
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

// The real path: on CI the temp folder is an 8.3 short path, and NEXUS shows (and passes) canonical paths.
const root = realpathSync.native(mkdtempSync(join(tmpdir(), 'nexus-workbench-')))
let log = ''
const child = spawn(app, [], {
  env: { ...process.env, NEXUS_DATA_DIR: join(root, 'data'), NEXUS_E2E_NOTIFIER: 'record', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_REPORT: join(root, 'fake-report.json') },
  stdio: ['ignore', 'pipe', 'pipe'],
})
child.stdout.on('data', (d) => (log += d))
child.stderr.on('data', (d) => (log += d))

let cdp
const inv = (cmd, args = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`)
const click = (sel) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el || el.disabled) return false; el.click(); return true })()`)
const textOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerText ?? ''`)
const htmlOf = (sel) => cdp.evaluate(`document.querySelector(${js(sel)})?.innerHTML ?? ''`)
async function until(fn, ms = 20000) {
  const deadline = Date.now() + ms
  for (;;) {
    const v = await fn().catch(() => null)
    if (v) return v
    if (Date.now() > deadline) return null
    await sleep(250)
  }
}
/** Replace the CodeMirror document the way a person does: focus, Ctrl+A, type. */
async function setEditor(sel, text) {
  const focused = await cdp.evaluate(`(() => { const c = document.querySelector(${js(sel)} + ' .cm-content'); if (!c) return false; c.focus(); return true })()`)
  if (!focused) return false
  const key = { modifiers: 2, key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65 }
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyDown', ...key })
  await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...key })
  await cdp.send('Input.insertText', { text })
  await sleep(200)
  return cdp.evaluate(`(document.querySelector(${js(sel)} + ' .cm-content')?.innerText ?? '').includes(${js(text.split('\n').pop())})`)
}
const closeDialog = () => cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn')].find((b) => b.innerText.trim() === 'Close')?.click(); true`)

try {
  await waitForPort(port, 120_000)
  cdp = await connect(port)
  await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 60000)
  const ws = await inv('workspace_create', { path: join(root, 'ws'), name: 'Bench', description: null, template: 'ai-engineering-pro' })
  const note = await inv('knowledge_create', {
    workspaceId: ws.id,
    input: { kind: 'note', title: 'Release notes', body: '## Plan\n\nShip **on Tuesday**.\n\n<script>window.__pwned = 1</script>\n\n[docs](https://example.com)' },
  })

  // --- W2: reading ---------------------------------------------------------------------------------------------------
  await cdp.evaluate(`location.hash = '#/knowledge'; location.reload(); true`)
  await cdp.waitFor(`document.querySelector('.shell') && document.querySelector('.filter')`, 30000)
  await cdp.evaluate(`[...document.querySelectorAll('.filter')].find((f) => /^Rules/.test(f.innerText.trim()))?.click(); true`)
  await cdp.waitFor(`[...document.querySelectorAll('.list-row.doc')].some((r) => r.innerText.includes('Security'))`, 15000)
  await cdp.evaluate(`[...document.querySelectorAll('.list-row.doc')].find((r) => r.innerText.includes('Security'))?.click(); true`)
  const cards = await until(async () => ((await cdp.evaluate(`document.querySelectorAll('[role=dialog] [data-constitution-rules] [data-rule]').length`)) || null))
  const reading = await textOf('[role=dialog] [data-doc-reading]')
  check('a constitution opens as rule cards with ids and enforcement', cards >= 10 && reading.includes('SEC-1') && reading.includes('Mandatory'), `${cards} cards`)
  check('…without raw rule markers', !reading.includes('<!--') && !reading.includes('enforcement='))
  await closeDialog()

  await cdp.evaluate(`[...document.querySelectorAll('.filter')].find((f) => /^Notes/.test(f.innerText.trim()))?.click(); true`)
  await cdp.waitFor(`document.querySelector('[data-doc="${note.id}"]')`, 15000)
  await click(`[data-doc="${note.id}"]`)
  await cdp.waitFor(`document.querySelector('[role=dialog] [data-markdown]')`, 10000)
  const html = await htmlOf('[role=dialog] [data-markdown]')
  check('a note opens as rendered Markdown', html.includes('<h2>Plan</h2>') && html.includes('<strong>on Tuesday</strong>'), html.slice(0, 80))
  check('…raw HTML in a document is shown as text, never run', !html.includes('<script') && (await cdp.evaluate(`window.__pwned === undefined`)))

  // --- W2: editing ---------------------------------------------------------------------------------------------------
  await click('[data-doc-edit-start]')
  await cdp.waitFor(`!!document.querySelector('[data-doc-edit] .cm-content')`, 10000)
  await cdp.evaluate(`(() => { const i = document.querySelector('[data-doc-edit-title]'); i.value = 'Release plan'; i.dispatchEvent(new Event('input', { bubbles: true })); return true })()`)
  const typed = await setEditor('[data-doc-edit]', '## Plan\n\nShip **on Wednesday**.')
  await click('[data-doc-save]')
  const saved = await until(async () => ((await cdp.evaluate(`!!document.querySelector('[role=dialog] [data-markdown]')`)) ? await htmlOf('[role=dialog] [data-markdown]') : null))
  const file = readFileSync(note.path, 'utf8')
  check('Edit → Save writes the document and shows it rendered again', typed && !!saved && saved.includes('on Wednesday') && file.includes('title: Release plan') && file.includes('Ship **on Wednesday**.'), file.split('\n').slice(0, 4).join(' | '))
  const doc = await inv('knowledge_read', { workspaceId: ws.id, id: note.id })
  check('…and the saved version is approved on this machine', doc.approved === true)

  // A change on disk after opening: saving is refused as a conflict and the file keeps the other change.
  await click('[data-doc-edit-start]')
  await cdp.waitFor(`!!document.querySelector('[data-doc-edit] .cm-content')`, 10000)
  writeFileSync(note.path, readFileSync(note.path, 'utf8').replace('on Wednesday', 'on Thursday'))
  await setEditor('[data-doc-edit]', '## Plan\n\nShip **on Friday**.')
  await click('[data-doc-save]')
  const conflict = await cdp.waitFor(`document.querySelector('[role=dialog] .notice-error')?.innerText`, 10000).catch(() => '')
  check('a stale save is refused as a conflict, nothing is overwritten', conflict.includes('changed on disk') && readFileSync(note.path, 'utf8').includes('on Thursday'), conflict.slice(0, 80))
  await cdp.evaluate(`document.querySelector('[role=dialog] footer .btn-ghost')?.click(); true`) // Cancel
  await cdp.evaluate(`document.querySelector('[data-confirm-ok]')?.click(); true`) // discard
  await closeDialog()

  // Constitutions open formatted; Edit is one click away.
  await cdp.evaluate(`location.hash = '#/constitutions'; true`)
  await cdp.waitFor(`document.querySelector('[data-constitution-detail] [data-constitution-rules]')`, 15000)
  check('the Constitution Manager opens a document in Read mode', (await textOf('[data-constitution-detail] [data-mode="read"]')) === 'Read' && !(await cdp.evaluate(`!!document.querySelector('[data-save]')`)))
  await click('[data-mode="structured"]')
  check('…and Edit shows the tree editor', !!(await until(async () => ((await cdp.evaluate(`!!document.querySelector('[data-save]')`)) ? true : null), 5000)))

  // --- W3: project explorer ----------------------------------------------------------------------------------------
  // The app is its own repository inside a wrapper repository (like drivebay inside maiw) — Learn offers the wrapper.
  const mono = join(root, 'mono')
  const repo = join(mono, 'apps', 'app')
  const git = (...args) => execFileSync('git', ['-c', 'user.name=Ann', '-c', 'user.email=ann@example.invalid', '-c', 'init.defaultBranch=main', '-c', 'core.autocrlf=false', ...args], { cwd: repo, encoding: 'utf8' })
  mkdirSync(join(repo, 'src'), { recursive: true })
  writeFileSync(join(mono, 'README.md'), '# mono\n')
  execFileSync('git', ['init', '-q'], { cwd: mono })
  writeFileSync(join(repo, 'src', 'main.ts'), 'export const answer = 41\n')
  writeFileSync(join(repo, 'README.md'), '# App\n')
  writeFileSync(join(repo, '.gitignore'), 'dist/\n')
  git('init', '-q')
  git('add', '-A')
  git('commit', '-qm', 'First version')
  writeFileSync(join(repo, 'src', 'main.ts'), 'export const answer = 42\n')
  git('mv', 'README.md', 'ABOUT.md')
  git('commit', '-qam', 'Fix the answer')
  mkdirSync(join(repo, 'dist'))
  writeFileSync(join(repo, 'dist', 'bundle.js'), 'x')
  writeFileSync(join(repo, '.env'), 'API_TOKEN=never-shown-in-nexus\n')
  const proj = await inv('project_register', { workspaceId: ws.id, path: repo, name: 'App' })
  // Registered through IPC: reload so the project store knows it (as after adding it in the UI).
  await cdp.evaluate(`location.hash = '#/projects/${proj.id}?tab=files'; location.reload(); true`)
  await cdp.waitFor(`!!document.querySelector('[data-project-files] [data-file="src"]')`, 20000)
  const tree = await cdp.evaluate(`[...document.querySelectorAll('[data-project-files] [data-file]')].map((n) => n.dataset.file).join(',')`)
  check('Files shows the project tree, folders first, without .git or ignored output', tree === 'src,.env,.gitignore,ABOUT.md', tree)
  await click('[data-project-files] [data-file="src"]')
  await cdp.waitFor(`!!document.querySelector('[data-file="src/main.ts"]')`, 10000)
  await click('[data-file="src/main.ts"]')
  const shown = await until(async () => {
    const t = await cdp.evaluate(`document.querySelector('[data-file-viewer] .cm-content')?.innerText ?? ''`)
    return t.includes('answer') ? t : null
  })
  check('…a file opens in the read-only viewer', !!shown && shown.includes('42') && (await cdp.evaluate(`document.querySelector('[data-file-viewer] .cm-content')?.getAttribute('aria-readonly')`)) === 'true', shown ?? '')
  await click('[data-file=".env"]')
  const hidden = await cdp.waitFor(`document.querySelector('[data-file-hidden]')?.innerText`, 10000).catch(() => '')
  check('…a file that may hold secrets is never shown', hidden.includes('secrets') && !(await cdp.evaluate(`document.body.innerText.includes('never-shown-in-nexus')`)), hidden.slice(0, 80))

  await click('[data-project-tab="commits"]')
  await cdp.waitFor(`document.querySelectorAll('[data-project-commits] [data-commit]').length === 2`, 15000)
  const history = await cdp.evaluate(`[...document.querySelectorAll('[data-commit] .summary')].map((s) => s.innerText).join(' | ')`)
  check('Commits lists the history, newest first, with author', history === 'Fix the answer | First version' && (await textOf('[data-commit]')).includes('Ann'), history)
  await cdp.evaluate(`document.querySelector('[data-commit]').click(); true`)
  await cdp.waitFor(`!!document.querySelector('[data-commit-file="src/main.ts"]')`, 10000)
  const changed = await textOf('[data-commit-files]')
  check('…a commit shows its changed files with counts and renames', changed.includes('README.md → ABOUT.md') && changed.includes('+1'), changed.replace(/\n/g, ' '))
  await click('[data-commit-file="src/main.ts"]')
  const diffText = await until(async () => {
    const t = await cdp.evaluate(`document.querySelector('[data-commit-diff] .cm-content')?.innerText ?? ''`)
    return t.includes('+export') ? t : null
  })
  check('…and a file opens as its diff', !!diffText && diffText.includes('-export const answer = 41') && diffText.includes('+export const answer = 42'), (diffText ?? '').slice(0, 80))

  // --- W4: learn the project -----------------------------------------------------------------------------------------
  await click('[data-project-tab="learn"]')
  const mapText = await cdp.waitFor(`document.querySelector('[data-learn-map]')?.innerText`, 30000).catch(() => '')
  check('Learn shows what NEXUS read itself (files, languages, history)', mapText.includes('tracked files') && mapText.includes('TypeScript') && /2\s*commits/.test(mapText), mapText.replace(/\n/g, ' ').slice(0, 160))
  check('…and Graphify only as an option (install hint or build button)', await cdp.evaluate(`!!document.querySelector('[data-learn-graph-missing], [data-learn-graph-build]')`))
  mkdirSync(join(repo, 'graphify-out'))
  writeFileSync(join(repo, 'graphify-out', 'GRAPH_REPORT.md'), '# Graph report\n')
  // Built while the person was elsewhere (e.g. in the terminal): coming back to the kept-alive project page reads again.
  await cdp.evaluate(`location.hash = '#/sessions'; true`)
  await sleep(1800)
  await cdp.evaluate(`location.hash = '#/projects/${proj.id}?tab=learn'; true`)
  const graphReady = await until(async () => ((await textOf('[data-learn-graph]')).includes('Graph ready') ? true : null))
  check('…a graph built meanwhile is recognised on coming back, without “Read again”', !!graphReady)

  // Extra context (ADR-0045): the wrapper repository is offered; notes go into the learning task.
  const offer = await cdp.waitFor(`document.querySelector('[data-learn-surrounding]')?.innerText`, 10000).catch(() => '')
  check('Learn offers the repository the project lives in', offer.includes('mono') && offer.includes(mono), offer.replace(/\n/g, ' ').slice(0, 140))
  await click('[data-learn-add-surrounding]')
  const added = await cdp.waitFor(`document.querySelector('[data-learn-folder="mono"]')?.innerText`, 10000).catch(() => '')
  check('…one click lets Claude read it too (listed, the offer gone)', added.includes(mono) && !(await cdp.evaluate(`!!document.querySelector('[data-learn-surrounding]')`)), added.replace(/\n/g, ' '))

  // A synthetic profile (fake Claude, temporary config folder) so the launch dialog can show the task.
  const cfg = join(root, 'claude-cfg')
  mkdirSync(cfg)
  await inv('profile_create', { input: { name: 'Fake', configDir: cfg, executable: fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url)), defaultWorkProfile: 'balanced' } })
  const refused = await cdp.evaluate(`window.__TAURI_INTERNALS__.invoke('project_learn_add_folder', ${js({ projectId: proj.id, path: cfg })}).then(() => 'added', (e) => 'refused: ' + JSON.stringify(e))`)
  check('…a Claude profile’s configuration folder is never given to Claude as context', refused.startsWith('refused') && refused.includes('Fake'), refused.slice(0, 160))
  await cdp.evaluate(`location.reload(); true`)
  await cdp.waitFor(`!!document.querySelector('[data-learn-start]')`, 30000)
  // The depth sets the budget; the rules Claude gets are shown as they are sent.
  await click('[data-learn-depth] [data-depth="quick"]')
  const budget = await textOf('[data-learn-budget]')
  check('Quick depth shows its budget', budget.includes('at most 20 files') && budget.includes('250 words'), budget)
  await click('[data-learn-rules-toggle]')
  const shownRules = await textOf('[data-learn-rules]')
  check('…and the exact rules Claude gets (LEARN-1 to LEARN-8 with the added folder, no subagents)', shownRules.includes('LEARN-2 Work alone') && shownRules.includes('LEARN-8') && shownRules.includes(mono), shownRules.slice(0, 80))
  // Notes for Claude: typed here, saved into the learning task when the session starts.
  await cdp.evaluate(`(() => { const t = document.querySelector('[data-learn-notes]'); t.value = 'app is one package of the mono repository; shared code lives in mono/packages.'; t.dispatchEvent(new Event('input')); return true })()`)
  await click('[data-learn-start]')
  const launch = await cdp.waitFor(`document.querySelector('[data-launch-task]')?.innerText`, 10000).catch(() => '')
  check('Start a learning session opens the launch for the learning task', launch.includes('Learn App'), launch.replace(/\n/g, ' ').slice(0, 120))
  const kickoff = await cdp.evaluate(`(() => ({ on: !!document.querySelector('[data-launch-kickoff] input')?.checked, text: document.querySelector('.kickoff-text')?.innerText ?? '' }))()`)
  check('…with “Start right away” ticked, showing the exact first message', kickoff.on && kickoff.text.includes('update_task'), kickoff.text)
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] footer .btn-primary')][0].click(); true`)
  const reportFile = join(root, 'fake-report.json')
  const report = await until(async () => {
    try {
      const r = existsSync(reportFile) ? JSON.parse(readFileSync(reportFile, 'utf8')) : null
      return r?.args?.[0] === '--session-id' ? r : null
    } catch {
      return null
    }
  })
  const tail = report?.args.slice(-2) ?? []
  check('…and Claude gets it as its first message, after `--` (no option can take it)', tail[0] === '--' && `“${tail[1]}”` === kickoff.text, JSON.stringify(tail).slice(0, 160))
  const args = report?.args ?? []
  const pluginDirs = args.flatMap((a, i) => (a === '--plugin-dir' ? [args[i + 1]] : []))
  const skillDir = pluginDirs.find((d) => existsSync(join(d, 'skills', 'learn-project', 'SKILL.md')))
  check('…with NEXUS’s built-in learn-project skill for this session (--plugin-dir)', !!skillDir && /[\\/]nexus$/.test(skillDir), JSON.stringify(pluginDirs).slice(0, 160))
  const system = args[args.indexOf('--append-system-prompt') + 1] ?? ''
  check('…and the Quick rules in its system prompt (no task text, no notes)', system.includes('LEARN-2 Work alone') && system.includes('open at most 20 files') && !system.includes('Learn App') && !system.includes('mono/packages'), system.slice(-160))
  const addDir = args.indexOf('--add-dir')
  check('…and the added folder as --add-dir (before the message’s `--`), with rule LEARN-8', addDir > 0 && args[addDir + 1] === mono && addDir < args.indexOf('--') && system.includes('LEARN-8'), JSON.stringify(args.slice(addDir, addDir + 2)))
  const learnTaskId = (await inv('project_learn_map', { workspaceId: ws.id, projectId: proj.id })).task?.id
  const learnSession = (await inv('session_list', { filter: null })).find((x) => x.taskId === learnTaskId)
  check('…recorded with the session (depth, skill, folders)', learnSession?.integrations?.learn?.depth === 'quick' && learnSession?.integrations?.learn?.skill === 'nexus:learn-project' && learnSession?.integrations?.learn?.folders?.[0] === mono, JSON.stringify(learnSession?.integrations?.learn ?? null))
  const taskBody = (await inv('knowledge_read', { workspaceId: ws.id, id: learnTaskId })).body
  check('…and the notes are in the learning task, before NEXUS’s map', taskBody.indexOf('## Notes from the person') > 0 && taskBody.indexOf('mono/packages') > 0 && taskBody.indexOf('mono/packages') < taskBody.indexOf("## NEXUS's map"), taskBody.slice(0, 120))
  await click('[data-project-tab="learn"]')

  // Back returns where you came from: project → Learn → Memory proposals (Knowledge) → Back → the project's Learn tab.
  await cdp.waitFor(`!!document.querySelector('[data-learn-session]')`, 10000)
  await cdp.evaluate(`[...document.querySelectorAll('[data-learn-session] button')].find((b) => b.innerText.includes('Memory proposals'))?.click(); true`)
  const inKnowledge = await until(async () => ((await cdp.evaluate('location.hash')).includes('knowledge') ? true : null))
  await click('[data-nav-back]')
  const backAt = await until(async () => {
    const h = await cdp.evaluate('location.hash')
    return h.includes(`/projects/${proj.id}`) && h.includes('tab=learn') ? h : null
  })
  check('Back returns from Memory proposals to the project’s Learn tab', !!inKnowledge && !!backAt, backAt ?? '')
  const version = await textOf('[data-app-version]')
  check('the top bar shows the NEXUS version', /^v\d+\.\d+\.\d+/.test(version.trim()), version)

  // NEXUS tools: the built-in skill (as Claude reads it), the MCP server's tools as `nexus mcp` lists them, the hooks.
  await cdp.evaluate(`location.hash = '#/tools'; true`)
  const tools = await cdp.waitFor(`document.querySelectorAll('[data-tools-mcp] [data-tool]').length || null`, 20000).catch(() => 0)
  check('NEXUS tools lists the MCP server’s 24 tools and the hooks', tools === 24 && (await textOf('[data-tools-hooks]')).includes('SessionStart'), `${tools} tools`)
  await cdp.evaluate(`document.querySelector('[data-skill="nexus:learn-project"] .btn').click(), true`)
  const skillBody = await cdp.waitFor(`document.querySelector('[data-skill-body]')?.innerText`, 10000).catch(() => '')
  check('…and the built-in nexus:learn-project skill, exactly as Claude reads it', skillBody.includes('Learn a project for NEXUS') && skillBody.includes('Stop and report'), skillBody.slice(0, 60))
  await cdp.evaluate(`[...document.querySelectorAll('[role=dialog] button')].find((b) => b.getAttribute('aria-label') === 'Close')?.click(); true`)

  // Skills (ADR-0046): every skill in one list; a workspace skill and a project skill written and edited in NEXUS.
  await cdp.evaluate(`location.hash = '#/skills'; true`)
  // Scoped to the Skills page: the NEXUS tools page (kept alive) also has [data-skill] elements.
  const listed = await cdp.waitFor(`(() => { const n = [...document.querySelectorAll('[data-skill-list] [data-skill]')].map((b) => b.dataset.skill); return n.length ? n : null })()`, 20000).catch(() => [])
  check('Skills lists the template’s workspace skills and NEXUS’s built-in skill', listed.includes('workspace:security-review') && listed.includes('nexus:learn-project'), listed.join(', ').slice(0, 160))
  const setField = (sel, value) => cdp.evaluate(`(() => { const el = document.querySelector(${js(sel)}); if (!el) return false; el.value = ${js(value)}; el.dispatchEvent(new Event('input')); return true })()`)
  await cdp.waitFor(`!!document.querySelector('[data-skill-new]')`, 10000)
  await click('[data-skill-new]')
  await cdp.waitFor(`!!document.querySelector('[data-new-name]')`, 10000)
  await setField('[data-new-name]', 'Release Notes')
  await setField('[data-new-description]', 'Draft release notes from merged changes: use when preparing a release.')
  const tidied = await cdp.evaluate(`document.querySelector('[data-new-name]').value`)
  await click('[data-new-create]')
  const created = await cdp.waitFor(`document.querySelector('[data-skill-detail] h2')?.innerText.includes('workspace:release-notes') ? document.querySelector('[data-skill-detail] h2').innerText : null`, 15000).catch(() => '')
  const skillFile = join(root, 'ws', 'skills', 'release-notes', 'SKILL.md')
  const written = existsSync(skillFile) ? readFileSync(skillFile, 'utf8') : ''
  check('New skill writes the workspace skill (name tidied, description quoted) and opens it, active', tidied === 'release-notes' && created.includes('active') && written.includes('name: release-notes') && written.includes('description: "Draft release notes from merged changes: use when preparing a release."'), `${tidied} | ${created} | ${written.split('\n').slice(0, 3).join(' / ')}`)
  const edited = written.replace('1. \n', '1. List the merged pull requests since the last tag.\n')
  const typedSkill = await setEditor('[data-skill-editor]', edited)
  await click('[data-skill-save]')
  const skillSaved = await until(async () => (readFileSync(skillFile, 'utf8').includes('List the merged pull requests') ? true : null), 10000)
  const ws1 = (await inv('skills_list', { workspaceId: ws.id })).find((s) => s.invokeAs === 'workspace:release-notes')
  check('…an edit saves to the file and keeps it approved for this workspace’s sessions', typedSkill && !!skillSaved && ws1?.status === 'approved', `${typedSkill} ${ws1?.status}`)

  await click('[data-skill-new]')
  await cdp.waitFor(`!!document.querySelector('[data-new-scope="project"]')`, 10000)
  await click('[data-new-scope="project"]')
  await setField('[data-new-name]', 'answer-module')
  await setField('[data-new-description]', 'How the answer module works. Use when changing src/main.ts.')
  await click('[data-new-create]')
  const projectSkill = join(repo, '.claude', 'skills', 'answer-module', 'SKILL.md')
  const inRepo = await until(async () => (existsSync(projectSkill) ? true : null), 10000)
  const projectEntry = await cdp.waitFor(`document.querySelector('[data-skill-list] [data-skill="answer-module"]')?.innerText`, 10000).catch(() => '')
  check('…a project skill goes into the project’s .claude/skills and is listed with its project', !!inRepo && projectEntry.includes('App') && projectEntry.includes('in repository'), projectEntry.replace(/\n/g, ' '))
  await click('[data-skill-list] [data-skill="nexus:learn-project"]')
  const ro = await cdp.waitFor(`document.querySelector('[data-skill-detail] h2')?.innerText.includes('nexus:learn-project') ? (document.querySelector('[data-skill-editor] .cm-content')?.getAttribute('aria-readonly') ?? 'none') : null`, 10000).catch(() => '')
  check('…built-in skills are shown read-only (no Save)', ro === 'true' && !(await cdp.evaluate(`!!document.querySelector('[data-skill-save]')`)), ro)

  await cdp.evaluate(`location.hash = '#/projects/${proj.id}?tab=learn'; true`)
  await cdp.waitFor(`!!document.querySelector('[data-learn-session]')`, 15000)
  const lm = await inv('project_learn_map', { workspaceId: ws.id, projectId: proj.id })
  const brief = lm.task ? (await inv('knowledge_read', { workspaceId: ws.id, id: lm.task.id })).body : ''
  check('…the task carries NEXUS\'s map and the drafting instructions (graph included)', brief.includes('draft_project_document') && brief.includes('GRAPH_REPORT.md') && brief.includes('do not change any file'), brief.slice(0, 100))

  for (const [topic, text] of [['overview', 'A tiny app that answers (verified in src/main.ts).'], ['architecture', 'One module, src/main.ts (inferred).']]) {
    await inv('knowledge_create', { workspaceId: ws.id, input: { kind: 'note', title: `App — ${topic} draft`, body: text, projectId: proj.id, labels: ['draft', topic] } })
  }
  await cdp.evaluate(`[...document.querySelectorAll('[data-learn-map] button')].find((b) => b.innerText.includes('Read again'))?.click(); true`)
  await cdp.waitFor(`!!document.querySelector('[data-draft="overview"]') && !!document.querySelector('[data-draft="architecture"]')`, 15000)
  await click('[data-draft="overview"] [data-draft-apply]')
  await until(async () => ((await cdp.evaluate(`!document.querySelector('[data-draft="overview"]')`)) ? true : null))
  const ctxDir = join(root, 'ws', 'project-context', 'app')
  const overview = readFileSync(join(ctxDir, 'overview.md'), 'utf8')
  check('Use as overview writes the draft into the project overview', overview.includes('A tiny app that answers') && overview.includes('accepted on'), overview.split('\n').slice(-3).join(' | '))
  await click('[data-draft="architecture"] [data-draft-discard]')
  const none = await until(async () => ((await textOf('[data-learn-drafts]')).includes('No drafts waiting') ? true : null))
  check('Set aside leaves the architecture untouched and the list empty', !!none && readFileSync(join(ctxDir, 'architecture.md'), 'utf8').includes('_Not written yet._'))
} catch (e) {
  failures++
  console.error(`E2E error: ${e.message}`)
} finally {
  cdp?.close()
  try {
    execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  } catch {}
}
if (failures !== 0) console.error(`\n--- app log ---\n${log.slice(-4000)}`)
console.log(failures === 0 ? '\nWORKBENCH OK' : `\nWORKBENCH FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
