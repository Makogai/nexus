// Other agents (ADR-0052): Codex shows whether it is installed and what its sessions get from NEXUS (full / partial /
// not available, with the reason); a profile is added with its settings; a refused option stays visible as an error.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { AgentsOverview } from '../src/lib/types'

const api = vi.hoisted(() => ({ agentsOverview: vi.fn(), agentProfileRegister: vi.fn(), agentProfileUpdate: vi.fn(), agentProfileForget: vi.fn(), uiPreferencesSet: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, toIpcError: (e: unknown) => ({ code: 'invalid', message: String(e) }) }))

import AgentProfilesCard from '../src/components/AgentProfilesCard.vue'
import { useProfileStore } from '../src/stores/profiles'

const overview = (profiles: AgentsOverview['profiles'] = []): AgentsOverview => ({
  agents: [
    { id: 'claude', name: 'Claude Code', installUrl: 'https://code.claude.com', installed: null, capabilities: [] },
    {
      id: 'codex',
      name: 'OpenAI Codex CLI',
      installUrl: 'https://github.com/openai/codex',
      installed: { path: 'C:\\Users\\me\\codex.exe', version: '0.166.0' },
      capabilities: [
        { id: 'tools', label: 'NEXUS tools (MCP)', support: 'full', detail: 'Given per session.' },
        { id: 'attention', label: 'Attention', support: 'partial', detail: 'Turn finished only.' },
        { id: 'usage', label: 'Usage', support: 'unavailable', detail: 'Not a stable format.' },
      ],
    },
  ],
  profiles,
  detected: [{ agent: 'codex', executable: 'C:\\Users\\me\\codex.exe', defaultHome: 'C:\\Users\\me\\.codex', defaultHomeExists: true, registeredProfileId: null }],
})

const mountCard = () => mount(AgentProfilesCard, { global: { stubs: { RouterLink: { template: '<a><slot /></a>' } } } })

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('Other agents', () => {
  it('shows Codex with what its sessions get and suggests a profile', async () => {
    api.agentsOverview.mockResolvedValue(overview())
    const w = mountCard()
    await flushPromises()
    expect(w.find('[data-agent="claude"]').exists()).toBe(false)
    expect(w.find('[data-agent-installed]').text()).toBe('0.166.0')
    expect(w.findAll('[data-capability]').map((c) => [c.attributes('data-capability'), c.attributes('data-support')])).toEqual([
      ['tools', 'full'],
      ['attention', 'partial'],
      ['usage', 'unavailable'],
    ])
    expect(w.find('[data-agent-suggest]').text()).toContain('~/.codex')
  })

  it('adds a profile with its settings and keeps a refusal visible', async () => {
    api.agentsOverview.mockResolvedValue(overview())
    const w = mountCard()
    await flushPromises()
    await w.find('[data-agent-add="codex"]').trigger('click')
    expect((w.find('[data-agent-field="name"]').element as HTMLInputElement).value).toBe('Codex')
    await w.find('[data-agent-field="args"]').setValue('--sandbox danger-full-access')
    await w.find('[data-agent-field="notify"]').setValue(false)
    api.agentProfileRegister.mockRejectedValueOnce('NEXUS does not start Codex without its sandbox (danger-full-access)')
    await w.find('[data-agent-form]').trigger('submit')
    await flushPromises()
    expect(api.agentProfileRegister.mock.calls[0]![0]).toMatchObject({ agent: 'codex', name: 'Codex', homeDir: null, extraArgs: ['--sandbox', 'danger-full-access'], turnNotifications: false })
    expect(w.find('[data-agent-error]').text()).toContain('danger-full-access')

    const saved = { id: 'cx', agent: 'codex' as const, name: 'Codex', homeDir: null, executable: null, extraArgs: ['--search'], inheritProviderEnv: false, turnNotifications: true, defaultWorkProfile: null, defaultModel: null, createdAt: 0, updatedAt: 0, lastUsedAt: null, archivedAt: null }
    api.agentProfileRegister.mockResolvedValueOnce(saved)
    api.agentsOverview.mockResolvedValue(overview([saved]))
    await w.find('[data-agent-field="args"]').setValue('--search')
    await w.find('[data-agent-form]').trigger('submit')
    await flushPromises()
    expect(w.find('[data-agent-form]').exists()).toBe(false)
    expect(w.find('[data-agent-profile="Codex"]').text()).toContain('~/.codex (your own)')
    expect(useProfileStore().agentProfiles.map((p) => p.id)).toEqual(['cx'])
  })
})
