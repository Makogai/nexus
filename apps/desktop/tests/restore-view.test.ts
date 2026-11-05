// Restore on this computer (P8): ordered steps with their state, repairs behind a plan, what came with the workspace
// and what cannot be recovered.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { Restoration, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  workspaceRestoration: vi.fn(),
  workspaceRepairPlan: vi.fn(),
  workspaceRepair: vi.fn(),
  workspaceList: vi.fn(),
  workspaceActive: vi.fn(),
  projectList: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
const push = vi.fn()
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }))

import RestoreView from '../src/views/RestoreView.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

const restoration: Restoration = {
  workspaceId: 'w1', name: 'Portable', done: 2, total: 5,
  steps: [
    { id: 'schema', title: 'Workspace schema', status: 'done', summary: 'Schema 2', detail: [], repair: null, navigate: 'workspace' },
    { id: 'projects', title: 'Projects', status: 'todo', summary: '1 project(s) missing here', detail: ['shop — projects/shop'], repair: 'clone-missing', navigate: 'projects' },
    { id: 'profiles', title: 'Claude profiles', status: 'todo', summary: 'Add your Claude profiles', detail: [], repair: null, navigate: 'profiles' },
  ],
  restored: { constitutions: 27, rules: 0, workflows: 7, skills: 11, projects: 1, tasks: 1, decisions: 0, notes: 0, developmentRecords: 1, template: 'ai-engineering-pro 1.0.0' },
  notRecoverable: ['Claude conversations and transcripts — they stay in the other computer’s Claude profiles'],
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  push.mockReset()
  useWorkspaceStore().active = { id: 'w1' } as Workspace
  api.workspaceRestoration.mockResolvedValue(restoration)
  api.workspaceList.mockResolvedValue([])
  api.workspaceActive.mockResolvedValue({ id: 'w1' })
  api.projectList.mockResolvedValue([])
})

describe('Restore view', () => {
  it('shows the steps, what came along and what cannot, and repairs only after the plan', async () => {
    const w = mount(RestoreView, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-restore-step="schema"]').attributes('data-restore-status')).toBe('done')
    expect(w.find('[data-restore-restored]').text()).toContain('27 constitutions')
    expect(w.find('[data-restore-not-recoverable]').text()).toContain('transcripts')
    api.workspaceRepairPlan.mockResolvedValue({ id: 'clone-missing', title: 'Clone missing projects', changes: ['git clone https://example.test/shop.git → projects/shop'], notes: [] })
    api.workspaceRepair.mockResolvedValue({ id: 'clone-missing', message: 'cloned 1 project(s)' })
    await w.find('[data-restore-repair="clone-missing"]').trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-restore-plan]')?.textContent).toContain('projects/shop')
    expect(api.workspaceRepair).not.toHaveBeenCalled()
    ;(document.querySelector('[data-restore-apply]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.workspaceRepair).toHaveBeenCalledWith('w1', 'clone-missing')
    await w.find('[data-restore-open="profiles"]').trigger('click')
    expect(push).toHaveBeenCalledWith({ name: 'profiles' })
    w.unmount()
  })
})
