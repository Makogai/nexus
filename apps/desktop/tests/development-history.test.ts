// Development History (P6): records with their sync state; a record opens into Verified / Observed / Reported panels.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { DevHistory, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({ projectHistory: vi.fn(), knowledgeRead: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))

import DevelopmentHistoryView from '../src/views/DevelopmentHistoryView.vue'
import { useWorkspaceStore } from '../src/stores/workspaces'

const history: DevHistory = {
  records: [
    { id: 'r1', title: 'shop — 2026-11-05 · Checkout', path: 'C:\\ws\\project-context\\shop\\development\\2026-11-05-abc.md', created: '2026-11-05T10:00:00Z', branch: 'main', commit: null, sync: 'local' },
  ],
  sessions: [{ id: 's1', title: 'Checkout', startedAt: Date.now() - 60_000, endedAt: Date.now(), state: 'exited', record: 'x' }],
  tasks: [['t1', 'Shipping costs', 'in-progress'], ['t2', 'Old', 'done']],
  decisions: [['Use the hosted payment page', 'accepted']],
  commits: [['0123456789abcdef', 'Add cart totals', Date.now()]],
  gates: [['test', 'passed', Date.now()]],
  blockers: [['Waiting for the API contract', Date.now()]],
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1' } as Workspace
  api.projectHistory.mockResolvedValue(history)
})

describe('Development history', () => {
  it('lists records with their sync state and opens them as labelled sections', async () => {
    api.knowledgeRead.mockResolvedValue({ id: 'r1', body: '_intro_\n\n## Session\n\n- one\n\n## Verified\n\n- 1 commit\n\n## Observed (Claude Code hooks)\n\n- Files edited\n\n## Reported by Claude (unverified)\n\n- decision: X\n' })
    const w = mount(DevelopmentHistoryView, { props: { id: 'p1' }, global: { stubs: { RouterLink: true } } })
    await flushPromises()
    expect(api.projectHistory).toHaveBeenCalledWith('w1', 'p1')
    expect(w.find('[data-record-sync="local"]').text()).toBe('not committed')
    expect(w.find('[data-history-tasks]').text()).toContain('Shipping costs')
    expect(w.find('[data-history-tasks]').text()).not.toContain('Old')
    expect(w.find('[data-history-blockers]').text()).toContain('Waiting for the API contract')
    await w.find('[data-record="r1"] .record-head').trigger('click')
    await flushPromises()
    const panels = w.findAll('[data-record-body] .section')
    expect(panels.map((p) => p.find('h3').text())).toEqual(['Session', 'Verified', 'Observed (Claude Code hooks)', 'Reported by Claude (unverified)'])
    expect(panels[1]!.classes()).toContain('verified')
    expect(panels[3]!.classes()).toContain('reported')
  })
})
