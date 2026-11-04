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
}))

vi.mock('../src/lib/ipc', () => ({
  api,
  toIpcError: (e: unknown) => {
    const o = e as { code?: string; message?: string }
    return { code: o?.code ?? 'internal', message: String(o?.message ?? e) }
  },
}))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn() }))

import { boardColumns, problems, rulesInOrder } from '../src/lib/knowledge'
import TaskBoard from '../src/components/TaskBoard.vue'
import KnowledgeDialog from '../src/components/KnowledgeDialog.vue'
import KnowledgeView from '../src/views/KnowledgeView.vue'
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
    return mount(KnowledgeView, { global: { stubs: { teleport: true, transition: false } } })
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
    expect(api.knowledgeUpdateTask).toHaveBeenCalledWith('ws', 'id-Ship login', 'done', null, 'h-Ship login')
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
