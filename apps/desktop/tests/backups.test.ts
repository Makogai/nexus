// Settings → Backups (ADR-0032): list, back up now, restore = schedule + restart after a confirmation.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { BackupInfo } from '../src/lib/types'

const api = vi.hoisted(() => ({
  shellList: vi.fn(),
  backupList: vi.fn(),
  backupCreate: vi.fn(),
  backupScheduleRestore: vi.fn(),
  backupCancelRestore: vi.fn(),
  backupDelete: vi.fn(),
  appRestart: vi.fn(),
  appInfo: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn() }))

import SettingsView from '../src/views/SettingsView.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import { useAppStore } from '../src/stores/app'

const backup = (o: Partial<BackupInfo>): BackupInfo => ({ id: 'b1', path: 'x', kind: 'manual', createdAt: Date.now(), schemaVersion: 15, appVersion: '0.1.0', bytes: 4096, restorable: true, problem: null, ...o })

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  api.shellList.mockResolvedValue([])
  useAppStore().info = { version: '0.1.0', platform: 'windows', dataDir: 'C:\\d', dbPath: 'C:\\d\\nexus.db', schemaVersion: 15, gitVersion: null, homeDir: 'C:\\h', recovery: null, supportedSchema: 15, cliPath: 'C:\\Programs\\NEXUS\\nexus.exe', pendingRestore: null, restored: null, restoreError: null } as never
})

describe('Settings → Backups', () => {
  it('lists backups, restores only after a confirmation (schedule + restart) and explains unusable ones', async () => {
    api.backupList.mockResolvedValue([backup({}), backup({ id: 'b2', kind: 'automatic', restorable: false, problem: 'made by a newer NEXUS' }), backup({ id: 'r', kind: 'replaced' })])
    api.backupScheduleRestore.mockResolvedValue(backup({}))
    const w = mount(SettingsView, { attachTo: document.body })
    const dialog = mount(ConfirmDialog, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-backup="r"]').exists(), 'replaced files are not offered as a backup').toBe(false)
    expect(w.find('[data-backup="b2"]').text()).toContain('made by a newer NEXUS')
    expect(w.find('[data-backup-restore="b2"]').exists()).toBe(false)
    expect(w.find('[data-cli]').text()).toContain('C:\\Programs\\NEXUS\\nexus.exe')
    await w.find('[data-backup-restore="b1"]').trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-confirm-message]')?.textContent).toContain('The current state is backed up first')
    expect(api.backupScheduleRestore).not.toHaveBeenCalled()
    ;(document.querySelector('[data-confirm-ok]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.backupScheduleRestore).toHaveBeenCalledWith('b1')
    expect(api.appRestart).toHaveBeenCalled()
    dialog.unmount()
    w.unmount()
  })

  it('backs up on request', async () => {
    api.backupList.mockResolvedValueOnce([]).mockResolvedValueOnce([backup({ id: 'new' })])
    api.backupCreate.mockResolvedValue(backup({ id: 'new' }))
    const w = mount(SettingsView, { attachTo: document.body })
    await flushPromises()
    await w.find('[data-backup-create]').trigger('click')
    await flushPromises()
    expect(api.backupCreate).toHaveBeenCalled()
    expect(w.find('[data-backup="new"]').exists()).toBe(true)
    w.unmount()
  })
})
