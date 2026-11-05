// Build the `nexus` CLI (also the MCP server and the hook handler) in the same profile as the desktop and place it
// where Tauri's `externalBin` expects it (`src-tauri/binaries/nexus-<target-triple>[.exe]`). The installer then puts
// it next to `nexus-desktop.exe`, which is where the desktop looks for it (ADR-0033). Run by the installer build
// (`tauri.installer.conf.json` → beforeBuildCommand); never needed for development or tests.
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const repo = join(here, '..', '..', '..')
const debug = process.env.TAURI_ENV_DEBUG === 'true'
const cargo = process.env.CARGO ?? 'cargo'
const host = execFileSync('rustc', ['-vV'], { encoding: 'utf8' }).match(/^host: (.+)$/m)?.[1]?.trim()
const triple = process.env.TAURI_ENV_TARGET_TRIPLE || host
if (!triple) throw new Error('cannot determine the target triple')
const cross = triple !== host
const args = ['build', '-p', 'nexus-cli', '--locked', ...(debug ? [] : ['--release']), ...(cross ? ['--target', triple] : [])]
console.log(`stage-sidecar: cargo ${args.join(' ')}`)
execFileSync(cargo, args, { cwd: repo, stdio: 'inherit' })
const exe = triple.includes('windows') ? '.exe' : ''
const built = join(repo, 'target', ...(cross ? [triple] : []), debug ? 'debug' : 'release', `nexus${exe}`)
if (!existsSync(built)) throw new Error(`the CLI was not built at ${built}`)
const dest = join(here, '..', 'src-tauri', 'binaries', `nexus-${triple}${exe}`)
mkdirSync(dirname(dest), { recursive: true })
copyFileSync(built, dest)
console.log(`stage-sidecar: ${built} -> ${dest}`)
