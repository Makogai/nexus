// Component tests: real components + real Pinia stores; only the IPC boundary and native plugins are mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import { createMemoryHistory, createRouter } from 'vue-router'
import { defineComponent, type Component, type DefineComponent } from 'vue'
import type { ClaudeProfile, LaunchOptions, Project, SessionView } from '../src/lib/types'

const api = vi.hoisted(() => ({
  sessionStartClaude: vi.fn(),
  sessionStartAgent: vi.fn(),
  agentsOverview: vi.fn(),
  sessionResume: vi.fn(),
  sessionContinueTask: vi.fn(),
  sessionInterrupt: vi.fn(),
  sessionTerminate: vi.fn(),
  sessionRename: vi.fn(),
  sessionSetArchived: vi.fn(),
  sessionRelease: vi.fn(),
  profileCreate: vi.fn(),
  profileRegister: vi.fn(),
  profileUpdate: vi.fn(),
  profileList: vi.fn(),
  profileDetect: vi.fn(),
  launchOptions: vi.fn(),
  shellList: vi.fn(),
  uiPreferencesSet: vi.fn(),
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
vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(), revealItemInDir: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))

import LaunchDialog from '../src/components/LaunchDialog.vue'
import ProfileDialog from '../src/components/ProfileDialog.vue'
import SessionRow from '../src/components/SessionRow.vue'
import CommandPalette from '../src/components/CommandPalette.vue'
import BaseModal from '../src/components/BaseModal.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import { useAppStore } from '../src/stores/app'
import { useProfileStore } from '../src/stores/profiles'
import { useProjectStore } from '../src/stores/projects'
import { useSessionStore } from '../src/stores/sessions'

const OPTIONS: LaunchOptions = {
  workProfiles: ['prototype', 'balanced', 'professional', 'critical'].map((id) => ({
    id,
    name: id[0]!.toUpperCase() + id.slice(1),
    summary: `${id} summary`,
    characteristics: ['x'],
    prompt: `NEXUS work profile: ${id.toUpperCase()}.`,
  })),
  defaultWorkProfile: 'balanced',
  effortLevels: ['low', 'medium', 'high', 'xhigh', 'max'],
  taskKickoff: 'Please start on this session’s NEXUS task now.',
}

function profile(id: string, over: Partial<ClaudeProfile> = {}): ClaudeProfile {
  return {
    id,
    name: id,
    description: '',
    color: '#7c6cf2',
    configDir: `/cfg/${id}`,
    executable: null,
    extraArgs: [],
    managedByNexus: false,
    inheritProviderEnv: false,
    isolateInstructions: false,
    defaultWorkProfile: null,
    defaultEffort: null,
    defaultModel: null,
    createdAt: 0,
    updatedAt: 0,
    lastUsedAt: null,
    archivedAt: null,
    ...over,
  }
}

const PROJECT: Project = {
  id: 'p1',
  workspaceId: 'w1',
  name: 'Game',
  localPath: '/code/game',
  remoteUrl: null,
  stack: { stacks: ['godot'], packageManagers: [], scripts: [], hasReadme: false, hasClaudeMd: false, hasAgentsMd: false, hasClaudeDir: false, isGit: true },
  createdAt: 0,
  lastOpenedAt: null,
  available: true,
}

function session(over: Partial<SessionView> = {}): SessionView {
  return {
    id: 's1',
    kind: 'claude',
    host: 'desktop',
    hostId: 'h1',
    workspaceId: 'w1',
    projectId: 'p1',
    profileId: 'work',
    claudeSessionId: '0f8b1c2e-6a4d-4e2a-9d4b-2f1c3b5a7e90',
    title: 'Fix login',
    cwd: '/code/game',
    branch: 'main',
    workProfile: 'balanced',
    effort: null,
    model: null,
    program: 'claude',
    pid: 42,
    state: 'exited',
    exitCode: 0,
    error: null,
    resumedFrom: null,
    startedAt: Date.now() - 60_000,
    endedAt: Date.now(),
    archivedAt: null,
    attached: false,
    running: false,
    liveElsewhere: false,
    integrations: null,
    turns: 0,
    prompts: 0,
    subagents: 0,
    lastActivityAt: null,
    attention: null,
    attentionKind: null,
    attentionAt: null,
    endReason: null,
    transcriptPath: null,
    taskId: null,
    agent: 'claude',
    agentProfileId: null,
    conversationRecorded: true,
    resumable: true,
    status: { status: 'completed', certainty: 'reported', since: null, detail: null },
    ...over,
  }
}

const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: '/', component: defineComponent({ template: '<div />' }) },
    { path: '/:any(.*)*', component: defineComponent({ template: '<div />' }) },
  ],
})

type AnyProps = Record<string, unknown>

function mountWith(component: Component, props: AnyProps = {}) {
  return mount(component as unknown as DefineComponent<AnyProps>, {
    props,
    global: { plugins: [router], stubs: { teleport: true } },
    attachTo: document.body,
  })
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  api.sessionRelease.mockResolvedValue(true)
  api.uiPreferencesSet.mockResolvedValue(undefined)
  api.shellList.mockResolvedValue([])
  api.agentsOverview.mockResolvedValue({ agents: [], profiles: [], detected: [] })
  document.body.innerHTML = ''
})

function seed() {
  const profiles = useProfileStore()
  profiles.list = [profile('work', { defaultWorkProfile: 'professional', defaultEffort: 'high' }), profile('game', { defaultWorkProfile: 'prototype', configDir: null })]
  profiles.launchOptions = OPTIONS
  useProjectStore().list = [PROJECT]
}

describe('LaunchDialog', () => {
  it('opens on the requested profile with that profile’s rigor and launches with the exact arguments', async () => {
    seed()
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game')
    await flushPromises()

    expect((w.find('#launch-profile').element as HTMLSelectElement).value).toBe('game')
    expect(w.find('.segment.selected').text()).toBe('Prototype')
    expect(w.text()).toContain('CLAUDE_CONFIG_DIR is removed for this session') // default-location profile

    api.sessionStartClaude.mockResolvedValue(session({ id: 'new', running: true, state: 'running' }))
    await w.find('#launch-title').setValue('  Boss fight  ')
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()

    expect(api.sessionStartClaude).toHaveBeenCalledTimes(1)
    expect(api.sessionStartClaude.mock.calls[0]![0]).toMatchObject({
      profileId: 'game',
      projectId: 'p1',
      cwd: null,
      workProfile: 'prototype',
      effort: null,
      model: null,
      title: 'Boss fight',
    })
    expect(app.launchDialog.open).toBe(false)
    expect(app.prefs.lastLaunch).toEqual({ profileId: 'game', workProfile: 'prototype', effort: null, agent: 'claude' })
    expect(useSessionStore().tabs).toContain('new')
  })

  it('launches Codex from its own profile when Codex is chosen (ADR-0052)', async () => {
    seed()
    const codexProfile = {
      id: 'cx', agent: 'codex' as const, name: 'Codex', homeDir: null, executable: null, extraArgs: [], inheritProviderEnv: false, turnNotifications: true,
      defaultWorkProfile: 'critical', defaultModel: null, createdAt: 0, updatedAt: 0, lastUsedAt: null, archivedAt: null,
    }
    const caps = [{ id: 'tools' as const, label: 'NEXUS tools (MCP)', support: 'full' as const, detail: 'x' }]
    api.agentsOverview.mockResolvedValue({
      agents: [
        { id: 'claude', name: 'Claude Code', installUrl: 'https://code.claude.com', installed: null, capabilities: caps },
        { id: 'codex', name: 'OpenAI Codex CLI', installUrl: 'https://github.com/openai/codex', installed: { path: 'C:\\codex.exe', version: '0.166.0' }, capabilities: caps },
      ],
      profiles: [codexProfile],
      detected: [],
    })
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game')
    await flushPromises()
    expect(w.findAll('[data-launch-agent]').map((b) => b.attributes('data-launch-agent'))).toEqual(['claude', 'codex'])
    expect(w.find('#launch-effort').exists()).toBe(true)
    await w.find('[data-launch-agent="codex"]').trigger('click')
    expect(w.find('#launch-effort').exists()).toBe(false)
    expect(w.find('[data-launch-agent-profile]').text()).toContain('Your own Codex home')
    expect(w.find('.segment.selected[role="radio"]:not([data-launch-agent])').text()).toBe('Critical')

    api.sessionStartAgent.mockResolvedValue(session({ id: 'cx-session', running: true, state: 'running', agent: 'codex', agentProfileId: 'cx' }))
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionStartClaude).not.toHaveBeenCalled()
    expect(api.sessionStartAgent.mock.calls[0]![0]).toMatchObject({ agentProfileId: 'cx', projectId: 'p1', workProfile: 'critical' })
    expect(api.sessionStartAgent.mock.calls[0]![0]).not.toHaveProperty('effort')
    expect(app.prefs.lastLaunch).toMatchObject({ agent: 'codex', agentProfileId: 'cx', profileId: 'game' })
  })

  it('a profile chosen to launch with decides the agent, whatever was used last', async () => {
    seed()
    const codex = (id: string, name: string) => ({
      id, agent: 'codex' as const, name, homeDir: null, executable: null, extraArgs: [], inheritProviderEnv: false, turnNotifications: true,
      defaultWorkProfile: null, defaultModel: null, createdAt: 0, updatedAt: 0, lastUsedAt: null, archivedAt: null,
    })
    api.agentsOverview.mockResolvedValue({
      agents: [
        { id: 'claude', name: 'Claude Code', installUrl: 'https://code.claude.com', installed: null, capabilities: [] },
        { id: 'codex', name: 'OpenAI Codex CLI', installUrl: 'https://github.com/openai/codex', installed: null, capabilities: [] },
      ],
      profiles: [codex('cx', 'Codex'), codex('cx-work', 'Codex work')],
      detected: [],
    })
    const app = useAppStore()
    app.prefs.lastLaunch = { profileId: 'work', workProfile: 'balanced', effort: null, agent: 'codex', agentProfileId: 'cx' }
    const w = mountWith(LaunchDialog)
    const checked = () => w.findAll('[data-launch-agent]').filter((b) => b.attributes('aria-checked') === 'true').map((b) => b.attributes('data-launch-agent'))

    app.openLaunch() // nothing chosen: the agent used last
    await flushPromises()
    expect(checked()).toEqual(['codex'])

    app.launchDialog.open = false
    await flushPromises()
    app.openLaunch(null, 'game') // a Claude profile's "Launch with this profile"
    await flushPromises()
    expect(checked()).toEqual(['claude'])
    expect((w.find('#launch-profile').element as HTMLSelectElement).value).toBe('game')

    app.launchDialog.open = false
    await flushPromises()
    app.openLaunch(null, null, null, { agentProfileId: 'cx-work' }) // a Codex profile's
    await flushPromises()
    expect(checked()).toEqual(['codex'])
    expect((w.find('#launch-agent-profile').element as HTMLSelectElement).value).toBe('cx-work')
  })

  it('launches for a task: its project is fixed, the task id and start choice are sent', async () => {
    seed()
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game', { id: 'task-1', title: 'Ship login', projectId: 'p1' })
    await flushPromises()
    expect(w.find('[data-launch-task]').text()).toContain('Ship login')
    expect(w.find('#launch-project').attributes('disabled')).toBeDefined()
    expect((w.find('#launch-project').element as HTMLSelectElement).value).toBe('p1')
    expect(w.find('#launch-title').attributes('placeholder')).toBe('Ship login')
    await w.find('[data-launch-task] input[type=checkbox]').setValue(false)
    api.sessionStartClaude.mockResolvedValue(session({ id: 'new', running: true, state: 'running' }))
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionStartClaude.mock.calls[0]![0]).toMatchObject({ projectId: 'p1', taskId: 'task-1', startTask: false, kickoff: true, title: null })
  })

  it('a task launch starts right away by default and remembers when the person turns it off', async () => {
    seed()
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game', { id: 'task-1', title: 'Ship login', projectId: 'p1' })
    await flushPromises()
    const box = () => w.find('[data-launch-kickoff] input').element as HTMLInputElement
    expect(box().checked, 'on by default').toBe(true)
    await w.find('[data-launch-kickoff] input').setValue(false)
    api.sessionStartClaude.mockResolvedValue(session({ id: 'new', running: true, state: 'running' }))
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionStartClaude.mock.calls[0]![0]).toMatchObject({ taskId: 'task-1', kickoff: false })
    expect(app.prefs.lastLaunch?.taskKickoff).toBe(false)
    app.openLaunch(null, 'game', { id: 'task-2', title: 'Fix cart', projectId: 'p1' })
    await flushPromises()
    expect(box().checked, 'the last choice is remembered').toBe(false)
  })

  it('a task with its own earlier session resumes it by default — no new session', async () => {
    seed()
    useSessionStore().list = [session({ id: 'own', taskId: 'task-1', running: false, resumable: true, state: 'exited' })]
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game', { id: 'task-1', title: 'Ship login', projectId: 'p1' })
    await flushPromises()
    expect(w.find('[data-start-in="own"]').classes()).toContain('selected')
    expect(w.find('[data-start-in-hint]').text()).toContain('cheapest')
    api.sessionResume.mockResolvedValue(session({ id: 'own', running: true, state: 'running', taskId: 'task-1' }))
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionResume).toHaveBeenCalledWith('own', expect.anything(), expect.anything())
    expect(api.sessionStartClaude).not.toHaveBeenCalled()
  })

  it('a task can be handed to a running session of its project that has NEXUS tools', async () => {
    seed()
    const running = session({ id: 'busy', title: 'Refactor cart', running: true, attached: true, state: 'running', integrations: { mcp: { state: 'enabled' } } as never })
    const noTools = session({ id: 'plain', title: 'No tools', running: true, attached: true, state: 'running', integrations: { mcp: { state: 'blocked', reason: 'x' } } as never })
    useSessionStore().list = [running, noTools]
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game', { id: 'task-2', title: 'Fix rounding', projectId: 'p1' })
    await flushPromises()
    expect(w.find('[data-start-in="new"]').classes(), 'handing over is never the default').toContain('selected')
    await w.find('[data-start-in="continue"]').trigger('click')
    expect(w.find('[data-start-in-hint]').text()).toContain('Refactor cart')
    expect(w.find('[data-continue-session]').exists(), 'only one session qualifies (the other has no NEXUS tools)').toBe(false)
    api.sessionContinueTask.mockResolvedValue({ ...running, taskId: 'task-2' })
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionContinueTask).toHaveBeenCalledWith('busy', 'task-2')
    expect(api.sessionStartClaude).not.toHaveBeenCalled()
  })

  it('the Learn view preselects “start right away”, shows the exact first message and sends it', async () => {
    seed()
    const app = useAppStore()
    const w = mountWith(LaunchDialog)
    app.openLaunch(null, 'game', { id: 'task-1', title: 'Learn App', projectId: 'p1' }, { kickoff: true })
    await flushPromises()
    expect((w.find('[data-launch-kickoff] input').element as HTMLInputElement).checked).toBe(true)
    expect(w.find('.kickoff-text').text()).toBe(`“${OPTIONS.taskKickoff}”`)
    api.sessionStartClaude.mockResolvedValue(session({ id: 'new', running: true, state: 'running' }))
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.sessionStartClaude.mock.calls[0]![0]).toMatchObject({ taskId: 'task-1', kickoff: true })
    // Without a task there is nothing to start on.
    app.openLaunch('p1', 'game', null, { kickoff: true })
    expect(app.launchDialog.kickoff).toBeUndefined()
  })

  it('switching profile applies that profile’s defaults', async () => {
    seed()
    const w = mountWith(LaunchDialog)
    useAppStore().openLaunch(null, 'game')
    await flushPromises()
    await w.find('#launch-profile').setValue('work')
    expect(w.find('.segment.selected').text()).toBe('Professional')
    expect((w.find('#launch-effort').element as HTMLSelectElement).value).toBe('high')
  })

  it('keeps the dialog open and shows the backend error when launching fails', async () => {
    seed()
    const w = mountWith(LaunchDialog)
    useAppStore().openLaunch()
    await flushPromises()
    api.sessionStartClaude.mockRejectedValue({ code: 'not_found', message: 'Claude Code was not found.' })
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(w.find('.notice-error').text()).toContain('Claude Code was not found.')
    expect(useAppStore().launchDialog.open).toBe(true)
  })

  it('explains what to do when no profile is registered', async () => {
    useProfileStore().launchOptions = OPTIONS
    const w = mountWith(LaunchDialog)
    useAppStore().openLaunch()
    await flushPromises()
    expect(w.text()).toContain('No Claude profile is registered yet')
    expect(w.findAll('footer .btn-primary')[0]!.attributes('disabled')).toBeDefined()
  })
})

describe('ProfileDialog', () => {
  it('create mode sends trimmed launcher arguments and the chosen folder', async () => {
    useProfileStore().launchOptions = OPTIONS
    api.profileCreate.mockResolvedValue(profile('x'))
    api.profileList.mockResolvedValue([])
    api.launchOptions.mockResolvedValue(OPTIONS)
    api.profileDetect.mockResolvedValue([])
    const w = mountWith(ProfileDialog, { open: false, mode: 'create' })
    await w.setProps({ open: true })
    await w.find('#pf-name').setValue('Experimental')
    await w.find('input.mono').setValue('/home/u/.claude-exp')
    await w.find('#pf-args').setValue('--permission-mode\n\n  acceptEdits  \n')
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.profileCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'Experimental',
        configDir: '/home/u/.claude-exp',
        extraArgs: ['--permission-mode', 'acceptEdits'],
        inheritProviderEnv: false,
      }),
    )
    expect(w.emitted('close')).toBeTruthy()
  })

  it('instruction isolation is opt-in, sent when chosen and unavailable for the default location', async () => {
    useProfileStore().launchOptions = OPTIONS
    api.profileCreate.mockResolvedValue(profile('w'))
    const w = mountWith(ProfileDialog, { open: true, mode: 'create' })
    await flushPromises()
    const box = w.find('[data-isolation] input')
    expect((box.element as HTMLInputElement).checked).toBe(false)
    expect(w.find('[data-isolation]').text()).toContain('Organisation-managed and project instructions still load')
    await w.find('#pf-name').setValue('Work')
    await w.find('.input.mono').setValue('/home/u/.claude-work')
    await box.setValue(true)
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.profileCreate).toHaveBeenCalledWith(expect.objectContaining({ isolateInstructions: true }))

    const d = mountWith(ProfileDialog, { open: true, mode: 'edit', profile: profile('d', { configDir: null }) })
    await flushPromises()
    expect(d.find('[data-isolation] input').attributes('disabled')).toBeDefined()
  })

  it('registering the detected default location sends no config dir', async () => {
    useProfileStore().launchOptions = OPTIONS
    api.profileRegister.mockResolvedValue(profile('d'))
    api.profileList.mockResolvedValue([])
    api.launchOptions.mockResolvedValue(OPTIONS)
    api.profileDetect.mockResolvedValue([])
    const detected = { configDir: null, path: '/home/u/.claude', suggestedName: 'Default', markers: [], source: 'default location', registeredProfileId: null }
    const w = mountWith(ProfileDialog, { open: false, mode: 'register', detected })
    await w.setProps({ open: true })
    expect((w.find('#pf-name').element as HTMLInputElement).value).toBe('Default')
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(api.profileRegister.mock.calls[0]![0].configDir).toBeNull()
  })

  it('shows validation errors from the backend', async () => {
    useProfileStore().launchOptions = OPTIONS
    api.profileCreate.mockRejectedValue({ code: 'invalid', message: "'--resume' is managed by NEXUS" })
    const w = mountWith(ProfileDialog, { open: false, mode: 'create' })
    await w.setProps({ open: true })
    await w.find('#pf-name').setValue('X')
    await w.findAll('footer .btn-primary')[0]!.trigger('click')
    await flushPromises()
    expect(w.find('.notice-error').text()).toContain('managed by NEXUS')
    expect(w.emitted('close')).toBeFalsy()
  })
})

describe('SessionRow', () => {
  it('resumable session offers Resume and resumes through the store', async () => {
    seed()
    api.sessionResume.mockResolvedValue(session({ id: 's2', running: true, state: 'running', resumedFrom: 's1' }))
    const w = mountWith(SessionRow, { session: session() })
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(api.sessionResume).toHaveBeenCalledWith('s1', 120, 32)
    expect(useSessionStore().tabs).toEqual(['s2'])
  })

  it('an interrupted session without a transcript is never offered for resume', () => {
    seed()
    const w = mountWith(SessionRow, {
      session: session({ state: 'interrupted', endedAt: null, conversationRecorded: false, resumable: false }),
    })
    expect(w.text()).toContain('Interrupted (NEXUS closed)')
    expect(w.text()).toContain('no conversation recorded')
    expect(w.text()).not.toContain('Resume')
    expect(w.text()).not.toMatch(/\d+s ·|· \d+s/) // unknown duration is not shown
  })

  it('terminating a running session requires confirmation', async () => {
    seed()
    const running = session({ running: true, state: 'running', endedAt: null, resumable: false, attached: true })
    const w = mountWith(SessionRow, { session: running })
    const confirm = mountWith(ConfirmDialog)
    await w.find('button[title="Terminate…"]').trigger('click')
    await flushPromises()
    expect(confirm.text()).toContain('Terminate this process?')
    await confirm.findAll('footer .btn').find((b) => b.text() === 'Cancel')!.trigger('click')
    await flushPromises()
    expect(api.sessionTerminate).not.toHaveBeenCalled()

    await w.find('button[title="Terminate…"]').trigger('click')
    await flushPromises()
    api.sessionTerminate.mockResolvedValue(undefined)
    await confirm.findAll('footer .btn').find((b) => b.text() === 'Terminate')!.trigger('click')
    await flushPromises()
    expect(api.sessionTerminate).toHaveBeenCalledWith('s1')
  })

  it('a confirmation opened from a dialog is on top of it, and Escape closes only the confirmation', async () => {
    seed()
    // The confirmation is mounted first (as in App.vue) and the dialog later — the stack, not the order, decides.
    const confirm = mountWith(ConfirmDialog)
    const closed = vi.fn()
    const dialog = mountWith(BaseModal, { open: true, title: 'Commit', onClose: closed })
    await flushPromises()
    const answer = useAppStore().confirm({ title: 'Commit these changes?', message: 'x', confirmLabel: 'Commit' })
    await flushPromises()
    const z = (w: ReturnType<typeof mountWith>) => Number((w.find('.backdrop').element as HTMLElement).style.zIndex)
    expect(z(confirm)).toBeGreaterThan(z(dialog))
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(await answer).toBe(false)
    expect(closed).not.toHaveBeenCalled()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    expect(closed).toHaveBeenCalledTimes(1)
    dialog.unmount()
    confirm.unmount()
  })

  it('explains when NEXUS tools were blocked by organisation policy', () => {
    seed()
    const reason = 'organisation policy disables --mcp-config (disableSideloadFlags)'
    const w = mountWith(SessionRow, { session: session({ integrations: { mcp: { state: 'blocked', reason } } }) })
    const note = w.findAll('span').find((x) => x.text() === 'NEXUS tools off')
    expect(note?.attributes('title')).toBe(reason)
  })

  it('shows turns, a reported attention badge and blocked hooks', () => {
    seed()
    const running = session({
      running: true,
      state: 'running',
      endedAt: null,
      resumable: false,
      turns: 4,
      attention: 'Permission requested: Bash',
      attentionKind: 'permission',
      status: { status: 'permission-required', certainty: 'reported', since: Date.now(), detail: 'Permission requested: Bash' },
      integrations: { mcp: { state: 'enabled' }, hooks: { state: 'blocked', reason: 'allowManagedHooksOnly' } },
    })
    const w = mountWith(SessionRow, { session: running })
    expect(w.text()).toContain('4 turns')
    expect(w.find('.attention').text()).toBe('Permission required')
    expect(w.find('.attention').attributes('title')).toBe('Permission requested: Bash')
    expect(w.findAll('span').find((x) => x.text() === 'hooks off')?.attributes('title')).toBe('allowManagedHooksOnly')
    expect(w.findAll('span').some((x) => x.text() === 'NEXUS tools off')).toBe(false)
    // An ended session never shows a stale attention badge.
    const ended = mountWith(SessionRow, { session: session({ attention: 'stale', state: 'exited' }) })
    expect(ended.find('.attention').exists()).toBe(false)
    // Silence is not a prompt: a quiet session shows no badge; without hooks the status is marked as unknown.
    const quiet = mountWith(SessionRow, { session: session({ running: true, state: 'running', endedAt: null, status: { status: 'running', certainty: 'assumed', since: null, detail: null } }) })
    expect(quiet.find('.attention').exists()).toBe(false)
    const blind = mountWith(SessionRow, { session: session({ running: true, state: 'running', endedAt: null, status: { status: 'unknown', certainty: 'assumed', since: null, detail: 'NEXUS hooks are off' } }) })
    expect(blind.find('.attention').text()).toBe('Unknown?')
  })

  it('a session owned by another host is shown as running without terminal controls', () => {
    seed()
    const w = mountWith(SessionRow, {
      session: session({ host: 'cli', hostId: 'other', state: 'running', endedAt: null, liveElsewhere: true, resumable: false }),
    })
    expect(w.text()).toContain('Running in a terminal (nexus launch)')
    expect(w.text()).toContain('open in its own terminal')
    expect(w.find('button[title="Terminate…"]').exists()).toBe(false)
  })
})

describe('CommandPalette', () => {
  it('filters fuzzily and runs the selected command with Enter', async () => {
    seed()
    const app = useAppStore()
    const w = mountWith(CommandPalette)
    app.paletteOpen = true
    await flushPromises()
    await w.find('.palette input').setValue('launch game')
    const first = w.findAll('.results li')[0]!
    expect(first.text()).toContain('Launch in Game')
    await w.find('.palette input').trigger('keydown', { key: 'Enter' })
    expect(app.paletteOpen).toBe(false)
    expect(app.launchDialog).toEqual({ open: true, projectId: 'p1', profileId: undefined, task: null, kickoff: undefined })
  })
})
