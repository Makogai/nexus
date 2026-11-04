// Context Manager (ADR-0028): wizard and inspector with real Pinia stores; IPC and router mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ContextAnalysis, ContextConfig, ContextReport, ContextSettings } from '../src/lib/types'

const api = vi.hoisted(() => ({
  contextGet: vi.fn(),
  contextSet: vi.fn(),
  contextPresets: vi.fn(),
  contextAnalyze: vi.fn(),
  contextDocs: vi.fn(),
  contextInspect: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'context', query: { project: 'p1' } }), useRouter: () => ({ push: vi.fn() }) }))

import ContextView from '../src/views/ContextView.vue'
import { useProfileStore } from '../src/stores/profiles'
import { useProjectStore } from '../src/stores/projects'

const BALANCED: ContextSettings = {
  alwaysRules: true, constitution: 'rules', rulesBudget: 1500, startupSummary: true, startupIndex: false, checkpointsAtStart: false,
  startupChars: 1800, projectDocs: true, observeInstructions: true, retrievalLimit: 10, retrievalChars: 20000,
}
const config = (over: Partial<ContextConfig> = {}): ContextConfig => ({ projectId: 'p1', strategy: 'balanced', settings: BALANCED, configured: false, updatedAt: null, ...over })
const ANALYSIS: ContextAnalysis = {
  projectId: 'p1', project: 'shop', docs: { files: 3, bytes: 4096, sample: ['README.md'], skipped: [] },
  instructions: { claudeMd: true, agentsMd: false, claudeDir: false },
  knowledge: { rules: 1, alwaysRules: 1, openTasks: 12, decisions: 4, notes: 2, specs: 0, workflows: 0, invalid: 0, conflicts: 0 },
  constitutionRules: 5, constitutionMandatory: 1,
  estimates: [
    { strategy: 'minimal', rulesChars: 200, startupCharsMax: 900, rulesInPrompt: 1, onDemand: 5 },
    { strategy: 'balanced', rulesChars: 900, startupCharsMax: 1800, rulesInPrompt: 5, onDemand: 1 },
    { strategy: 'deep', rulesChars: 1400, startupCharsMax: 3000, rulesInPrompt: 6, onDemand: 0 },
  ],
  recommendation: 'deep', reasons: ['12 open tasks'], configured: null,
}
const REPORT: ContextReport = {
  configured: config({ strategy: 'deep', configured: true }),
  predicted: null,
  observed: {
    session: { id: 's1', title: 'Checkout', startedAt: Date.now(), profileId: 'work' },
    instructionsObserved: true,
    instructions: [
      { path: '/code/shop/CLAUDE.md', memoryType: 'Project', loadReason: 'session_start', predicted: true },
      { path: '/code/shop/sub/CLAUDE.md', memoryType: 'Project', loadReason: 'nested_traversal', predicted: false },
    ],
    missing: ['/home/u/.claude/CLAUDE.md'],
    nexus: { rules: { included: ['TC-1'], pendingApproval: [], overBudget: [] } },
  },
  onDemand: { knowledge: ANALYSIS.knowledge, projectDocs: 3, constitutionRules: ['TC-3'], tools: ['search_knowledge', 'get_document'] },
  retrieved: [{ tool: 'search_knowledge', results: 2, chars: 800, ids: ['README.md'], at: Date.now() }],
  metrics: { sessions: 3, sessionsWithRetrieval: 1, retrievals: 1, chars: 800, empty: 0, byTool: [['search_knowledge', 1]], topDocuments: [['README.md', 1]] },
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useProjectStore().list = [{ id: 'p1', name: 'shop' }] as never
  useProfileStore().list = [{ id: 'work', name: 'Work' }] as never
  api.contextGet.mockResolvedValue(config())
  api.contextAnalyze.mockResolvedValue(ANALYSIS)
  api.contextDocs.mockResolvedValue([{ path: 'README.md', title: 'Shop', size: 100, modifiedMs: Date.now() }])
  api.contextPresets.mockResolvedValue([['balanced', BALANCED]])
  api.contextInspect.mockResolvedValue(REPORT)
})

describe('ContextView', () => {
  it('shows the analysis, recommends a strategy with reasons and applies the choice', async () => {
    api.contextSet.mockResolvedValue(config({ strategy: 'deep', configured: true }))
    const w = mount(ContextView, { attachTo: document.body })
    await flushPromises()
    expect(api.contextAnalyze).toHaveBeenCalledWith('p1')
    expect(w.find('[data-recommendation]').text()).toContain('Deep — 12 open tasks')
    expect(w.find('[data-not-configured]').exists()).toBe(true)
    expect(w.find('[data-strategy="minimal"]').text()).toContain('~200 chars of rules')
    await w.find('[data-use="deep"]').trigger('click')
    await flushPromises()
    expect(api.contextSet).toHaveBeenCalledWith('p1', 'deep', null)
    expect(w.find('[data-strategy="deep"]').text()).toContain('in use')
  })

  it('custom settings are sent as configured', async () => {
    api.contextSet.mockImplementation(async (_p, s, settings) => config({ strategy: s, settings, configured: true }))
    const w = mount(ContextView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-custom-open]').trigger('click')
    await w.find('[data-custom] input[type="checkbox"]').setValue(false) // always-on rules off
    await w.find('[data-use="custom"]').trigger('click')
    await flushPromises()
    const [, strategy, settings] = api.contextSet.mock.calls[0]!
    expect(strategy).toBe('custom')
    expect(settings).toMatchObject({ alwaysRules: false, rulesBudget: 1500 })
  })

  it('the inspector separates predicted, unexpected and missing files and lists retrievals', async () => {
    const w = mount(ContextView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-tab="inspector"]').trigger('click')
    await flushPromises()
    expect(api.contextInspect).toHaveBeenCalledWith('p1', 'work')
    const observed = w.find('[data-col="observed"]')
    expect(observed.findAll('[data-observed="predicted"]')).toHaveLength(1)
    expect(observed.find('[data-observed="unexpected"]').text()).toContain('sub/CLAUDE.md')
    expect(observed.find('[data-observed="missing"]').text()).toContain('predicted, not reported')
    expect(w.find('[data-col="retrieved"]').text()).toContain('search_knowledge')
    expect(w.find('[data-col="on-demand"]').text()).toContain('Constitution by id: TC-3')
  })
})
