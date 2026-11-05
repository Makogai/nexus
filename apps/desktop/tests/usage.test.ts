// Usage dashboard (ADR-0030): helpers, the trend chart and the view with real Pinia stores; IPC and router mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { SessionUsage, UsageGroup, UsageReport, UsageTokens, UsageView as View } from '../src/lib/types'
import { buckets, compact, costText, niceMax, percent } from '../src/lib/usage'

const api = vi.hoisted(() => ({ usageReport: vi.fn(), usageSessions: vi.fn(), usageScan: vi.fn(), uiPreferencesSet: vi.fn() }))
const replace = vi.hoisted(() => vi.fn())
vi.mock('../src/lib/ipc', () => ({ api, USAGE_EVENT: 'nexus://usage', SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'usage', query: {} }), useRouter: () => ({ push: vi.fn(), replace }) }))

import UsageTrendChart from '../src/components/UsageTrendChart.vue'
import UsageView from '../src/views/UsageView.vue'
import { useProjectStore } from '../src/stores/projects'

const T = (o: Partial<UsageTokens> = {}): UsageTokens => ({ requests: 0, input: 0, output: 0, cacheWrite: 0, cacheRead: 0, webSearches: 0, subagentRequests: 0, ...o })
const group = (key: string, label: string, o: Partial<UsageGroup> = {}): UsageGroup => ({
  key,
  label,
  sessions: 1,
  tokens: T({ requests: 2, input: 100, output: 50, cacheRead: 900 }),
  efficiency: { tokensPerSession: 1050, contextPerRequest: 500, outputPerRequest: 25, cacheHit: 0.9 },
  cost: { usd: 0.42, incomplete: false, withheldSubscription: 0, withheldUnknown: 0 },
  contextEstimate: { sessions: 1, tokensPerSession: 120, shareOfContext: 0.24 },
  ...o,
})
function report(o: Partial<UsageReport> = {}): UsageReport {
  const today = new Date()
  const day = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-${String(today.getDate()).padStart(2, '0')}`
  return {
    totals: T({ requests: 2, input: 100, output: 50, cacheRead: 900, subagentRequests: 1 }),
    sessions: 1,
    efficiency: { tokensPerSession: 1050, contextPerRequest: 500, outputPerRequest: 25, cacheHit: 0.9 },
    cost: { usd: 0.42, incomplete: false, withheldSubscription: 0, withheldUnknown: 0 },
    contextEstimate: { sessions: 1, tokensPerSession: 120, shareOfContext: 0.24 },
    coverage: { sessions: 3, measured: 1, noRequests: 1, pending: 0, missing: 1, unreadable: 0, outsideNexusRequests: 0 },
    daily: [{ day, tokens: T({ requests: 2, input: 100, output: 50, cacheRead: 900 }) }],
    by: 'project',
    groups: [group('p1', 'shop')],
    claudeVersions: ['2.1.311'],
    formatVerified: true,
    tokenProvenance: 'parsed',
    ...o,
  }
}
const session = (o: Partial<SessionUsage>): SessionUsage => ({
  sessionId: 's1', title: 'Checkout', startedAt: Date.now(), endedAt: null, live: false, projectId: 'p1', project: 'shop', profileId: 'w', profile: 'Work',
  taskId: null, task: null, workflowRunId: null, workflow: null, strategy: 'balanced', workProfile: 'balanced', status: 'measured', detail: null,
  tokens: T({ requests: 2, input: 100, output: 50, cacheRead: 900 }), cost: { usd: 0.42, incomplete: false, withheldSubscription: 0, withheldUnknown: 0 }, contextEstimate: 120, ...o,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  replace.mockReset()
})

describe('usage helpers', () => {
  it('formats measured numbers, estimates and unavailable values', () => {
    expect([compact(950), compact(1_234), compact(3_400_000), compact(null)]).toEqual(['950', '1.23k', '3.4M', '–'])
    const c = { withheldSubscription: 0, withheldUnknown: 0 }
    expect(costText({ usd: 1.234, incomplete: false, ...c })).toBe('≈ $1.23')
    expect(costText({ usd: 0.001, incomplete: true, ...c })).toBe('≈ < $0.01 (incomplete)')
    expect(costText({ usd: null, incomplete: false, ...c })).toBe('unavailable')
    expect(costText({ usd: null, incomplete: false, withheldSubscription: 2, withheldUnknown: 0 })).toBe('not shown (subscription)')
    expect(costText({ usd: null, incomplete: false, withheldSubscription: 0, withheldUnknown: 1 })).toBe('billing unknown')
    expect([percent(0.825), percent(null)]).toEqual(['83%', '–'])
    expect([niceMax(0), niceMax(7), niceMax(180), niceMax(2_600)]).toEqual([1, 10, 200, 5_000])
  })
  it('fills missing days and uses weeks for long periods', () => {
    const now = new Date(2026, 10, 5, 12).getTime()
    const b = buckets([{ day: '2026-11-04', tokens: T({ input: 5 }) }], 7, now)
    expect(b.map((x) => x.day)).toEqual(['2026-10-30', '2026-10-31', '2026-11-01', '2026-11-02', '2026-11-03', '2026-11-04', '2026-11-05'])
    expect(b.find((x) => x.day === '2026-11-04')!.tokens.input).toBe(5)
    const long = buckets([{ day: '2026-06-01', tokens: T({ input: 3 }) }, { day: '2026-06-02', tokens: T({ input: 4 }) }], 180, now)
    expect(long.every((x) => x.days <= 7) && long.length <= 27).toBe(true)
    expect(long.reduce((n, x) => n + x.tokens.input, 0)).toBe(7)
  })
})

describe('UsageTrendChart', () => {
  it('stacks token types per day with a legend, a tooltip per column and a table view', async () => {
    const w = mount(UsageTrendChart, { props: { buckets: [{ day: '2026-11-04', label: '4 Nov', days: 1, tokens: T({ requests: 3, input: 10, cacheWrite: 20, cacheRead: 30, output: 40 }) }, { day: '2026-11-05', label: '5 Nov', days: 1, tokens: T() }] } })
    expect(w.findAll('.legend li').map((l) => l.text())).toEqual(['Input', 'Cache write', 'Cache read', 'Output'])
    const col = w.find('[data-bucket="2026-11-04"]')
    expect(col.attributes('data-total')).toBe('100')
    expect(col.findAll('rect:not(.hit), path')).toHaveLength(4)
    expect(col.attributes('aria-label')).toContain('Cache read 30')
    await col.trigger('pointerenter')
    const tip = w.find('[data-usage-tooltip]').text()
    expect(tip).toContain('100 tokens')
    expect(tip).toContain('40Output')
    await w.find('[data-usage-table-toggle]').trigger('click')
    expect(w.find('[data-usage-trend-table]').text()).toContain('4 Nov')
  })
})

describe('UsageView', () => {
  it('labels measured, estimated and unavailable values and filters everything below', async () => {
    useProjectStore().list = [{ id: 'p1', name: 'shop', stack: { stacks: [] } }] as never
    const view: View = { report: report(), telemetry: [{ profileId: 'w', profile: 'Work', organizationManaged: true, managedTelemetry: true }] }
    api.usageScan.mockResolvedValue(0)
    api.usageReport.mockImplementation(async (_f, by) => (by === 'strategy' ? { ...view, report: report({ groups: [group('minimal', 'Minimal'), group('deep', 'Deep', { efficiency: { tokensPerSession: 9e4, contextPerRequest: 9000, outputPerRequest: 30, cacheHit: 0.4 } })] }) } : view))
    api.usageSessions.mockResolvedValue([
      session({}),
      session({ sessionId: 's2', status: 'missing', detail: 'no transcript in the profile’s config dir', cost: { usd: null, incomplete: false, withheldSubscription: 0, withheldUnknown: 0 }, contextEstimate: null }),
      session({ sessionId: 's3', cost: { usd: null, incomplete: false, withheldSubscription: 1, withheldUnknown: 0 } }),
    ])
    const w = mount(UsageView, { attachTo: document.body })
    await flushPromises()
    expect(api.usageScan).toHaveBeenCalled()
    expect(w.find('[data-usage-tile="tokens"]').text()).toContain('1.05k')
    expect(w.find('[data-usage-tile="tokens"]').text()).toContain('Parsed')
    expect(w.find('[data-usage-legend]').text()).toContain('never shown as spend')
    expect(w.find('[data-usage-unverified]').exists()).toBe(false)
    expect(w.find('[data-usage-tile="cost"]').text()).toMatch(/Estimate.*≈ \$0\.42.*not a bill/s)
    expect(w.find('[data-usage-tile="coverage"]').text()).toContain('1 / 3')
    expect(w.find('[data-usage-tile="coverage"]').text()).toContain('1 unavailable')
    expect(w.find('[data-usage-group="p1"]').text()).toContain('shop')
    expect(w.find('[data-usage-strategy-row="deep"]').text()).toContain('9k')
    const missing = w.find('[data-usage-session="s2"]')
    expect(missing.attributes('data-usage-status')).toBe('missing')
    expect(missing.text()).toContain('Unavailable')
    expect(missing.text()).toContain('no transcript')
    expect(missing.text()).toContain('unavailable')
    expect(w.find('[data-usage-session="s3"]').text()).toContain('not shown (subscription)')
    expect(w.find('[data-usage-privacy]').text()).toContain('Telemetry for profile Work is configured by your organisation')

    api.usageReport.mockClear()
    await w.find('[data-usage-project]').setValue('p1')
    await flushPromises()
    expect(api.usageReport).toHaveBeenCalledWith(expect.objectContaining({ projectId: 'p1' }), 'project')
    expect(api.usageSessions).toHaveBeenLastCalledWith(expect.objectContaining({ projectId: 'p1' }), 100)
    expect(replace).toHaveBeenLastCalledWith({ query: { project: 'p1' } })
    await w.find('[data-usage-by="strategy"]').trigger('click')
    await flushPromises()
    expect(api.usageReport).toHaveBeenCalledWith(expect.objectContaining({ projectId: 'p1' }), 'strategy')
    w.unmount()
  })
})

describe('UsageView with subscription and unverified formats', () => {
  it('never shows subscription usage as spend and warns about unverified transcript formats', async () => {
    api.usageScan.mockResolvedValue(0)
    const view: View = { report: report({ cost: { usd: null, incomplete: false, withheldSubscription: 2, withheldUnknown: 0 }, formatVerified: false, claudeVersions: ['2.2.0'] }), telemetry: [] }
    api.usageReport.mockResolvedValue(view)
    api.usageSessions.mockResolvedValue([])
    const w = mount(UsageView, { attachTo: document.body })
    await flushPromises()
    const cost = w.find('[data-usage-tile="cost"]').text()
    expect(cost).toContain('not shown (subscription)')
    expect(cost).toContain('2 subscription sessions not converted into spend')
    expect(w.find('[data-usage-unverified]').text()).toContain('Claude Code 2.2.0')
    w.unmount()
  })
})
