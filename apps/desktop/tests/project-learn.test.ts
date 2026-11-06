// Learn tab (W4, ADR-0045): the repository a project lives in is offered as extra context, notes go into the learning
// task, and a graph built in a terminal is noticed without "Read again".
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { LearnGraph, ProjectMap, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  projectLearnMap: vi.fn(),
  projectLearnDrafts: vi.fn(),
  projectLearnStart: vi.fn(),
  projectLearnGraph: vi.fn(),
  projectLearnAddFolder: vi.fn(),
  projectLearnRemoveFolder: vi.fn(),
  projectLearnExcludeGraph: vi.fn(),
}))
const terminal = vi.hoisted(() => ({ typeIntoTerminal: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'invalid', message: String(e) }) }))
vi.mock('../src/lib/terminal', () => terminal)
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))

import ProjectLearn from '../src/components/ProjectLearn.vue'
import { useAppStore } from '../src/stores/app'
import { useWorkspaceStore } from '../src/stores/workspaces'

const graph = (over: Partial<LearnGraph> = {}): LearnGraph => ({
  installed: true,
  built: false,
  report: null,
  builtMs: null,
  ignored: true,
  command: 'graphify extract . --code-only; graphify cluster-only . --no-label',
  ...over,
})
const map = (over: Partial<ProjectMap> = {}): ProjectMap => ({
  projectId: 'p1',
  name: 'drivebay',
  files: 3,
  filesTruncated: false,
  languages: [],
  folders: [],
  docs: [],
  stacks: [],
  packageManagers: [],
  scripts: [],
  history: null,
  context: { overviewId: null, overviewGenerated: true, architectureId: null, architectureSummary: false, notes: 0, decisions: 0, openTasks: 0 },
  graph: graph(),
  task: null,
  depths: [{ id: 'standard', files: 50, overviewWords: 400, architectureWords: 700, memories: [3, 8], rules: 'LEARN-1 …' }],
  skill: 'nexus:learn-project',
  drafts: 0,
  extraFolders: [],
  surrounding: { path: 'D:\\code\\maiw', name: 'maiw' },
  ...over,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of [...Object.values(api), terminal.typeIntoTerminal]) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as Workspace
  api.projectLearnMap.mockResolvedValue(map())
  api.projectLearnDrafts.mockResolvedValue([])
})
afterEach(() => vi.useRealTimers())

describe('ProjectLearn', () => {
  it('offers the surrounding repository, adds it in one click and sends the notes with the start', async () => {
    const w = mount(ProjectLearn, { props: { projectId: 'p1' } })
    await flushPromises()
    expect(w.find('[data-learn-surrounding]').text()).toContain('maiw')
    api.projectLearnAddFolder.mockResolvedValue([{ path: 'D:\\code\\maiw', name: 'maiw', available: true }])
    api.projectLearnMap.mockResolvedValue(map({ extraFolders: [{ path: 'D:\\code\\maiw', name: 'maiw', available: true }] }))
    await w.find('[data-learn-add-surrounding]').trigger('click')
    await flushPromises()
    expect(api.projectLearnAddFolder).toHaveBeenCalledWith('p1', 'D:\\code\\maiw')
    expect(w.find('[data-learn-folder="maiw"]').exists()).toBe(true)
    expect(w.find('[data-learn-surrounding]').exists()).toBe(false)

    const openLaunch = vi.spyOn(useAppStore(), 'openLaunch').mockImplementation(() => {})
    api.projectLearnStart.mockResolvedValue({ id: 't1', title: 'Learn drivebay', status: 'ready', depth: 'standard', notes: 'One app of maiw.' })
    await w.find('[data-learn-notes]').setValue('One app of maiw.')
    await w.find('[data-learn-start]').trigger('click')
    await flushPromises()
    expect(api.projectLearnStart).toHaveBeenCalledWith('w1', 'p1', 'standard', 'One app of maiw.')
    expect(openLaunch).toHaveBeenCalled()
  })

  it('shows the notes the learning task already has and lets a folder be removed', async () => {
    api.projectLearnMap.mockResolvedValue(
      map({
        task: { id: 't1', title: 'Learn drivebay', status: 'ready', depth: 'standard', notes: 'Shared code: maiw/packages.' },
        extraFolders: [{ path: 'D:\\gone', name: 'gone', available: false }],
        surrounding: null,
      }),
    )
    const w = mount(ProjectLearn, { props: { projectId: 'p1' } })
    await flushPromises()
    expect((w.find('[data-learn-notes]').element as HTMLTextAreaElement).value).toBe('Shared code: maiw/packages.')
    expect(w.find('[data-learn-folder="gone"]').text()).toContain('missing')
    api.projectLearnRemoveFolder.mockResolvedValue([])
    await w.find('[data-learn-folder="gone"] button').trigger('click')
    await flushPromises()
    expect(api.projectLearnRemoveFolder).toHaveBeenCalledWith('p1', 'D:\\gone')
  })

  it('watches for the graph after Build and reloads when the report appears', async () => {
    vi.useFakeTimers()
    terminal.typeIntoTerminal.mockResolvedValue({ id: 's1' })
    api.projectLearnExcludeGraph.mockResolvedValue(false)
    const w = mount(ProjectLearn, { props: { projectId: 'p1' } })
    await flushPromises()
    await w.find('[data-learn-graph-build]').trigger('click')
    await flushPromises()
    expect(terminal.typeIntoTerminal).toHaveBeenCalledWith(map().graph.command, { projectId: 'p1' })
    expect(w.find('[data-learn-graph-watching]').exists()).toBe(true)

    api.projectLearnGraph.mockResolvedValue(graph())
    await vi.advanceTimersByTimeAsync(4100)
    expect(api.projectLearnGraph).toHaveBeenCalledTimes(1)
    expect(api.projectLearnMap).toHaveBeenCalledTimes(1)

    const built = graph({ built: true, builtMs: 1, report: 'graphify-out/GRAPH_REPORT.md' })
    api.projectLearnGraph.mockResolvedValue(built)
    api.projectLearnMap.mockResolvedValue(map({ graph: built }))
    await vi.advanceTimersByTimeAsync(4100)
    await flushPromises()
    expect(api.projectLearnMap).toHaveBeenCalledTimes(2)
    expect(w.find('[data-learn-graph-ready]').exists()).toBe(true)
    expect(w.find('[data-learn-graph-watching]').exists()).toBe(false)
    await vi.advanceTimersByTimeAsync(10_000)
    expect(api.projectLearnGraph).toHaveBeenCalledTimes(2)
  })

  it('Check again says what it found', async () => {
    const toast = vi.spyOn(useAppStore(), 'toast')
    const w = mount(ProjectLearn, { props: { projectId: 'p1' } })
    await flushPromises()
    api.projectLearnGraph.mockResolvedValue(graph())
    await w.find('[data-learn-graph-check]').trigger('click')
    await flushPromises()
    expect(toast).toHaveBeenCalledWith('info', expect.stringContaining('No graph yet'))
  })
})
