// "Needs you" (ADR-0048): Home lists what waits across projects; one click goes to the place to act.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { NeedsYouItem, SessionView, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  needsYou: vi.fn(),
  sessionList: vi.fn(),
  profileDetect: vi.fn(),
  claudeInstallations: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
const router = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('vue-router', () => ({ useRouter: () => router, useRoute: () => ({ query: {} }), RouterLink: { template: '<a><slot /></a>' } }))
vi.mock('../src/router', () => ({ router }))

import HomeView from '../src/views/HomeView.vue'
import { openNeedsYou } from '../src/lib/needsYou'
import { useAppStore } from '../src/stores/app'
import { useSessionStore } from '../src/stores/sessions'
import { useWorkspaceStore } from '../src/stores/workspaces'
import type { Router } from 'vue-router'

const item = (kind: NeedsYouItem['kind'], o: Partial<NeedsYouItem> = {}): NeedsYouItem => ({ kind, id: `${kind}-1`, title: kind, detail: 'waits', projectId: 'p1', projectName: 'anilist', docKind: null, since: null, ...o })

beforeEach(() => {
  setActivePinia(createPinia())
  Object.values(api).forEach((f) => f.mockReset())
  router.push.mockReset()
  api.sessionList.mockResolvedValue([])
  api.profileDetect.mockResolvedValue([])
  api.claudeInstallations.mockResolvedValue([])
  useWorkspaceStore().active = { id: 'w1', name: 'W' } as Workspace
})

describe('Needs you', () => {
  it('Home lists what waits across projects, with the project named', async () => {
    api.needsYou.mockResolvedValue([item('push-failed', { title: 'main', detail: 'push failed (rejected)' }), item('task-review', { title: 'Fix login', projectName: 'drivebay' })])
    const w = mount(HomeView, { global: { stubs: { ProjectCard: true, SessionRow: true } } })
    await flushPromises()
    expect(api.needsYou).toHaveBeenCalledWith('w1')
    const rows = w.findAll('[data-needs-you-item]')
    expect(rows.map((r) => r.attributes('data-needs-you-item'))).toEqual(['push-failed', 'task-review'])
    expect(rows[1]!.text()).toContain('Fix login')
    expect(rows[1]!.text()).toContain('drivebay')
    await rows[0]!.trigger('click')
    expect(useAppStore().pushRequestId).toBe('push-failed-1')
  })

  it('opens each kind where the person acts', async () => {
    const r = router as unknown as Router
    await openNeedsYou(item('task-review'), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'project', params: { id: 'p1' }, query: { tab: 'tasks' } })
    await openNeedsYou(item('draft'), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'project', params: { id: 'p1' }, query: { tab: 'learn' } })
    await openNeedsYou(item('workflow-review', { id: 'run1' }), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'workflows', query: { run: 'run1' } })
    await openNeedsYou(item('approval', { docKind: 'constitution' }), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'constitutions', query: { pending: '1' } })
    await openNeedsYou(item('approval', { docKind: 'rule' }), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'knowledge', query: { tab: 'rule' } })
    await openNeedsYou(item('kit'), r)
    expect(router.push).toHaveBeenLastCalledWith({ name: 'kits' })
    // A waiting session that runs here: its terminal comes forward.
    useSessionStore().upsert({ id: 's1', running: true } as SessionView)
    await openNeedsYou(item('session', { id: 's1' }), r)
    expect(useSessionStore().activeTab).toBe('s1')
  })
})
