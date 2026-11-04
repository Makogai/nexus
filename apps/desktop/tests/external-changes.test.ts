// Cross-process change detection (ADR-0009): regression test for the baseline race found by the E2E smoke test —
// a write by another process between the initial data load and the first poll must still be detected.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const api = vi.hoisted(() => ({ dbDataVersion: vi.fn(), appInfo: vi.fn(), uiPreferencesGet: vi.fn(), uiPreferencesSet: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'x', message: String(e) }) }))

import { useAppStore } from '../src/stores/app'

describe('external change detection', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    setActivePinia(createPinia())
    api.dbDataVersion.mockReset()
  })
  afterEach(() => vi.useRealTimers())

  it('detects a write that lands between the initial load and the first poll', async () => {
    const app = useAppStore()
    api.dbDataVersion.mockResolvedValueOnce(4) // baseline, taken before loading data
    await app.markDataBaseline()
    // … initial data is loaded here; meanwhile `nexus launch` commits → version 5 …
    api.dbDataVersion.mockResolvedValue(5)
    app.watchExternalChanges(1000)
    await vi.advanceTimersByTimeAsync(1000)
    expect(app.externalChanges).toBe(1)
    await vi.advanceTimersByTimeAsync(3000)
    expect(app.externalChanges).toBe(1) // unchanged version → no further reloads
  })

  it('does not poll while the window is hidden', async () => {
    const app = useAppStore()
    api.dbDataVersion.mockResolvedValue(1)
    await app.markDataBaseline()
    const vis = vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('hidden')
    app.watchExternalChanges(1000)
    await vi.advanceTimersByTimeAsync(5000)
    expect(api.dbDataVersion).toHaveBeenCalledTimes(1) // only the baseline
    vis.mockRestore()
  })
})
