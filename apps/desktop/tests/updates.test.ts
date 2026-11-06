// Signed updates (ADR-0044): release builds check, install by themselves at start (unless turned off or sessions run),
// and ask before restarting over running sessions. The Tauri updater (signature verification) is mocked here.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

const plugin = vi.hoisted(() => ({ check: vi.fn(), relaunch: vi.fn() }))
vi.mock('@tauri-apps/plugin-updater', () => ({ check: plugin.check }))
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: plugin.relaunch }))
vi.mock('../src/lib/ipc', () => ({ api: {}, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))

import { useAppStore } from '../src/stores/app'
import { useSessionStore } from '../src/stores/sessions'
import { useUpdateStore } from '../src/stores/updates'
import type { AppInfo, SessionView } from '../src/lib/types'

function fakeUpdate(version = '0.1.2') {
  return { version, body: 'Notes', date: '2026-11-06', downloadAndInstall: vi.fn(async (cb?: (e: unknown) => void) => cb?.({ event: 'Finished' })) }
}
const info = (updates: boolean) => ({ version: '0.1.1', updates }) as unknown as AppInfo

beforeEach(() => {
  setActivePinia(createPinia())
  plugin.check.mockReset()
  plugin.relaunch.mockReset()
})

describe('updates', () => {
  it('a development build never checks', async () => {
    useAppStore().info = info(false)
    await useUpdateStore().checkNow(true)
    expect(plugin.check).not.toHaveBeenCalled()
  })

  it('at start an update installs by itself (verified by the plugin), then NEXUS restarts', async () => {
    useAppStore().info = info(true)
    const u = fakeUpdate()
    plugin.check.mockResolvedValue(u)
    await useUpdateStore().checkNow(true)
    expect(u.downloadAndInstall).toHaveBeenCalled()
    expect(plugin.relaunch).toHaveBeenCalled()
  })

  it('is only offered when automatic installing is off, and asks before ending running sessions', async () => {
    const app = useAppStore()
    app.info = info(true)
    app.prefs.autoUpdate = false
    const u = fakeUpdate()
    plugin.check.mockResolvedValue(u)
    const updates = useUpdateStore()
    await updates.checkNow(true)
    expect(updates.available?.version).toBe('0.1.2')
    expect(u.downloadAndInstall).not.toHaveBeenCalled()

    useSessionStore().list = [{ id: 's1', running: true, attached: true } as SessionView]
    const confirm = vi.spyOn(app, 'confirm').mockResolvedValue(false)
    await updates.install()
    expect(confirm).toHaveBeenCalled()
    expect(u.downloadAndInstall, 'declined: nothing installed').not.toHaveBeenCalled()
    confirm.mockResolvedValue(true)
    await updates.install()
    expect(u.downloadAndInstall).toHaveBeenCalled()
  })

  it('no update, or offline: nothing happens and the reason is kept for Settings', async () => {
    useAppStore().info = info(true)
    plugin.check.mockResolvedValue(null)
    const updates = useUpdateStore()
    await updates.checkNow(true)
    expect(updates.available).toBeNull()
    plugin.check.mockRejectedValue(new Error('offline'))
    await updates.checkNow(false)
    expect(updates.phase).toBe('error')
    expect(updates.error).toBe('offline')
  })
})
