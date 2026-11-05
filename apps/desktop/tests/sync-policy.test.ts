// Sync policy card (P7): review/manual save directly; trusted only after confirming the remote is private; unavailable
// trusted mode (organisation policy, no remote) cannot be chosen.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { SyncPolicyView } from '../src/lib/types'

const api = vi.hoisted(() => ({ syncPolicyGet: vi.fn(), syncPolicySet: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))

import SyncPolicyCard from '../src/components/SyncPolicyCard.vue'

const view = (over: Partial<SyncPolicyView> = {}): SyncPolicyView => ({
  policy: { mode: 'manual', trust: null },
  managed: { trustedAllowed: true, reviewAllowed: true, source: null },
  allowlist: ['project-context/*/development/*.md', 'project-context/*/current-state.md'],
  trustedUnavailable: null,
  remote: 'https://example.test/me/workspace.git',
  branch: 'main',
  ...over,
})

beforeEach(() => {
  setActivePinia(createPinia())
  api.syncPolicyGet.mockReset()
  api.syncPolicySet.mockReset()
})

describe('SyncPolicyCard', () => {
  it('turns trusted sync on only after the private-remote confirmation', async () => {
    api.syncPolicyGet.mockResolvedValue(view())
    api.syncPolicySet.mockImplementation(async (_w: string, mode: string) => view({ policy: { mode: mode as never, trust: null } }))
    const w = mount(SyncPolicyCard, { props: { workspaceId: 'w1' }, attachTo: document.body })
    await flushPromises()
    await w.find('[data-sync-policy-choose="review"]').trigger('change')
    await flushPromises()
    expect(api.syncPolicySet).toHaveBeenLastCalledWith('w1', 'review', false)
    await w.find('[data-sync-policy-choose="trusted"]').trigger('change')
    await flushPromises()
    expect(document.querySelector('[data-trusted-allowlist]')?.textContent).toContain('project-context/*/development/*.md')
    const confirm = document.querySelector('[data-trusted-confirm]') as HTMLButtonElement
    expect(confirm.disabled).toBe(true)
    expect(api.syncPolicySet).toHaveBeenCalledTimes(1)
    ;(document.querySelector('[data-trusted-private]') as HTMLInputElement).click()
    await flushPromises()
    ;(document.querySelector('[data-trusted-confirm]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.syncPolicySet).toHaveBeenLastCalledWith('w1', 'trusted', true)
    w.unmount()
  })

  it('cannot choose trusted when it is unavailable and says why', async () => {
    api.syncPolicyGet.mockResolvedValue(view({ trustedUnavailable: 'turned off by your organisation (policy.json)' }))
    const w = mount(SyncPolicyCard, { props: { workspaceId: 'w1' } })
    await flushPromises()
    expect(w.find('[data-sync-policy-choose="trusted"]').attributes('disabled')).toBeDefined()
    expect(w.find('[data-trusted-unavailable]').text()).toContain('organisation')
  })
})
