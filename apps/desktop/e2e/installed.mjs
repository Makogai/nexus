// Installed-app verification (ADR-0033): install the real NSIS installer, check what an installed NEXUS needs without
// developer tools or PATH entries, then uninstall and check that user data stays.
//
//   node e2e/installed.mjs [installer.exe]                      production-configuration installer (default:
//                                                              target/debug/bundle/nsis/NEXUS_*_x64-setup.exe)
//   node e2e/installed.mjs --ui <e2e-installer.exe>             a *test* installer built with the e2e feature, to drive
//                                                              onboarding, a terminal and a synthetic profile in the
//                                                              installed app (never published)
//
// Refuses to run when NEXUS is already installed for this user. Uses temporary data folders (NEXUS_DATA_DIR) and a
// synthetic Claude profile backed by the fake CLI; never touches real profiles. Always uninstalls at the end.
// Windows only.
import { execFileSync, spawn, spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs'
import { createHash } from 'node:crypto'
import { homedir, tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect, waitForPort } from './cdp-lib.mjs'

if (process.platform !== 'win32') {
  console.error('installed: Windows only')
  process.exit(2)
}
const ui = process.argv.includes('--ui')
const argPath = process.argv.slice(2).find((a) => !a.startsWith('--'))
const bundleDir = fileURLToPath(new URL('../../../target/debug/bundle/nsis/', import.meta.url))
// Always an absolute Windows path: Tauri's NSIS installer looks for its options (/R relaunch, /NS no shortcut, /P, /UPDATE)
// in the whole command line *including its own path*, so `../target/release/bundle/nsis/…` would read as /R and /NS.
const found = argPath ?? (existsSync(bundleDir) ? join(bundleDir, readdirSync(bundleDir).find((f) => f.endsWith('-setup.exe')) ?? '') : '')
const installer = found ? resolve(found) : ''
if (!installer || !existsSync(installer)) {
  console.error('installed: build an installer first: npx tauri build --debug --config src-tauri/tauri.installer.conf.json')
  process.exit(2)
}
const fakeClaude = fileURLToPath(new URL('../../../target/debug/fake-claude.exe', import.meta.url))
const local = process.env.LOCALAPPDATA ?? join(homedir(), 'AppData', 'Local')
const defaultInstall = join(local, 'Programs', 'NEXUS')
const defaultData = join(local, 'NEXUS')
const UNINSTALL_KEY = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\NEXUS'
// Where the installer remembers the last install location (Tauri keeps it after uninstalling; the next install reuses
// that folder — for the --ui phase a deleted temporary folder).
const LOCATION_KEY = 'HKCU\\Software\\nexus\\NEXUS'
const LOCATION_PARENT = 'HKCU\\Software\\nexus'
const js = (v) => JSON.stringify(v)
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}
const regQuery = (key, value) => {
  const r = spawnSync('reg', ['query', key, ...(value ? ['/v', value] : [])], { encoding: 'utf8' })
  return r.status === 0 ? r.stdout : null
}
const walk = (d) => readdirSync(d).flatMap((e) => (statSync(join(d, e)).isDirectory() ? walk(join(d, e)).map((x) => join(e, x)) : [e]))
const sha256 = (p) => createHash('sha256').update(readFileSync(p)).digest('hex')

if (regQuery(UNINSTALL_KEY) || existsSync(join(defaultInstall, 'nexus-desktop.exe'))) {
  console.error('installed: NEXUS is already installed for this user — uninstall it first (this test never overwrites an installation)')
  process.exit(2)
}
if (regQuery(LOCATION_KEY)) {
  console.error(`installed: the installer remembers a previous install location (${LOCATION_KEY}) — a new install would reuse it.\n` +
    `If no NEXUS is installed, remove it with: reg delete "${LOCATION_PARENT}" /f`)
  process.exit(2)
}
const dataExistedBefore = existsSync(defaultData)
const root = mkdtempSync(join(tmpdir(), 'nexus-installed-'))
const installDir = ui ? join(root, 'install') : defaultInstall // the production phase checks the default location
const dataDir = join(root, 'data')

// ---- install ----
const args = ['/S', ...(ui ? [`/D=${installDir}`] : [])]
// An app the installer starts by itself (/R, an update) must not touch the real data folder.
const inst = spawnSync(installer, args, { stdio: 'ignore', timeout: 180_000, env: { ...process.env, NEXUS_DATA_DIR: join(root, 'installer-launched-data') } })
const stopInstalledApps = () =>
  spawnSync('powershell', ['-NoProfile', '-Command', `Get-Process nexus-desktop -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '${installDir.replace(/'/g, "''")}*' } | Stop-Process -Force`], { stdio: 'ignore' })
stopInstalledApps()
check('the installer runs silently and succeeds', inst.status === 0, `exit ${inst.status}`)
const desktopExe = join(installDir, 'nexus-desktop.exe')
const cliExe = join(installDir, 'nexus.exe')
let app = null
try {
  for (let i = 0; i < 40 && !existsSync(desktopExe); i++) await sleep(250)
  const files = existsSync(installDir) ? walk(installDir).sort() : []
  check('installed into its own folder, apart from the data folder', existsSync(desktopExe) && !installDir.toLowerCase().startsWith(defaultData.toLowerCase() + '\\'), installDir)
  check('the desktop program, the CLI (MCP server, hooks) and the uninstaller are installed', ['nexus-desktop.exe', 'nexus.exe', 'uninstall.exe'].every((f) => files.includes(f)), files.join(', '))
  const unexpected = files.filter((f) => !['nexus-desktop.exe', 'nexus.exe', 'uninstall.exe'].includes(f))
  check('nothing else is bundled (no test doubles, configuration, keys or databases)', unexpected.length === 0, unexpected.join(', ') || 'only the three programs')
  console.log(`      sha256 nexus-desktop.exe ${sha256(desktopExe).slice(0, 16)}… nexus.exe ${sha256(cliExe).slice(0, 16)}…`)
  if (!ui) {
    const reg = regQuery(UNINSTALL_KEY, 'InstallLocation') ?? ''
    check('Windows lists NEXUS under installed apps (per user)', reg.toLowerCase().includes(installDir.toLowerCase()), reg.trim().split('\n').pop()?.trim())
    const lnk = join(process.env.APPDATA ?? '', 'Microsoft', 'Windows', 'Start Menu', 'Programs', 'NEXUS.lnk')
    check('a Start Menu shortcut exists', existsSync(lnk))
    check('the data folder is not created by installing', existsSync(defaultData) === dataExistedBefore)
  }

  // ---- the installed CLI, without PATH ----
  const env = Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(CLAUDECODE|CLAUDE_CODE_|ANTHROPIC_|OTEL_|NEXUS_)/i.test(k) && k.toUpperCase() !== 'CLAUDE_CONFIG_DIR'))
  env.PATH = (env.PATH ?? env.Path ?? '').split(';').filter((p) => !/\\target\\|\\.cargo\\|nexus/i.test(p)).join(';')
  delete env.Path
  const cli = (...a) => execFileSync(cliExe, ['--data-dir', dataDir, ...a], { encoding: 'utf8', env })
  check('the installed CLI runs by its full path', /nexus/i.test(execFileSync(cliExe, ['--version'], { encoding: 'utf8', env })))

  // ---- self-check of the installed desktop (no window, read-only) ----
  const report = join(root, 'self-check.json')
  spawnSync(desktopExe, ['--self-check', report], { env: { ...env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9351' }, timeout: 60_000 })
  const sc = existsSync(report) ? JSON.parse(readFileSync(report, 'utf8')) : {}
  check('the installed desktop finds its CLI next to it and runs it', sc.cli?.found && sc.cli.path?.toLowerCase() === cliExe.toLowerCase() && /nexus/i.test(sc.cli.version ?? ''), js(sc.cli))
  if (ui) check('this is a test installer (debugging compiled in — never publish it)', sc.e2eWebviewDebugging === true)
  else check('webview debugging is not compiled in and the override was dropped (T18)', sc.e2eWebviewDebugging === false && sc.webviewArgumentsAfterHardening === null, js([sc.e2eWebviewDebugging, sc.webviewArgumentsAfterHardening]))
  check('the default data folder is the per-user NEXUS folder, outside the program folder', sc.dataDir?.toLowerCase() === defaultData.toLowerCase() && sc.dataInsideInstallDir === false, sc.dataDir)
  if (!ui) check('notifications are sent as NEXUS (installed identity, nexus:// activation)', sc.notifier?.aumid === 'dev.nexus.desktop' && sc.notifier.display === 'NEXUS' && sc.notifier.protocol === true, js(sc.notifier))

  // ---- a synthetic profile launched through the installed CLI: sessions get the installed tools ----
  const repo = join(root, 'repo')
  mkdirSync(repo)
  writeFileSync(join(repo, 'README.md'), '# probe\n')
  execFileSync('git', ['init', '-q', '-b', 'main'], { cwd: repo })
  cli('workspaces', 'create', join(root, 'ws'), '--name', 'Installed')
  cli('projects', 'add', repo)
  cli('profiles', 'add', '--name', 'Synthetic', '--create', '--config-dir', join(root, 'cfg'), '--executable', fakeClaude)
  const fakeReport = join(root, 'fake.json')
  execFileSync(cliExe, ['--data-dir', dataDir, 'launch', '--project', 'repo', '-q'], { env: { ...env, FAKE_CLAUDE_REPORT: fakeReport } })
  const fake = JSON.parse(readFileSync(fakeReport, 'utf8'))
  const mcpConfig = JSON.parse(readFileSync(fake.args[fake.args.indexOf('--mcp-config') + 1], 'utf8'))
  const settings = JSON.parse(readFileSync(fake.args[fake.args.indexOf('--settings') + 1], 'utf8'))
  const mcpCommand = Object.values(mcpConfig.mcpServers)[0]?.command
  const hookCommand = Object.values(settings.hooks)[0]?.[0]?.hooks?.[0]?.command
  check('Claude sessions get the installed CLI as MCP server and hook handler (full path)', mcpCommand?.toLowerCase() === cliExe.toLowerCase() && hookCommand?.toLowerCase() === cliExe.toLowerCase(), js([mcpCommand, hookCommand]))
  const sessionId = JSON.parse(cli('--json', 'sessions'))[0].id
  const mcp = spawnSync(cliExe, ['--data-dir', dataDir, 'mcp', '--session', sessionId], {
    input: [
      js({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'installed', version: '1' } } }),
      js({ jsonrpc: '2.0', method: 'notifications/initialized' }),
      js({ jsonrpc: '2.0', id: 2, method: 'tools/list' }),
    ].join('\n') + '\n',
    encoding: 'utf8',
    env,
    timeout: 30_000,
  })
  const tools = mcp.stdout.split('\n').filter(Boolean).map((l) => JSON.parse(l)).find((m) => m.id === 2)?.result?.tools ?? []
  check('the installed MCP server answers a client (initialize, tools/list)', tools.length === 21, `${tools.length} tools`)

  // ---- the installed desktop running ----
  const port = ui ? '9352' : '9351'
  app = spawn(desktopExe, [], { env: { ...env, NEXUS_DATA_DIR: dataDir, NEXUS_E2E_NOTIFIER: ui ? 'record' : '', WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`, FAKE_CLAUDE_REPORT: fakeReport }, stdio: 'ignore' })
  await sleep(ui ? 1000 : 8000)
  if (!ui) {
    let devtools = false
    try {
      await fetch(`http://127.0.0.1:${port}/json`)
      devtools = true
    } catch {
      devtools = false
    }
    check('the installed app runs without a DevTools endpoint (T18)', app.exitCode === null && !devtools)
    check('first launch opens the database and takes the daily automatic backup', existsSync(join(dataDir, 'nexus.db')) && existsSync(join(dataDir, 'backups')) && readdirSync(join(dataDir, 'backups')).some((b) => b.endsWith('-automatic')))

    // A notification while NEXUS runs: Windows files it under NEXUS; the click (nexus://) reaches the app.
    const hook = spawnSync(cliExe, ['--data-dir', dataDir, 'hook', '--session', sessionId, 'notification'], {
      input: js({ notification_type: 'permission_prompt', message: 'Claude needs your permission to use Bash' }),
      encoding: 'utf8',
      env,
    })
    // Windows' own notification history for the NEXUS sender: count and the text shown.
    const historyScript =
      "[Console]::OutputEncoding = [Text.Encoding]::UTF8; " +
      "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; " +
      "$h = [Windows.UI.Notifications.ToastNotificationManager]::History.GetHistory('dev.nexus.desktop'); " +
      "$t = @($h | ForEach-Object { $_.Content.GetElementsByTagName('text') | ForEach-Object { $_.InnerText } }); " +
      "@{ count = $h.Count; text = ($t -join ' | ') } | ConvertTo-Json -Compress"
    let history = { count: -1, text: '' }
    let raw = ''
    for (let i = 0; i < 30 && !(Number(history.count) >= 1); i++) {
      await sleep(1000)
      const ps = spawnSync('powershell', ['-NoProfile', '-Command', historyScript], { encoding: 'utf8' })
      raw = `${ps.status} ${ps.stdout.trim()} ${ps.stderr.trim().slice(0, 200)}`
      try {
        history = JSON.parse(ps.stdout.trim() || '{}')
      } catch {
        history = { count: -1, text: '' }
      }
    }
    check('a desktop notification is delivered under the NEXUS sender', hook.status === 0 && Number(history.count) >= 1, Number(history.count) >= 1 ? `toast history: ${history.count}` : raw)
    const shown = String(history.text ?? '')
    check(
      'the toast shows no machine paths or identifiers (data folder, project folder, session id)',
      shown.length > 0 && ![dataDir, repo, sessionId, homedir()].some((s) => shown.toLowerCase().includes(String(s).toLowerCase())),
      shown.slice(0, 120),
    )
    const display = regQuery('HKCU\\Software\\Classes\\AppUserModelId\\dev.nexus.desktop', 'DisplayName') ?? ''
    check('…registered with the display name NEXUS', /DisplayName\s+REG_\w+\s+NEXUS\s*$/m.test(display), display.trim().split('\n').pop()?.trim())
    const proto = regQuery('HKCU\\Software\\Classes\\nexus\\shell\\open\\command') ?? ''
    check('…and nexus:// opens the installed program', proto.toLowerCase().includes(desktopExe.toLowerCase()), proto.trim().split('\n').pop()?.trim())
    const list = JSON.parse(cli('--json', 'notifications', 'list', '--all')).notifications ?? []
    const note = list.find((n) => n.sessionId === sessionId)
    if (note) {
      spawnSync('cmd', ['/c', 'start', '', `nexus://notification/${note.id}`], { stdio: 'ignore' })
      let read = null
      for (let i = 0; i < 40 && !read; i++) {
        await sleep(500)
        read = (JSON.parse(cli('--json', 'notifications', 'list', '--all')).notifications ?? []).find((n) => n.id === note.id)?.readAt
      }
      check('a click (nexus:// activation) reaches the running app and opens the notification', !!read)
    }
  } else {
    // UI phase (test installer only): onboarding, terminal and a synthetic profile in the installed app.
    await waitForPort(port, 120_000)
    const cdp = await connect(port)
    const onboarding = await cdp.waitFor(`document.querySelector('.onboarding') || document.querySelector('.shell')`, 30000).then(() => cdp.evaluate(`!!document.querySelector('[data-onboarding-clone]') || !!document.querySelector('.shell')`)).catch(() => false)
    check('the installed app shows the first-run screen (or its shell)', onboarding)
    const inv = (cmd, a = {}) => cdp.evaluate(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(a)})`)
    const info = await inv('app_info')
    check('the installed app reports its CLI next to it', info.cliPath?.toLowerCase() === cliExe.toLowerCase(), info.cliPath)
    await cdp.evaluate(`location.reload(); true`)
    await cdp.waitFor(`document.querySelector('.shell')`, 30000)
    // A terminal as a person opens it: the terminal panel's "Open shell".
    await cdp.evaluate(`(() => { const b = [...document.querySelectorAll('button')].find((x) => x.innerText.trim() === 'Open shell'); b?.click(); return !!b })()`)
    await cdp.waitFor(`document.querySelector('.xterm-rows')`, 20000).catch(() => false)
    let running = null
    for (let i = 0; i < 40 && !running; i++) {
      running = (await inv('session_list', { filter: null })).find((x) => x.kind === 'shell' && x.running) ?? null
      if (!running) await sleep(250)
    }
    if (running) {
      await sleep(1500)
      await inv('session_write', { id: running.id, data: 'echo NEXUS-INSTALLED-TERMINAL\r' })
    }
    const term = await cdp.waitFor(`[...document.querySelectorAll('.xterm-rows')].some((r) => r.innerText.includes('NEXUS-INSTALLED-TERMINAL'))`, 20000).catch(() => false)
    check('a terminal runs in the installed app (open, type, see the output)', !!running && !!term, running ? '' : 'no shell session')
    const profiles = await inv('profile_list', { includeArchived: false }).catch(() => [])
    const prof = profiles.find?.((p) => p.name === 'Synthetic')
    writeFileSync(fakeReport, '')
    const s = await inv('session_start_claude', { args: { profileId: prof?.id, projectId: null, cwd: repo, workProfile: 'balanced', cols: 100, rows: 30 } }).catch((e) => ({ error: String(e) }))
    let r = null
    for (let i = 0; i < 40 && !r; i++) {
      await sleep(250)
      try {
        r = JSON.parse(readFileSync(fakeReport, 'utf8'))
      } catch {
        r = null
      }
    }
    const cfg = r ? JSON.parse(readFileSync(r.args[r.args.indexOf('--mcp-config') + 1], 'utf8')) : null
    check('a synthetic profile launches from the installed app with the installed MCP server', !!cfg && Object.values(cfg.mcpServers)[0]?.command?.toLowerCase() === cliExe.toLowerCase(), s.error ?? '')
    cdp.close()
  }
} catch (e) {
  failures++
  console.error(`installed: error: ${e.message}`)
} finally {
  if (app) spawnSync('taskkill', ['/PID', String(app.pid), '/T', '/F'], { stdio: 'ignore' })
  await sleep(1500)
  // ---- uninstall ----
  const uninstaller = join(installDir, 'uninstall.exe')
  if (existsSync(uninstaller)) spawnSync(uninstaller, ['/S'], { stdio: 'ignore', timeout: 120_000 })
  for (let i = 0; i < 120 && existsSync(desktopExe); i++) await sleep(500)
  check('uninstalling removes the programs', !existsSync(desktopExe) && !existsSync(cliExe))
  if (!ui) {
    check('…the Windows uninstall entry', !regQuery(UNINSTALL_KEY))
    check('…and what NEXUS registered for itself (sender, nexus://)', !regQuery('HKCU\\Software\\Classes\\AppUserModelId\\dev.nexus.desktop') && !regQuery('HKCU\\Software\\Classes\\nexus'))
  }
  check('user data stays: the database and its backups survive the uninstall', existsSync(join(dataDir, 'nexus.db')) && (ui || existsSync(join(dataDir, 'backups'))))
  check('the per-user data folder was not touched by installing or uninstalling', existsSync(defaultData) === dataExistedBefore)
  // Leave the machine as it was: the remembered install location did not exist before this run.
  if (regQuery(LOCATION_KEY)) spawnSync('reg', ['delete', LOCATION_PARENT, '/f'], { stdio: 'ignore' })
  check('no installer state is left behind', !regQuery(LOCATION_KEY) && !regQuery(UNINSTALL_KEY))
}
console.log(failures === 0 ? `\nINSTALLED ${ui ? 'UI ' : ''}OK` : `\nINSTALLED ${ui ? 'UI ' : ''}FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
