// Workspace schema banner (ADR-0036): read-only reason, explained migration that runs only after confirming the plan.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  workspaceList: vi.fn(),
  workspaceActive: vi.fn(),
  workspaceMigrationPlan: vi.fn(),
  workspaceMigrate: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))

import WorkspaceBanner from '../src/components/WorkspaceBanner.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

const ws = (compat: Workspace['compat']): Workspace => ({
  id: 'w1', name: 'W', description: '', rootPath: 'C:\\w', createdAt: 0, lastOpenedAt: null, projectCount: 0, available: true, compat, template: null,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('WorkspaceBanner', () => {
  it('explains a read-only workspace and offers no migration', () => {
    useWorkspaceStore().active = ws({ schemaVersion: 7, writable: false, reason: 'This workspace uses schema 7 from a newer NEXUS', needsMigration: false })
    const w = mount(WorkspaceBanner)
    expect(w.find('[data-workspace-readonly]').text()).toContain('schema 7 from a newer NEXUS')
    expect(w.find('[data-workspace-migrate]').exists()).toBe(false)
  })

  it('shows the plan and migrates only after confirming', async () => {
    useWorkspaceStore().active = ws({ schemaVersion: 1, writable: true, reason: null, needsMigration: true })
    api.workspaceMigrationPlan.mockResolvedValue({
      workspaceId: 'w1', from: 1, to: 2,
      changes: [{ path: 'nexus-workspace.json', action: 'update', description: 'schema 1 → 2' }, { path: '.gitignore', action: 'append', description: 'adds projects/' }],
      notes: ['Nothing is moved or deleted'],
    })
    api.workspaceMigrate.mockResolvedValue({ plan: { workspaceId: 'w1', from: 1, to: 2, changes: [], notes: [] }, backupDir: 'C:\\data\\workspace-backups\\w1\\x' })
    api.workspaceList.mockResolvedValue([])
    api.workspaceActive.mockResolvedValue(ws({ schemaVersion: 2, writable: true, reason: null, needsMigration: false }))
    const w = mount(WorkspaceBanner, { attachTo: document.body })
    await w.find('[data-workspace-migrate-review]').trigger('click')
    await flushPromises()
    const changes = document.querySelector('[data-migration-changes]')?.textContent ?? ''
    expect(changes).toContain('nexus-workspace.json')
    expect(changes).toContain('.gitignore')
    expect(api.workspaceMigrate).not.toHaveBeenCalled()
    ;(document.querySelector('[data-migration-apply]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.workspaceMigrate).toHaveBeenCalledWith('w1')
    expect(w.find('[data-workspace-migrate]').exists()).toBe(false)
    w.unmount()
  })
})
