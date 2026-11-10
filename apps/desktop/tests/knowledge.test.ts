// Knowledge UI: real components + real Pinia stores; only the IPC boundary and native plugins are mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { KnowledgeItem, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  knowledgeOverview: vi.fn(),
  knowledgeRead: vi.fn(),
  knowledgeSearch: vi.fn(),
  knowledgeCreate: vi.fn(),
  knowledgeUpdateTask: vi.fn(),
  knowledgeApprove: vi.fn(),
  knowledgeTask: vi.fn(),
  knowledgeProposals: vi.fn((): Promise<unknown[]> => Promise.resolve([])),
  knowledgeProposalApprove: vi.fn(),
  knowledgeProposalReject: vi.fn(),
  instructionsInspect: vi.fn(),
  projectResume: vi.fn(),
  sessionResume: vi.fn(),
}))

vi.mock('../src/lib/ipc', () => ({
  api,
  SESSION_EVENT: 'nexus://session',
  toIpcError: (e: unknown) => {
    const o = e as { code?: string; message?: string }
    return { code: o?.code ?? 'internal', message: String(o?.message ?? e) }
  },
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn(), openUrl: vi.fn() }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))

import { boardColumns, problems, rulesInOrder } from '../src/lib/knowledge'
import TaskBoard from '../src/components/TaskBoard.vue'
import KnowledgeDialog from '../src/components/KnowledgeDialog.vue'
import KnowledgeView from '../src/views/KnowledgeView.vue'
import KnowledgeDocDialog from '../src/components/KnowledgeDocDialog.vue'
import InstructionsPanel from '../src/components/InstructionsPanel.vue'
import { useProfileStore } from '../src/stores/profiles'
import type { ClaudeProfile, InstructionsPreview, ResumeSummary, TaskView } from '../src/lib/types'
import ResumeProjectView from '../src/views/ResumeProjectView.vue'
import { createMemoryHistory, createRouter } from 'vue-router'
import { useAppStore } from '../src/stores/app'
import { useWorkspaceStore } from '../src/stores/workspaces'

function item(title: string, over: Partial<KnowledgeItem> = {}): KnowledgeItem {
  return {
    id: `id-${title}`,
    workspaceId: 'ws',
    path: `/ws/${title}.md`,
    kind: 'task',
    title,
    status: 'backlog',
    projectId: null,
    priority: null,
    stacks: [],
    applies: null,
    branch: null,
    commit: null,
    privacy: 'shared',
    updated: '2026-01-01T00:00:00Z',
    hash: `h-${title}`,
    valid: true,
    error: null,
    conflict: null,
    scope: 'workspace',
    needsApproval: false,
    ...over,
  }
}

const WS = { id: 'ws', name: 'Game studio', available: true } as unknown as Workspace

describe('knowledge helpers', () => {
  it('groups tasks into columns by status and priority, skipping broken files', () => {
    const cols = boardColumns([
      item('a', { priority: 'low' }),
      item('b', { priority: 'urgent' }),
      item('c', { status: 'done' }),
      item('d', { status: 'nonsense' }),
      item('e', { valid: false }),
      item('f', { conflict: 'dup' }),
      item('r', { kind: 'rule' }),
    ])
    expect(cols.backlog.map((t) => t.title)).toEqual(['b', 'd', 'a'])
    expect(cols.done.map((t) => t.title)).toEqual(['c'])
    expect(Object.values(cols).flat()).toHaveLength(4)
  })

  it('orders rules like a launch: global, workspace, stack, project', () => {
    const rules = rulesInOrder([
      item('p', { kind: 'rule', scope: 'project' }),
      item('s', { kind: 'rule', scope: 'stack' }),
      item('g', { kind: 'rule', scope: 'global' }),
      item('w', { kind: 'rule', scope: 'workspace' }),
      item('t'),
    ])
    expect(rules.map((r) => r.title)).toEqual(['g', 'w', 's', 'p'])
    expect(problems([item('x', { valid: false }), item('y')]).map((p) => p.title)).toEqual(['x'])
  })
})

describe('TaskBoard', () => {
  it('moves a task through its status select', async () => {
    const w = mount(TaskBoard, { props: { tasks: [item('Ship login')] } })
    expect(w.find('[data-status="backlog"]').text()).toContain('Ship login')
    await w.find('select').setValue('in-progress')
    expect(w.emitted('move')?.[0]?.[1]).toBe('in-progress')
  })
})

describe('KnowledgeDialog', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('creates a global always-on rule without a project', async () => {
    api.knowledgeCreate.mockResolvedValue(item('Honesty', { kind: 'rule' }))
    const w = mount(KnowledgeDialog, {
      props: { open: false, workspaceId: 'ws', projects: [], initialKind: 'rule' },
      global: { stubs: { teleport: true, transition: false } },
    })
    await w.setProps({ open: true })
    await w.find('#k-where').setValue('global')
    await w.find('#k-title').setValue('Honesty')
    await w.find('#k-applies').setValue('always')
    expect(w.find('#k-project').exists()).toBe(false)
    expect(w.find('#k-stacks').exists()).toBe(false)
    await w.find('.btn-primary').trigger('click')
    await flushPromises()
    expect(api.knowledgeCreate).toHaveBeenCalledWith('ws', expect.objectContaining({ kind: 'rule', title: 'Honesty', applies: 'always', global: true, projectId: null, localOnly: false }))
    expect(w.emitted('created')).toBeTruthy()
  })
})

describe('KnowledgeView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    useWorkspaceStore().active = WS
  })

  function mountView() {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:p(.*)*', component: { template: '<div />' } }] })
    return mount(KnowledgeView, { global: { plugins: [router], stubs: { teleport: true, transition: false } } })
  }

  it('shows pending approvals and approves only after the rule was opened', async () => {
    const rule = item('Synced rule', { kind: 'rule', applies: 'always', needsApproval: true })
    api.knowledgeOverview.mockResolvedValue({ report: { scanned: 2, changed: 0, removed: 0, invalid: 0, conflicts: 0 }, items: [rule, item('Task one')] })
    api.knowledgeRead.mockResolvedValue({ ...rule, fields: { applies: 'always' }, body: 'Arrived through git.', truncated: false, approved: false, relativePath: 'rules/x.md' })
    api.knowledgeApprove.mockResolvedValue(rule)
    const w = mountView()
    await flushPromises()
    const banner = w.find('.approvals')
    expect(banner.text()).toContain('1 always-on rule')
    await banner.find('button').trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Arrived through git.')
    const approve = w.findAll('button').find((b) => b.text().includes('Approve this version'))!
    await approve.trigger('click')
    await flushPromises()
    expect(api.knowledgeApprove).toHaveBeenCalledWith('ws', 'id-Synced rule')
    expect(api.knowledgeOverview).toHaveBeenCalledTimes(2)
  })

  it('moves tasks with the content hash and reports a concurrent edit', async () => {
    api.knowledgeOverview.mockResolvedValue({ report: { scanned: 1, changed: 0, removed: 0, invalid: 0, conflicts: 0 }, items: [item('Ship login')] })
    api.knowledgeUpdateTask.mockRejectedValue({ code: 'conflict', message: 'changed' })
    const w = mountView()
    await flushPromises()
    await w.find('[data-status="backlog"] select').setValue('done')
    await flushPromises()
    expect(api.knowledgeUpdateTask).toHaveBeenCalledWith('ws', 'id-Ship login', { status: 'done', expectedHash: 'h-Ship login' })
    expect(useAppStore().toasts.at(-1)?.message).toContain('changed on disk')
  })

  it('lists broken files under Problems and never as documents', async () => {
    api.knowledgeOverview.mockResolvedValue({
      report: { scanned: 2, changed: 0, removed: 0, invalid: 1, conflicts: 0 },
      items: [item('broken', { valid: false, error: '`id` is required', kind: null }), item('Decide', { kind: 'decision', status: 'proposed' })],
    })
    const w = mountView()
    await flushPromises()
    await w.find('[data-tab="decision"]').trigger('click')
    expect(w.findAll('.doc').map((d) => d.text())).toEqual([expect.stringContaining('Decide')])
    await w.find('[data-tab="health"]').trigger('click')
    expect(w.text()).toContain('`id` is required')
  })
})

describe('task workflow UI', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  it('offers Launch on open tasks only', async () => {
    const w = mount(TaskBoard, { props: { tasks: [item('Open'), item('Finished', { status: 'done' })] } })
    expect(w.findAll('.launch')).toHaveLength(1)
    await w.find('.launch').trigger('click')
    expect(w.emitted('launch')?.[0]?.[0]).toMatchObject({ title: 'Open' })
  })

  it('task dialog toggles checklist items with the content hash and offers Launch', async () => {
    const view: TaskView = {
      id: 'id-Ship login', title: 'Ship login', status: 'in-progress', priority: 'high', projectId: null, branch: 'feature/login',
      commit: null, labels: [], acceptance: [], description: 'Form.\n- [ ] Email\n- [x] Password', truncated: false,
      checklist: [{ index: 1, text: 'Email', done: false }, { index: 2, text: 'Password', done: true }],
      recentLog: ['2026-11-04 [claude] Form started'], logEntries: 1, hash: 'h1',
    }
    api.knowledgeTask.mockResolvedValue(view)
    api.knowledgeUpdateTask.mockResolvedValue(item('Ship login'))
    const w = mount(KnowledgeDocDialog, {
      props: { item: item('Ship login', { status: 'in-progress' }), workspaceId: 'ws' },
      global: { stubs: { teleport: true, transition: false, RouterLink: true } },
    })
    await flushPromises()
    expect(w.text()).toContain('[claude] Form started')
    expect(w.text()).toContain('feature/login')
    const boxes = w.findAll('[data-checklist] input')
    expect(boxes).toHaveLength(2)
    await boxes[0]!.setValue(true)
    await flushPromises()
    expect(api.knowledgeUpdateTask).toHaveBeenCalledWith('ws', 'id-Ship login', { check: [1], uncheck: [], expectedHash: 'h1' })
    expect(w.emitted('changed')).toBeTruthy()
    await w.find('[data-launch]').trigger('click')
    expect(w.emitted('launch')?.[0]?.[0]).toMatchObject({ title: 'Ship login' })
  })

  it('an open document follows changes on disk; an edit keeps its draft and is told (ADR-0049)', async () => {
    const EditorStub = { props: ['modelValue'], template: `<textarea data-editor-stub :value="modelValue" />` }
    const v1 = { ...item('Plan', { kind: 'note', hash: 'h1' }), fields: {}, body: 'Ship on Tuesday.', truncated: false, approved: true, relativePath: 'knowledge/plan.md' }
    api.knowledgeRead.mockResolvedValue(v1)
    const w = mount(KnowledgeDocDialog, {
      props: { item: item('Plan', { kind: 'note' }), workspaceId: 'ws' },
      global: { stubs: { teleport: true, transition: false, RouterLink: true, CodeEditor: EditorStub, UsageSummary: true } },
    })
    await flushPromises()
    expect(w.text()).toContain('Ship on Tuesday.')
    const app = useAppStore()

    app.freshness++ // nothing changed: the same version stays
    await flushPromises()
    expect(api.knowledgeRead).toHaveBeenCalledTimes(2)
    api.knowledgeRead.mockResolvedValue({ ...v1, hash: 'h2', body: 'Ship on Wednesday.' })
    app.freshness++
    await flushPromises()
    expect(w.text()).toContain('Ship on Wednesday.')
    expect(w.find('[data-doc-changed-on-disk]').exists()).toBe(false)

    await w.find('[data-doc-edit-start]').trigger('click')
    await flushPromises()
    expect((w.find('[data-editor-stub]').element as HTMLTextAreaElement).value).toBe('Ship on Wednesday.')
    api.knowledgeRead.mockResolvedValue({ ...v1, hash: 'h3', body: 'Ship on Thursday.' })
    app.freshness++
    await flushPromises()
    expect(w.find('[data-doc-changed-on-disk]').text()).toContain('changed on disk')
    expect((w.find('[data-editor-stub]').element as HTMLTextAreaElement).value).toBe('Ship on Wednesday.')

    await w.findAll('button').find((b) => b.text() === 'Cancel')!.trigger('click')
    await flushPromises()
    expect(w.text()).toContain('Ship on Thursday.')
    expect(w.find('[data-doc-changed-on-disk]').exists()).toBe(false)
  })

  it('instructions panel lists Claude’s own layers in order and what NEXUS adds', async () => {
    useProfileStore().list = [{ id: 'p', name: 'Personal' } as ClaudeProfile]
    const preview: InstructionsPreview = {
      claude: {
        layers: [
          { scope: 'user', kind: 'claude-md', path: '/cfg/CLAUDE.md', load: 'at-start', reason: null, chars: 10, lines: 1, importedFrom: null, paths: [] },
          { scope: 'project', kind: 'claude-md', path: '/home/.claude/CLAUDE.md', load: 'at-start', reason: "the default profile's CLAUDE.md — loaded as project memory", chars: 5, lines: 1, importedFrom: null, paths: [] },
          { scope: 'project', kind: 'rule', path: '/repo/.claude/rules/rust.md', load: 'on-demand', reason: null, chars: 3, lines: 1, importedFrom: null, paths: ['src/**/*.rs'] },
          { scope: 'local', kind: 'claude-md', path: '/repo/CLAUDE.local.md', load: 'excluded', reason: 'claudeMdExcludes: **/CLAUDE.local.md', chars: 2, lines: 1, importedFrom: null, paths: [] },
        ],
        filesAtStart: 2, charsAtStart: 15, excludes: [], agentsMode: 'claude-md-or-agents-md', disabled: false,
        warnings: ['/home/.claude/CLAUDE.md belongs to the default Claude profile but also reaches this profile’s sessions'],
      },
      nexus: {
        workProfile: 'Balanced', workProfileChars: 100, task: null, taskBriefChars: 0, appendedChars: 160, mcp: null, hooks: null, isolationExcludes: ['/home/.claude/CLAUDE.md'],
        rules: { included: ['Tests'], pendingApproval: [], overBudget: [], alreadyLoaded: ['Small commits (in CLAUDE.md)'] },
      },
    }
    api.instructionsInspect.mockResolvedValue(preview)
    const w = mount(InstructionsPanel, { props: { projectId: 'proj' } })
    await flushPromises()
    expect(api.instructionsInspect).toHaveBeenCalledWith({ profileId: 'p', projectId: 'proj' })
    expect(w.findAll('.layer').map((l) => l.attributes('data-load'))).toEqual(['at-start', 'at-start', 'on-demand', 'excluded'])
    expect(w.text()).toContain('when Claude touches src/**/*.rs')
    expect(w.find('.notice-warn').text()).toContain('default Claude profile')
    expect(w.find('[data-already]').text()).toContain('Small commits (in CLAUDE.md)')
    expect(w.find('[data-isolation-on]').text()).toContain('1 pattern(s)')
  })
})

describe('ResumeProjectView', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
  })

  const summary: ResumeSummary = {
    projectId: 'p1',
    projectName: 'shop',
    verified: {
      checkedAt: Date.now(), available: true, repo: true, branch: 'main', detached: false,
      head: { oid: 'abcdef123456', summary: 'Add cart', at: Date.now() }, upstream: 'origin/main', ahead: 0, behind: 2,
      staged: 0, unstaged: 1, untracked: 0, conflicted: 0, recentCommits: [{ oid: 'abcdef123456', summary: 'Add cart', at: Date.now() }],
    },
    saved: {
      tasks: [{ id: 't1', title: 'Ship checkout', status: 'in-progress', priority: null, branch: null, checklistDone: 1, checklistTotal: 2, lastProgress: '2026-11-04 [claude] Cart done', updated: null }],
      notes: [{ id: 'n1', kind: 'note', title: 'Sandbox keys', status: null, relevance: 'this-branch', branch: 'main', updated: null }],
      decisions: [],
      checkpoints: [{ kind: 'progress', summary: 'Cart done', sessionTitle: 'Ship checkout', at: Date.now(), branch: 'feature/old', otherBranch: true }],
      sessions: [{ id: 's1', title: 'Ship checkout', state: 'interrupted', startedAt: Date.now(), endedAt: null, resumable: true, taskId: 't1', branch: 'main' }],
      hiddenOtherBranch: 1,
      pendingProposals: 2,
    },
    next: { kind: 'resume-session', label: 'Resume work on “Ship checkout”', evidence: ['saved: task “Ship checkout” is in-progress', 'saved: its last session can be resumed'], sessionId: 's1', taskId: 't1' },
    warnings: ['Some checkpoints were recorded on another branch than the one checked out now.'],
  }

  function mountView() {
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:p(.*)*', component: { template: '<div />' } }] })
    return mount(ResumeProjectView, { props: { id: 'p1' }, global: { plugins: [router], stubs: { teleport: true } } })
  }

  it('keeps verified Git state apart from saved history and resumes the recommended session', async () => {
    api.projectResume.mockResolvedValue(summary)
    api.sessionResume.mockResolvedValue({ id: 's2', running: true, state: 'running' })
    const w = mountView()
    await flushPromises()
    const verified = w.find('[data-section="verified"]').text()
    const saved = w.find('[data-section="saved"]').text()
    expect(verified).toContain('main')
    expect(verified).toContain('2 behind')
    expect(verified).not.toContain('Ship checkout')
    expect(saved).toContain('Ship checkout')
    expect(saved).toContain('recorded on feature/old')
    expect(saved).toContain('1 note(s) of other branches hidden')
    expect(w.findAll('.evidence .badge').map((b) => b.text())).toEqual(['saved', 'saved'])
    expect(w.text()).toContain('recorded on another branch')
    expect(w.find('[data-pending-proposals]').text()).toContain('2 memory proposal(s)')
    await w.find('[data-next-action]').trigger('click')
    await flushPromises()
    expect(api.sessionResume).toHaveBeenCalledWith('s1', expect.anything(), expect.anything())
  })

  it('launches the recommended task through the task launch dialog', async () => {
    api.projectResume.mockResolvedValue({ ...summary, next: { ...summary.next, kind: 'launch-task', sessionId: null, label: 'Continue “Ship checkout”' } })
    const w = mountView()
    await flushPromises()
    await w.find('[data-next-action]').trigger('click')
    expect(useAppStore().launchDialog).toMatchObject({ open: true, projectId: 'p1', task: { id: 't1', title: 'Ship checkout' } })
  })
})

describe('memory proposals', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    vi.clearAllMocks()
    useWorkspaceStore().active = WS
  })

  it('are reviewed in the Knowledge view: edited, then approved — or rejected', async () => {
    const proposal = {
      id: 'mp1', workspaceId: 'ws', projectId: 'p1', sessionId: 's1', sessionTitle: 'Payments spike', kind: 'decision' as const,
      title: 'Use Stripe Checkout', body: 'It handles SCA.', source: 'decision', branch: 'main', status: 'pending' as const,
      documentId: null, createdAt: Date.now(), decidedAt: null,
    }
    api.knowledgeOverview.mockResolvedValue({ report: { scanned: 0, changed: 0, removed: 0, invalid: 0, conflicts: 0 }, items: [] })
    api.knowledgeProposals.mockResolvedValue([proposal])
    api.knowledgeProposalApprove.mockResolvedValue(item('Payments use Stripe Checkout', { kind: 'decision' }))
    const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:p(.*)*', component: { template: '<div />' } }] })
    await router.push('/knowledge?tab=proposals')
    const w = mount(KnowledgeView, { global: { plugins: [router], stubs: { teleport: true, transition: false } } })
    await flushPromises()
    expect(w.find('[data-proposals]').text()).toContain('not knowledge')
    const card = w.find('[data-proposal="mp1"]')
    await card.find('input.title').setValue('Payments use Stripe Checkout')
    await card.find('select').setValue('branch')
    await card.find('[data-approve]').trigger('click')
    await flushPromises()
    expect(api.knowledgeProposalApprove).toHaveBeenCalledWith('ws', 'mp1', { title: 'Payments use Stripe Checkout', body: 'It handles SCA.', scope: 'branch', localOnly: false })
    await w.find('[data-proposal="mp1"] [data-reject]').trigger('click')
    await flushPromises()
    expect(api.knowledgeProposalReject).toHaveBeenCalledWith('mp1')
  })
})

