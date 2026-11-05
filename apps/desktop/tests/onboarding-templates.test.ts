// Onboarding: a new workspace starts from a template (ADR-0037) — Developer by default, or empty.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { TemplateSummary } from '../src/lib/types'

const api = vi.hoisted(() => ({
  templateList: vi.fn(),
  workspaceCreate: vi.fn(),
  workspaceList: vi.fn(),
  profileDetect: vi.fn(),
  projectList: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn().mockResolvedValue('C:\\work') }))
vi.mock('../src/router', () => ({ router: { push: vi.fn() } }))

import OnboardingView from '../src/views/OnboardingView.vue'

const t = (id: string, title: string, o: Partial<TemplateSummary> = {}): TemplateSummary => ({
  id, version: '1.0.0', title, summary: `${title} summary`, extends: null, features: [], highlights: [],
  constitutions: 1, stackRuleSets: 0, skills: 0, workflows: 0, integrations: 0, alwaysOnChars: 800, ...o,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  api.templateList.mockResolvedValue([t('minimal', 'Minimal'), t('developer', 'Developer', { skills: 5, workflows: 4 }), t('ai-engineering-pro', 'AI Engineering Pro', { stackRuleSets: 13, skills: 11, workflows: 7 })])
  api.profileDetect.mockResolvedValue([])
  api.workspaceList.mockResolvedValue([])
  api.projectList.mockResolvedValue([])
  api.workspaceCreate.mockImplementation(async (path: string, name: string) => ({ id: 'w', name, rootPath: path }))
})

async function openCreate() {
  const w = mount(OnboardingView, { attachTo: document.body })
  await flushPromises()
  const createChoice = w.findAll('button.choice').find((b) => b.text().includes('Create'))
  await createChoice!.trigger('click')
  await w.find('.input-row .btn').trigger('click') // Browse… → parent folder
  await flushPromises()
  return w
}

describe('Onboarding templates', () => {
  it('offers the templates with Developer preselected and creates the workspace from the chosen one', async () => {
    const w = await openCreate()
    expect(w.findAll('[data-template]').map((b) => b.attributes('data-template'))).toEqual(['minimal', 'developer', 'ai-engineering-pro', 'none'])
    expect(w.find('[data-template="developer"]').attributes('aria-checked')).toBe('true')
    expect(w.find('[data-template="ai-engineering-pro"]').text()).toContain('13 stack rule sets')
    await w.find('[data-template="ai-engineering-pro"]').trigger('click')
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(api.workspaceCreate).toHaveBeenCalledWith(expect.stringContaining('personal'), 'Personal', undefined, 'ai-engineering-pro')
    w.unmount()
  })

  it('can create an empty workspace', async () => {
    const w = await openCreate()
    await w.find('[data-template="none"]').trigger('click')
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(api.workspaceCreate).toHaveBeenCalledWith(expect.any(String), 'Personal', undefined, null)
    w.unmount()
  })
})
