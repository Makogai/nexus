// Dev Environment (ADR-0051): tools grouped by category with what projects need; filters; install commands and
// version-manager hints are only typed into a terminal; "Check again" probes one tool; errors stay until dismissed.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { DevEnvironment, DevTool } from '../src/lib/types'

const api = vi.hoisted(() => ({ devEnvironment: vi.fn(), devEnvironmentCheck: vi.fn() }))
const typed = vi.hoisted(() => vi.fn())
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('../src/lib/terminal', () => ({ typeIntoTerminal: typed }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))
vi.mock('vue-router', () => ({ RouterLink: { props: ['to'], template: '<a :href="String(to)"><slot /></a>' } }))

import DevEnvironmentView from '../src/views/DevEnvironmentView.vue'

const tool = (over: Partial<DevTool>): DevTool => ({
  id: 'x', name: 'X', category: 'runtime', installUrl: 'https://example.org', path: null, version: null, copies: [], installCommand: null, note: null, neededBy: [], ...over,
})

const env = (tools: DevTool[]): DevEnvironment => ({ workspaceId: 'w1', platform: 'windows', probedAt: 1, tools, projectsWithoutEnvironment: ['far-away'] })

const node = tool({
  id: 'node', name: 'Node.js', path: 'C:\\nvm4w\\nodejs\\node.exe', version: '18.19.0', copies: ['C:\\nvm4w\\nodejs\\node.exe', 'C:\\Program Files\\nodejs\\node.exe'],
  neededBy: [
    { projectId: 'p1', project: 'shop', constraint: '20.11.1', source: '.nvmrc', status: 'mismatch', hint: { manager: 'nvm', command: ['nvm', 'install', '20.11.1'] } },
    { projectId: 'p2', project: 'blog', constraint: '>=18', source: 'package.json engines.node', status: 'ok', hint: null },
  ],
})
const git = tool({ id: 'git', name: 'Git', category: 'source-control', path: 'C:\\git.exe', version: '2.47.0', copies: ['C:\\git.exe'] })
const go = tool({ id: 'go', name: 'Go', installCommand: ['winget', 'install', '--id', 'GoLang.Go', '-e'] })
const make = tool({ id: 'make', name: 'GNU Make', category: 'build' })

beforeEach(() => {
  setActivePinia(createPinia())
  api.devEnvironment.mockReset()
  api.devEnvironmentCheck.mockReset()
  typed.mockReset()
  typed.mockResolvedValue({ id: 's1' })
})

describe('Dev Environment', () => {
  it('groups tools by category and shows versions, copies on PATH and what projects need', async () => {
    api.devEnvironment.mockResolvedValue(env([git, node, go, make]))
    const w = mount(DevEnvironmentView)
    await flushPromises()
    expect(api.devEnvironment).toHaveBeenCalledWith(false)
    expect(w.findAll('[data-devenv-category]').map((c) => c.attributes('data-devenv-category'))).toEqual(['source-control', 'runtime', 'build'])
    const n = w.find('[data-devenv-tool="node"]')
    expect(n.find('[data-devenv-version]').text()).toBe('18.19.0')
    expect(n.find('[data-devenv-copies]').text()).toContain('2 copies on PATH')
    expect(n.findAll('[data-devenv-need]').map((x) => x.attributes('data-devenv-need'))).toEqual(['mismatch', 'ok'])
    expect(n.text()).toContain('shop needs 20.11.1')
    expect(w.find('[data-devenv-summary]').text()).toContain('1 not as a project needs')
    expect(w.text()).toContain('Needs unknown for far-away')
    expect(w.find('[data-devenv-install="make"]').exists()).toBe(false)
  })

  it('types install commands and version-manager hints into a terminal, never runs them', async () => {
    api.devEnvironment.mockResolvedValue(env([node, go]))
    const w = mount(DevEnvironmentView)
    await flushPromises()
    await w.find('[data-devenv-install="go"]').trigger('click')
    await flushPromises()
    expect(typed).toHaveBeenLastCalledWith('winget install --id GoLang.Go -e', { projectId: null })
    await w.find('[data-devenv-hint="nvm"]').trigger('click')
    await flushPromises()
    expect(typed).toHaveBeenLastCalledWith('nvm install 20.11.1', { projectId: 'p1' })
  })

  it('filters by need, missing and installed, and finds by name', async () => {
    api.devEnvironment.mockResolvedValue(env([git, node, go, make]))
    const w = mount(DevEnvironmentView)
    await flushPromises()
    const ids = () => w.findAll('[data-devenv-tool]').map((t) => t.attributes('data-devenv-tool'))
    await w.find('[data-devenv-filter="needed"]').trigger('click')
    expect(ids()).toEqual(['node'])
    await w.find('[data-devenv-filter="missing"]').trigger('click')
    expect(ids()).toEqual(['node', 'go', 'make'])
    await w.find('[data-devenv-filter="installed"]').trigger('click')
    expect(ids()).toEqual(['git', 'node'])
    await w.find('[data-devenv-filter="all"]').trigger('click')
    await w.find('[data-devenv-search]').setValue('mak')
    expect(ids()).toEqual(['make'])
  })

  it('checks one tool again and refreshes everything on request', async () => {
    api.devEnvironment.mockResolvedValue(env([go]))
    api.devEnvironmentCheck.mockResolvedValue(env([{ ...go, path: 'C:\\Go\\bin\\go.exe', version: '1.23.2', installCommand: null }]))
    const w = mount(DevEnvironmentView)
    await flushPromises()
    await w.find('[data-devenv-check="go"]').trigger('click')
    await flushPromises()
    expect(api.devEnvironmentCheck).toHaveBeenCalledWith('go')
    expect(w.find('[data-devenv-tool="go"] [data-devenv-version]').text()).toBe('1.23.2')
    await w.find('[data-devenv-refresh]').trigger('click')
    await flushPromises()
    expect(api.devEnvironment).toHaveBeenLastCalledWith(true)
  })

  it('keeps a failure visible until it is dismissed', async () => {
    api.devEnvironment.mockRejectedValue('probe failed')
    const w = mount(DevEnvironmentView)
    await flushPromises()
    expect(w.find('[data-devenv-error]').text()).toContain('probe failed')
    await w.find('[data-devenv-error] button').trigger('click')
    expect(w.find('[data-devenv-error]').exists()).toBe(false)
  })
})
