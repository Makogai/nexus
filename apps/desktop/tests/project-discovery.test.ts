// Project discovery (P4): missing projects are cloned only when selected; new repositories are registered only after
// a confirmation; clones of known projects are linked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ProjectScan, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  projectScan: vi.fn(),
  projectClone: vi.fn(),
  projectList: vi.fn(),
  projectRegister: vi.fn(),
  projectLocate: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))

import ProjectDiscovery from '../src/components/ProjectDiscovery.vue'
import { useAppStore } from '../src/stores/app'
import { useProjectStore } from '../src/stores/projects'
import { useWorkspaceStore } from '../src/stores/workspaces'

const ws = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as Workspace
const scan = (over: Partial<ProjectScan> = {}): ProjectScan => ({
  projectsDir: 'projects',
  found: [
    { path: 'C:\\ws\\projects\\billing', rel: 'projects/billing', name: 'billing', remoteUrl: 'https://example.test/billing.git', stacks: ['rust'], status: 'known', projectId: 'p2' },
    { path: 'C:\\ws\\projects\\scratch', rel: 'projects/scratch', name: 'scratch', remoteUrl: null, stacks: [], status: 'new', projectId: null },
    { path: 'C:\\ws\\projects\\shop', rel: 'projects/shop', name: 'shop', remoteUrl: null, stacks: ['vue'], status: 'registered', projectId: 'p0' },
  ],
  missing: [
    { id: 'p1', name: 'Invoices', remoteUrl: 'https://example.test/invoices.git', cloneTo: 'projects/invoices', canClone: true, reason: null },
    { id: 'p3', name: 'Notes', remoteUrl: null, cloneTo: null, canClone: false, reason: 'no remote is recorded — locate a copy instead' },
  ],
  external: [],
  ...over,
})

beforeEach(async () => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = ws
  api.projectScan.mockResolvedValue(scan())
  api.projectList.mockResolvedValue([])
  await useProjectStore().load('w1')
})

describe('ProjectDiscovery', () => {
  it('clones the selected missing projects into their suggested folders', async () => {
    api.projectClone.mockResolvedValue({ id: 'p1' })
    const w = mount(ProjectDiscovery)
    await flushPromises()
    expect(w.find('[data-missing="Notes"] input').attributes('disabled')).toBeDefined()
    expect(w.find('[data-missing="Notes"]').text()).toContain('locate a copy')
    await w.find('[data-discovery-clone]').trigger('click')
    await flushPromises()
    expect(api.projectClone).toHaveBeenCalledTimes(1)
    expect(api.projectClone).toHaveBeenCalledWith('w1', 'p1', 'projects/invoices')
  })

  it('registers a new repository only after the confirmation and links known clones', async () => {
    const app = useAppStore()
    const confirm = vi.spyOn(app, 'confirm').mockResolvedValueOnce(false).mockResolvedValueOnce(true)
    api.projectRegister.mockResolvedValue({ id: 'new' })
    api.projectLocate.mockResolvedValue({ id: 'p2' })
    const w = mount(ProjectDiscovery)
    await flushPromises()
    expect(w.find('[data-discovery-new]').text()).not.toContain('projects/shop')
    await w.find('[data-register="projects/scratch"]').trigger('click')
    await flushPromises()
    expect(confirm.mock.calls[0]?.[0]?.message).toContain('nothing in the repository is run')
    expect(api.projectRegister).not.toHaveBeenCalled()
    await w.find('[data-register="projects/scratch"]').trigger('click')
    await flushPromises()
    expect(api.projectRegister).toHaveBeenCalledWith('w1', 'C:\\ws\\projects\\scratch', undefined)
    await w.find('[data-link="projects/billing"]').trigger('click')
    await flushPromises()
    expect(api.projectLocate).toHaveBeenCalledWith('p2', 'C:\\ws\\projects\\billing')
  })

  it('stays hidden when there is nothing to do', async () => {
    api.projectScan.mockResolvedValue(scan({ found: [], missing: [] }))
    const w = mount(ProjectDiscovery)
    await flushPromises()
    expect(w.find('[data-discovery]').exists()).toBe(false)
  })
})
