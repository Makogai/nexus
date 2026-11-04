// AI Kits and profile diagnostics UI: real components + real Pinia stores; only the IPC boundary is mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import type { ClaudeProfile, Diagnostics, KitInventory, KitView } from '../src/lib/types'

const api = vi.hoisted(() => ({
  kitList: vi.fn(),
  kitApprove: vi.fn(),
  kitEnable: vi.fn(),
  kitDisable: vi.fn(),
  kitValidate: vi.fn(),
  kitUpdate: vi.fn(),
  kitRemove: vi.fn(),
  kitRegister: vi.fn(),
  profileDiagnostics: vi.fn(),
  profileList: vi.fn(async () => []),
  profileDetails: vi.fn(async () => null),
}))
vi.mock('../src/lib/ipc', () => ({
  api,
  SESSION_EVENT: 'nexus://session',
  toIpcError: (e: unknown) => {
    const o = e as { code?: string; message?: string }
    return { code: o?.code ?? 'internal', message: String(o?.message ?? e) }
  },
}))
vi.mock('@tauri-apps/plugin-dialog', () => ({ open: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))

import { defaultSelection, effectiveTokens } from '../src/lib/kits'
import KitProfileCard from '../src/components/KitProfileCard.vue'
import RegisterKitDialog from '../src/components/RegisterKitDialog.vue'
import KitsView from '../src/views/KitsView.vue'
import ProfileDiagnosticsView from '../src/views/ProfileDiagnosticsView.vue'
import { useProfileStore } from '../src/stores/profiles'

const entry = (name: string, description = `${name} skill`) => ({ name, description, listingChars: name.length + description.length + 2 })
function inventory(skills: number): KitInventory {
  return {
    name: 'probe-kit', version: '1.0.0', description: 'Probe', kind: 'plugin',
    skills: Array.from({ length: skills }, (_, i) => entry(`s${i}`)), agents: [entry('helper')], commands: [],
    outputStyles: [], hooks: [{ event: 'SessionStart', handlers: 1, types: ['command'], programs: ['node'] }],
    mcpServers: [{ name: 'db', transport: 'stdio', target: 'uvx', envVars: 0, headers: 0 }], files: 20, bytes: 1000, problems: [],
  }
}
function kit(over: Partial<KitView> = {}): KitView {
  return {
    id: 'k1', name: 'probe-kit', sourceKind: 'local', location: '/kits/probe-kit', gitRef: null, subdir: '', kind: 'plugin', version: '1.0.0',
    contentHash: 'h', approvedHash: null, commitSha: null, registeredAt: 0, updatedAt: 0, status: 'needs-review', contentDir: '/kits/probe-kit',
    inventory: inventory(12), profiles: [], ...over,
  }
}
const profile = (id: string, name: string) => ({ id, name, color: '#7c6cf2', configDir: `/cfg/${id}` }) as ClaudeProfile

describe('kit helpers', () => {
  it('large kits start with no skills; small ones with all; code-running parts are off', () => {
    expect(defaultSelection(inventory(12))).toEqual({ skills: [], agents: true, commands: true, hooks: false, mcp: false })
    expect(defaultSelection(inventory(3)).skills).toBeNull()
    const inv = inventory(12)
    expect(effectiveTokens(inv, { skills: [], agents: false, commands: false, hooks: false, mcp: false })).toBe(0)
    expect(effectiveTokens(inv, { skills: ['s1'], agents: false, commands: false, hooks: false, mcp: false })).toBe(Math.ceil(inv.skills[1]!.listingChars / 4))
  })
})

describe('KitProfileCard', () => {
  it('selects individual skills, warns when hooks or MCP are enabled, and shows policy blocks', async () => {
    const w = mount(KitProfileCard, { props: { kit: kit(), profile: profile('w', 'Work'), row: null, policy: null } })
    expect(w.text()).toContain('Skills — 0 of 12')
    await w.find('[data-skill="s3"]').setValue(true)
    expect(w.text()).toContain('Skills — 1 of 12')
    expect(w.text()).not.toContain('run programs from the kit')
    await w.find('[data-hooks]').setValue(true)
    expect(w.text()).toContain('run programs from the kit with your permissions in every session of Work')
    await w.find('[data-save]').trigger('click')
    expect(w.emitted('save')?.[0]?.[0]).toEqual({ skills: ['s3'], agents: true, commands: true, hooks: true, mcp: false })

    const blocked = mount(KitProfileCard, { props: { kit: kit(), profile: profile('c', 'Company'), row: null, policy: { disableSideloadFlags: true } as never } })
    expect(blocked.text()).toContain('disableSideloadFlags')
  })
})

describe('KitsView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    useProfileStore().list = [profile('w', 'Work'), profile('p', 'Personal')]
  })

  it('requires review before use, then enables the kit for one profile with the chosen skills', async () => {
    api.kitList.mockResolvedValue([kit()])
    api.kitApprove.mockResolvedValue(kit({ status: 'approved', approvedHash: 'h' }))
    api.kitEnable.mockResolvedValue({})
    const w = mount(KitsView, { global: { stubs: { teleport: true } } })
    await flushPromises()
    expect(w.find('[data-review]').text()).toContain('No session gets this kit until you approve this exact version')
    expect(w.find('[data-kit-detail]').text()).toContain('off unless enabled per profile')
    await w.find('[data-approve]').trigger('click')
    await flushPromises()
    expect(api.kitApprove).toHaveBeenCalledWith('k1')
    const work = w.find('[data-profile="w"]')
    await work.find('[data-skill="s0"]').setValue(true)
    await work.find('[data-save]').trigger('click')
    await flushPromises()
    expect(api.kitEnable).toHaveBeenCalledWith('k1', 'w', { skills: ['s0'], agents: true, commands: true, hooks: false, mcp: false })
  })

  it('registers a Git kit through the dialog', async () => {
    api.kitRegister.mockResolvedValue(kit())
    const w = mount(RegisterKitDialog, { props: { open: false }, global: { stubs: { teleport: true, transition: false } } })
    await w.setProps({ open: true })
    await w.find('[data-mode="git"]').trigger('click')
    await w.find('#kit-url').setValue('https://github.com/acme/kit.git')
    await w.find('#kit-ref').setValue('v1')
    await w.find('[data-register]').trigger('click')
    await flushPromises()
    expect(api.kitRegister).toHaveBeenCalledWith({ source: 'https://github.com/acme/kit.git', git: true, gitRef: 'v1', subdir: null, plugin: null })
    expect(w.emitted('registered')).toBeTruthy()
  })
})

describe('ProfileDiagnosticsView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('explains the profile by source and never shows values', async () => {
    const diag = {
      profileId: 'w', profileName: 'Work', configDir: '/cfg/w', defaultLocation: false, configExists: true, managedByNexus: false,
      isolateInstructions: false, executable: '/bin/claude', version: '2.1.311', executableError: null,
      policy: { organizationManaged: true } as never, policyEffects: ['Only organisation MCP servers run (allowManagedMcpServersOnly): user, plugin and kit servers are ignored.'],
      settings: {
        path: '/cfg/w/settings.json', exists: true, parseError: null, model: 'claude-opus-5-5', defaultPermissionMode: 'acceptEdits', outputStyle: null,
        permissionRules: [2, 0, 1], additionalDirectories: 0, envNames: ['ANTHROPIC_AUTH_TOKEN', 'DEBUG'], sensitiveEnvNames: ['ANTHROPIC_AUTH_TOKEN'],
        apiKeyHelper: true, cloudCredentialHelper: false, statusLine: false, disableAllHooks: false, claudeMdExcludes: 0,
        enabledPlugins: [], disabledPlugins: [], extraMarketplaces: [], otherKeys: [],
      },
      memory: { claudeMdLines: 3, claudeMdChars: 100, rules: 0, rulesChars: 0 },
      skills: [{ ...entry('deploy'), source: 'user', origin: null }, { ...entry('probe-kit:s0'), source: 'kit', origin: 'probe-kit' }],
      agents: [], commands: [], outputStyles: [],
      mcpServers: [{ name: 'db', qualifiedName: 'db', scope: 'user', origin: null, transport: 'stdio', target: 'uvx', envVars: 1, headers: 0 }],
      hooks: [{ event: 'Stop', handlers: 1, types: ['command'], programs: ['notify'], source: 'user', origin: null }],
      plugins: [], marketplaces: [], kits: [{ kitId: 'k1', inventory: inventory(1), blocked: null }],
      context: { skillsTokens: 10, agentsTokens: 0, commandsTokens: 0, memoryTokens: 25, totalTokens: 35 },
      warnings: ['settings.json sets credential-like variables for every session of this profile (ANTHROPIC_AUTH_TOKEN) — values are not shown'],
    } satisfies Diagnostics
    api.profileDiagnostics.mockResolvedValue(diag)
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:p(.*)*', component: { template: '<div />' } }] })
    const w = mount(ProfileDiagnosticsView, { props: { id: 'w' }, global: { plugins: [router] } })
    await flushPromises()
    expect(w.find('[data-section="policy"]').text()).toContain('allowManagedMcpServersOnly')
    expect(w.find('[data-section="skills"]').text()).toContain('Your skills')
    expect(w.find('[data-section="skills"]').text()).toContain('From NEXUS kits (this profile only)')
    expect(w.find('[data-section="overview"]').text()).toContain('names only — values are never shown')
    expect(w.find('[data-section="mcp"]').text()).toContain('uvx')
    expect(w.find('[data-section="hooks"]').text()).toContain('runs notify')
    expect(w.findAll('[data-warning]')).toHaveLength(1)
    expect(w.find('[data-section="context"]').text()).toContain('35 tokens')
  })
})
