// Claude Code setup (P5): official methods when Claude Code is missing; a method is only typed into a terminal (the
// person presses Enter); a ready installation shows no methods.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ClaudeSetupStatus } from '../src/lib/types'

const api = vi.hoisted(() => ({ claudeSetupStatus: vi.fn() }))
const typed = vi.hoisted(() => vi.fn())
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('../src/lib/terminal', () => ({ typeIntoTerminal: typed }))
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push: vi.fn() }) }))

import ClaudeSetupView from '../src/views/ClaudeSetupView.vue'

const status = (over: Partial<ClaudeSetupStatus> = {}): ClaudeSetupStatus => ({
  installations: [], ready: false, version: null, profiles: 0, gitForWindows: true, node: null, docsUrl: 'https://code.claude.com/docs/en/setup', verified: '2026-11-05',
  next: ['Install Claude Code with one of the official methods below (the recommended one updates itself).'],
  methods: [
    { id: 'native-powershell', title: 'Native installer (PowerShell)', command: 'irm https://claude.ai/install.ps1 | iex', shell: 'powershell', recommended: true, autoUpdates: true, notes: 'No administrator rights needed.' },
    { id: 'winget', title: 'WinGet', command: 'winget install Anthropic.ClaudeCode', shell: 'any', recommended: false, autoUpdates: false, notes: 'Does not update itself.' },
  ],
  ...over,
})

beforeEach(() => {
  setActivePinia(createPinia())
  api.claudeSetupStatus.mockReset()
  typed.mockReset()
})

describe('Claude Code setup', () => {
  it('offers the official methods and only types the chosen one into a terminal', async () => {
    api.claudeSetupStatus.mockResolvedValue(status())
    const w = mount(ClaudeSetupView)
    await flushPromises()
    expect(w.find('[data-setup-status]').text()).toContain('not installed')
    const m = w.find('[data-method="native-powershell"]')
    expect(m.text()).toContain('irm https://claude.ai/install.ps1 | iex')
    expect(m.text()).toContain('recommended')
    expect(w.find('[data-method="winget"]').text()).toContain('update by hand')
    expect(typed).not.toHaveBeenCalled()
    await w.find('[data-method-type="native-powershell"]').trigger('click')
    expect(typed).toHaveBeenCalledWith('irm https://claude.ai/install.ps1 | iex', { shell: 'powershell' })
  })

  it('shows a ready installation without installation methods', async () => {
    api.claudeSetupStatus.mockResolvedValue(status({ ready: true, version: '2.1.311', profiles: 2, next: [] }))
    const w = mount(ClaudeSetupView)
    await flushPromises()
    expect(w.find('[data-setup-status]').text()).toContain('Claude Code 2.1.311')
    expect(w.find('[data-setup-methods]').exists()).toBe(false)
  })
})
