// Interoperability check: the official MCP TypeScript SDK client talks to `nexus mcp` over stdio.
// Uses only synthetic fixtures (temp data dir, fake-claude profile).
//
//   cargo build -p nexus-cli -p fake-claude
//   node e2e/mcp-interop.mjs
import { execFileSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'

const ext = process.platform === 'win32' ? '.exe' : ''
const nexus = fileURLToPath(new URL(`../../../target/debug/nexus${ext}`, import.meta.url))
const fake = fileURLToPath(new URL(`../../../target/debug/fake-claude${ext}`, import.meta.url))
const root = mkdtempSync(join(tmpdir(), 'nexus-mcp-interop-'))
const data = join(root, 'data')
const repo = join(root, 'repo')
mkdirSync(repo)
writeFileSync(join(repo, 'package.json'), JSON.stringify({ dependencies: { phaser: '3' } }))
execFileSync('git', ['init', '-q', '-b', 'main'], { cwd: repo })

const cli = (...args) => execFileSync(nexus, ['--data-dir', data, ...args], { encoding: 'utf8' })
cli('workspaces', 'create', join(root, 'ws'), '--name', 'Interop')
cli('projects', 'add', repo)
cli('profiles', 'add', '--name', 'Fake', '--create', '--config-dir', join(root, 'cfg'), '--executable', fake)
cli('launch', '--project', 'repo', '--name', 'Interop session', '-q')
const session = JSON.parse(cli('--json', 'sessions'))[0]

let failures = 0
const check = (name, ok, detail = '') => {
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? `  — ${detail}` : ''}`)
  if (!ok) failures++
}

const client = new Client({ name: 'nexus-interop-check', version: '1.0.0' })
await client.connect(new StdioClientTransport({ command: nexus, args: ['--data-dir', data, 'mcp', '--session', session.id] }))
check('initialize handshake with the official SDK', client.getServerVersion()?.name === 'nexus', JSON.stringify(client.getServerVersion()))

const { tools } = await client.listTools()
check('tools/list parsed by the SDK', tools.length === 7, tools.map((t) => t.name).join(', '))
check('read-only tools are annotated', tools.filter((t) => t.annotations?.readOnlyHint).length === 6)

const cp = await client.callTool({ name: 'checkpoint', arguments: { kind: 'progress', summary: 'Player movement works' } })
check('checkpoint tool call', cp.isError === false)

const ctx = await client.callTool({ name: 'get_session_context', arguments: {} })
const sc = ctx.structuredContent
check('structured session context', sc?.session?.title === 'Interop session' && sc?.project?.stacks?.includes('phaser'), JSON.stringify(sc?.project?.stacks))
check('checkpoint visible in context', sc?.recentCheckpoints?.[0]?.summary === 'Player movement works')

const bad = await client.callTool({ name: 'get_project', arguments: { project: 'not-in-this-workspace' } })
check('unknown project is a tool error, not a crash', bad.isError === true)

await client.close()
console.log(failures === 0 ? '\nMCP INTEROP OK' : `\nMCP INTEROP FAILED (${failures})`)
process.exitCode = failures === 0 ? 0 : 1
