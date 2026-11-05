// Workspace view (P3): Doctor findings with recommendations; repairs and templates only after their plan is shown.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { DoctorReport, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  workspaceDoctor: vi.fn(),
  workspaceRepairPlan: vi.fn(),
  workspaceRepair: vi.fn(),
  templateList: vi.fn(),
  templatePlan: vi.fn(),
  templateApply: vi.fn(),
  workspaceList: vi.fn(),
  workspaceActive: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
const push = vi.fn()
vi.mock('vue-router', () => ({ useRouter: () => ({ push }) }))

import WorkspaceView from '../src/views/WorkspaceView.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

const ws: Workspace = {
  id: 'w1', name: 'Personal', description: '', rootPath: 'C:\\ws', createdAt: 0, lastOpenedAt: null, projectCount: 1, available: true,
  compat: { schemaVersion: 2, writable: true, reason: null, needsMigration: false }, template: null,
}
const report = (over: Partial<DoctorReport> = {}): DoctorReport => ({
  workspaceId: 'w1',
  checkedAt: Date.now(),
  counts: { ok: 1, info: 1, warn: 1, error: 0 },
  findings: [
    { check: 'workspace-skills', area: 'skills', status: 'warn', summary: '3 workspace skill(s) do not reach Claude on this machine yet', detail: [], recommendation: 'Register the workspace skills, then review them.', repair: { id: 'register-workspace-skills', title: 'Register the workspace skills for review' }, navigate: 'kits' },
    { check: 'template', area: 'workspace', status: 'info', summary: 'no starter template', detail: [], recommendation: null, repair: null, navigate: 'workspace' },
    { check: 'schema', area: 'workspace', status: 'ok', summary: 'workspace schema 2', detail: [], recommendation: null, repair: null, navigate: null },
  ],
  ...over,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  push.mockReset()
  useWorkspaceStore().active = ws
  api.workspaceDoctor.mockResolvedValue(report())
  api.templateList.mockResolvedValue([
    { id: 'ai-engineering-pro', version: '1.0.0', title: 'AI Engineering Pro', summary: 'library', extends: 'developer', features: [], highlights: [], constitutions: 14, stackRuleSets: 13, skills: 11, workflows: 7, integrations: 4, alwaysOnChars: 950 },
  ])
  api.workspaceList.mockResolvedValue([ws])
  api.workspaceActive.mockResolvedValue(ws)
})

describe('Workspace view', () => {
  it('shows problems first with their recommendation and applies a repair only after its plan', async () => {
    const w = mount(WorkspaceView, { attachTo: document.body })
    await flushPromises()
    const finding = w.find('[data-finding="workspace-skills"]')
    expect(finding.text()).toContain('Register the workspace skills, then review them.')
    expect(w.find('[data-finding-ok="schema"]').exists()).toBe(true)
    api.workspaceRepairPlan.mockResolvedValue({ id: 'register-workspace-skills', title: 'Register', changes: ['register C:\\ws/skills as the workspace AI Kit (not approved yet)'], notes: ['Review it in AI Kits'] })
    api.workspaceRepair.mockResolvedValue({ id: 'register-workspace-skills', message: 'registered' })
    await finding.find('[data-repair="register-workspace-skills"]').trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-repair-changes]')?.textContent).toContain('not approved yet')
    expect(api.workspaceRepair).not.toHaveBeenCalled()
    ;(document.querySelector('[data-repair-apply]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.workspaceRepair).toHaveBeenCalledWith('w1', 'register-workspace-skills')
    expect(api.workspaceDoctor).toHaveBeenCalledTimes(2)
    await finding.find('.btn-ghost').trigger('click')
    expect(push).toHaveBeenCalledWith({ name: 'kits' })
    w.unmount()
  })

  it('installs a template after showing the per-file plan', async () => {
    api.templatePlan.mockResolvedValue({
      template: { id: 'ai-engineering-pro', version: '1.0.0' }, installed: null,
      items: [{ path: 'constitutions/core.md', kind: 'constitution', template: 'minimal', action: 'add' }, { path: 'constitutions/testing.md', kind: 'constitution', template: 'developer', action: 'skip-existing' }],
      notes: ['Files you already have at a template path are never overwritten.'],
    })
    api.templateApply.mockResolvedValue({ plan: { template: { id: 'ai-engineering-pro', version: '1.0.0' } }, written: ['constitutions/core.md'], savedForReview: [], kitId: null })
    const w = mount(WorkspaceView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-template-review="ai-engineering-pro"]').trigger('click')
    await flushPromises()
    const plan = document.querySelector('[data-template-plan]')?.textContent ?? ''
    expect(plan).toContain('1 new files')
    expect(plan).toContain('never overwritten')
    ;(document.querySelector('[data-template-apply]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.templateApply).toHaveBeenCalledWith('w1', 'ai-engineering-pro')
    w.unmount()
  })
})
