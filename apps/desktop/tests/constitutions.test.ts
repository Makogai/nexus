// Constitution Manager (ADR-0027): real components + Pinia; IPC and router mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ConstitutionStructure, ConstitutionView } from '../src/lib/types'
import { counts, matches, move, newArticle } from '../src/lib/constitution'

const api = vi.hoisted(() => ({
  constitutionList: vi.fn(),
  constitutionGet: vi.fn(),
  constitutionSave: vi.fn(),
  constitutionSaveRaw: vi.fn(),
  constitutionEffective: vi.fn(),
  constitutionSearch: vi.fn(),
  constitutionHistory: vi.fn(),
  knowledgeApprove: vi.fn(),
  knowledgeOverview: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({
  api,
  toIpcError: (e: unknown) => ({ code: (e as { code?: string })?.code ?? 'internal', message: String((e as { message?: string })?.message ?? e) }),
}))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'constitutions', query: {} }), useRouter: () => ({ push: vi.fn() }) }))

import ConstitutionsView from '../src/views/ConstitutionsView.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

const STRUCTURE: ConstitutionStructure = {
  preamble: 'How we work.',
  articles: [
    {
      title: 'Security',
      text: '',
      sections: [
        {
          title: '',
          text: '',
          rules: [
            { id: 'TC-1', title: 'Never commit credentials', enforcement: 'mandatory', enabled: true, overrides: null, text: 'Use the secret store.' },
            { id: 'TC-2', title: 'Explain trade-offs', enforcement: 'guideline', enabled: true, overrides: null, text: 'Briefly.' },
          ],
        },
      ],
    },
  ],
}

function view(over: Partial<ConstitutionView> = {}): ConstitutionView {
  return {
    id: 'c1', title: 'Team charter', scope: 'workspace', stacks: [], projectId: null, taskId: null, workflowId: null, applies: 'always',
    status: 'active', prefix: 'TC', approved: true, rules: 2, mandatory: 1, errors: 0, warnings: 0, relativePath: 'rules/c1.md', hash: 'h1',
    updated: '2026-11-04T10:00:00Z', structure: JSON.parse(JSON.stringify(STRUCTURE)), problems: [], body: '## Security\n', appliesTo: [{ id: 'p1', name: 'shop' }],
    otherRules: [], ...over,
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1', name: 'W' } as never
  api.knowledgeOverview.mockResolvedValue({ items: [] })
  api.constitutionSearch.mockResolvedValue([])
})

describe('constitution helpers', () => {
  it('moves, counts and filters', () => {
    const list = [1, 2, 3]
    expect(move(list, 0, -1)).toBe(false)
    expect(move(list, 0, 1) && list).toEqual([2, 1, 3])
    const s: ConstitutionStructure = { preamble: '', articles: [...STRUCTURE.articles, newArticle()] }
    expect(counts(s)).toMatchObject({ total: 3, mandatory: 1, rule: 1, guideline: 1 })
    expect(matches(STRUCTURE.articles[0]!.sections[0]!.rules[0]!, 'secret')).toBe(true)
    expect(matches(STRUCTURE.articles[0]!.sections[0]!.rules[0]!, 'tabs')).toBe(false)
  })
})

describe('ConstitutionsView', () => {
  it('flags a version changed outside NEXUS and approves only that version', async () => {
    api.constitutionList.mockResolvedValue([view({ approved: false })])
    api.constitutionGet.mockResolvedValue(view({ approved: false }))
    api.knowledgeApprove.mockResolvedValue({})
    const w = mount(ConstitutionsView, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-pending-banner]').text()).toContain('1 constitution(s) changed outside NEXUS')
    expect(w.find('[data-constitution="Team charter"]').text()).toContain('needs approval')
    await w.find('[data-approve]').trigger('click')
    await flushPromises()
    expect(api.knowledgeApprove).toHaveBeenCalledWith('w1', 'c1')
  })

  it('edits a rule in the tree and saves the structure with the expected hash', async () => {
    api.constitutionList.mockResolvedValue([view()])
    api.constitutionGet.mockResolvedValue(view())
    api.constitutionSave.mockImplementation(async (_w, _id, structure) => view({ structure, hash: 'h2' }))
    const w = mount(ConstitutionsView, { attachTo: document.body })
    await flushPromises()
    expect(w.text()).toContain('1 mandatory, 0 rules, 1 guidelines')
    expect(w.find('[data-save]').attributes('disabled')).toBeDefined()
    const title = w.find('[data-rule="TC-2"] input[aria-label="Rule title"]')
    await title.setValue('Explain trade-offs in one sentence')
    await w.find('[data-rule="TC-2"] select[aria-label="Enforcement"]').setValue('rule')
    await w.find('[data-save]').trigger('click')
    await flushPromises()
    const [, , saved, hash] = api.constitutionSave.mock.calls[0]!
    expect(hash).toBe('h1')
    expect(saved.articles[0].sections[0].rules[1]).toMatchObject({ id: 'TC-2', title: 'Explain trade-offs in one sentence', enforcement: 'rule' })
  })

  it('saves raw Markdown in text mode and shows effective rules with conflicts', async () => {
    api.constitutionList.mockResolvedValue([view()])
    api.constitutionGet.mockResolvedValue(view())
    api.constitutionSaveRaw.mockResolvedValue(view({ hash: 'h3' }))
    api.constitutionEffective.mockResolvedValue({
      rules: [{ id: 'TC-1', title: 'Never commit credentials', enforcement: 'mandatory', text: 'Use the secret store.', article: 'Security', section: '', docId: 'c1', docTitle: 'Team charter', scope: 'workspace', replaces: null }],
      overridden: [],
      conflicts: [{ kind: 'mandatory-override', message: '“Repo rules” tries to disable the mandatory policy `TC-1`', ruleId: 'TC-1', docs: ['c1', 'c2'] }],
      pending: [],
      documents: [],
    })
    const w = mount(ConstitutionsView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-mode="markdown"]').trigger('click')
    await w.find('[data-raw]').setValue('## Security\n\n#### New one\n')
    await w.find('[data-save]').trigger('click')
    await flushPromises()
    expect(api.constitutionSaveRaw).toHaveBeenCalledWith('w1', 'c1', '## Security\n\n#### New one\n', 'h1')
    await w.find('[data-tab="effective"]').trigger('click')
    await flushPromises()
    expect(api.constitutionEffective).toHaveBeenCalledWith('w1', null, null, false)
    expect(w.find('[data-effective-rule="TC-1"]').text()).toContain('Mandatory policy')
    expect(w.find('[data-conflict]').text()).toContain('mandatory policy `TC-1`')
  })
})
