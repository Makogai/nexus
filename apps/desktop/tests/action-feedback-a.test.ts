// Every action shows it is working and how it ended (ADR-0048, rule NX-7): while a backend call runs, its button is
// disabled and aria-busy (no second trigger), and afterwards it is usable again with any error still visible.
// Workflows, gates, skills, knowledge documents and constitutions; IPC mocked with calls that finish when told to.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount, type DOMWrapper, type VueWrapper } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { ConstitutionStructure, ConstitutionView, Gate, KnowledgeDoc, KnowledgeItem, SkillEntry, SkillFile, TaskView, WorkflowView } from '../src/lib/types'

const api = vi.hoisted(() => ({
  workflowList: vi.fn(),
  workflowTemplates: vi.fn(),
  workflowGet: vi.fn(),
  workflowCreate: vi.fn(),
  workflowSave: vi.fn(),
  workflowSaveRaw: vi.fn(),
  workflowRuns: vi.fn(),
  workflowRun: vi.fn(),
  workflowStart: vi.fn(),
  knowledgeOverview: vi.fn(),
  knowledgeApprove: vi.fn(),
  knowledgeRead: vi.fn(),
  knowledgeTask: vi.fn(),
  knowledgeUpdate: vi.fn(),
  knowledgeUpdateTask: vi.fn(),
  sessionStartClaude: vi.fn(),
  gatesList: vi.fn(),
  gateSuggest: vi.fn(),
  gateUpsert: vi.fn(),
  gateApprove: vi.fn(),
  gateApprovalImpact: vi.fn(),
  gateRevoke: vi.fn(),
  gateRemove: vi.fn(),
  gateRun: vi.fn(),
  skillsList: vi.fn(),
  skillsRead: vi.fn(),
  skillsCreate: vi.fn(),
  skillsSave: vi.fn(),
  skillsDelete: vi.fn(),
  constitutionList: vi.fn(),
  constitutionGet: vi.fn(),
  constitutionSave: vi.fn(),
  constitutionSaveRaw: vi.fn(),
  constitutionEffective: vi.fn(),
  constitutionSearch: vi.fn(),
  constitutionHistory: vi.fn(),
  constitutionVersion: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({
  api,
  SESSION_EVENT: 'nexus://session',
  WORKFLOW_EVENT: 'nexus://workflows',
  USAGE_EVENT: 'nexus://usage',
  toIpcError: (e: unknown) => ({ code: (e as { code?: string })?.code ?? 'internal', message: String((e as { message?: string })?.message ?? e) }),
}))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn(), openUrl: vi.fn() }))
vi.mock('vue-router', () => ({ useRoute: () => ({ name: 'none', query: {} }), useRouter: () => ({ push: vi.fn() }), RouterLink: { template: '<a><slot /></a>' } }))
vi.mock('../src/router', () => ({ router: { push: vi.fn() } }))

import WorkflowsView from '../src/views/WorkflowsView.vue'
import StartWorkflowDialog from '../src/components/StartWorkflowDialog.vue'
import GatesPanel from '../src/components/GatesPanel.vue'
import SkillsView from '../src/views/SkillsView.vue'
import KnowledgeDocDialog from '../src/components/KnowledgeDocDialog.vue'
import ConstitutionsView from '../src/views/ConstitutionsView.vue'
import { useAppStore } from '../src/stores/app'
import { useProfileStore } from '../src/stores/profiles'
import { useProjectStore } from '../src/stores/projects'
import { useWorkspaceStore } from '../src/stores/workspaces'

/** A backend call that finishes only when the test says so. */
function slow<T = unknown>() {
  let resolve!: (v: T) => void
  let reject!: (e: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}
type El = DOMWrapper<Element>
function expectBusy(el: El) {
  expect(el.attributes('disabled')).toBeDefined()
  expect(el.attributes('aria-busy')).toBe('true')
}
function expectIdle(el: El) {
  expect(el.attributes('disabled')).toBeUndefined()
  expect(el.attributes('aria-busy')).toBeUndefined()
}
const find = (w: VueWrapper, sel: string) => {
  const el = w.find(sel)
  expect(el.exists(), sel).toBe(true)
  return el
}

const ModalStub = { props: ['open'], template: '<div v-if="open"><slot /><slot name="footer" /></div>' }
const EditorStub = {
  props: ['modelValue', 'readonly'],
  emits: ['update:modelValue'],
  template: `<textarea data-editor-stub :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" />`,
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useWorkspaceStore().active = { id: 'w1', name: 'W', rootPath: 'C:\\ws' } as never
  useProjectStore().list = [{ id: 'p1', name: 'shop', available: true, stack: { stacks: [] } }] as never
  useProfileStore().list = [{ id: 'work', name: 'Work' }] as never
  api.uiPreferencesSet.mockResolvedValue(undefined)
  api.knowledgeOverview.mockResolvedValue({ items: [] })
})

// ---- Workflows ----------------------------------------------------------------------------------------------------

const workflow = (over: Partial<WorkflowView> = {}): WorkflowView => ({
  id: 'wf', title: 'Feature', category: 'feature', rigor: 'balanced', skills: [], steps: 1, gates: [], approved: false, errors: 0, status: 'active',
  relativePath: 'workflows/feature.md', hash: 'h1', projectId: null, problems: [], body: '## Plan\n',
  definition: { preamble: '', steps: [{ id: 'plan', title: 'Plan', kind: 'claude', needs: [], gates: [], rigor: null, text: '', criteria: [] }] } as never,
  ...over,
})

describe('WorkflowsView', () => {
  beforeEach(() => {
    api.workflowList.mockResolvedValue([workflow()])
    api.workflowTemplates.mockResolvedValue([{ id: 'feature', title: 'Feature implementation', category: 'feature', summary: 'Plan, build, verify.' }])
    api.workflowGet.mockResolvedValue(workflow())
    api.workflowRuns.mockResolvedValue([{ id: 'r1', workflowTitle: 'Feature', goal: 'Checkout', rigor: 'balanced', status: 'running', updatedAt: Date.now() }])
  })
  const mountView = () => mount(WorkflowsView, { global: { stubs: { GatesPanel: true, StartWorkflowDialog: true, WorkflowRunDetail: true } } })

  it('marks the workflow being opened, then its approve and save buttons, while each call runs', async () => {
    const w = mountView()
    await flushPromises()
    const opening = slow<WorkflowView>()
    api.workflowGet.mockReturnValueOnce(opening.promise)
    await find(w, '[data-workflow="Feature"]').trigger('click')
    expectBusy(find(w, '[data-workflow="Feature"]'))
    opening.resolve(workflow())
    await flushPromises()
    expectIdle(find(w, '[data-workflow="Feature"]'))

    const approving = slow()
    api.knowledgeApprove.mockReturnValueOnce(approving.promise)
    await find(w, '[data-approve-workflow]').trigger('click')
    expectBusy(find(w, '[data-approve-workflow]'))
    await find(w, '[data-approve-workflow]').trigger('click')
    expect(api.knowledgeApprove).toHaveBeenCalledTimes(1)
    approving.resolve({})
    await flushPromises()
    expectIdle(find(w, '[data-approve-workflow]'))

    const saving = slow<WorkflowView>()
    api.workflowSave.mockReturnValueOnce(saving.promise)
    await find(w, '[data-save-settings]').trigger('click')
    expectBusy(find(w, '[data-save-settings]'))
    expect(find(w, '[data-save-settings]').text()).toContain('Saving…')
    saving.resolve(workflow())
    await flushPromises()
    expectIdle(find(w, '[data-save-settings]'))
  })

  it('keeps the Markdown editor and the template picker usable after a failed call, with the error shown', async () => {
    const toast = vi.spyOn(useAppStore(), 'toast')
    const w = mountView()
    await flushPromises()
    await find(w, '[data-workflow="Feature"]').trigger('click')
    await flushPromises()
    await w.findAll('[data-workflow-detail] header .btn').find((b) => b.text() === 'Markdown')!.trigger('click')
    const saving = slow<WorkflowView>()
    api.workflowSaveRaw.mockReturnValueOnce(saving.promise)
    await find(w, '[data-save-workflow]').trigger('click')
    expectBusy(find(w, '[data-save-workflow]'))
    saving.reject({ code: 'conflict', message: 'changed on disk' })
    await flushPromises()
    expectIdle(find(w, '[data-save-workflow]'))
    expect(toast).toHaveBeenCalledWith('error', 'Not saved: changed on disk')

    await find(w, '[data-new-workflow]').trigger('click')
    const creating = slow<WorkflowView>()
    api.workflowCreate.mockReturnValueOnce(creating.promise)
    await find(w, '[data-template="feature"]').trigger('click')
    expectBusy(find(w, '[data-template="feature"]'))
    creating.reject({ message: 'disk full' })
    await flushPromises()
    expectIdle(find(w, '[data-template="feature"]'))
    expect(toast).toHaveBeenCalledWith('error', 'Could not create the workflow: disk full')
  })

  it('marks the run being opened', async () => {
    const w = mountView()
    await flushPromises()
    await find(w, '[data-tab="runs"]').trigger('click')
    await flushPromises()
    const opening = slow()
    api.workflowRun.mockReturnValueOnce(opening.promise)
    await find(w, '[data-run-item="r1"]').trigger('click')
    expectBusy(find(w, '[data-run-item="r1"]'))
    opening.resolve({ id: 'r1' })
    await flushPromises()
    expectIdle(find(w, '[data-run-item="r1"]'))
  })
})

describe('StartWorkflowDialog', () => {
  it('is busy while the run starts and keeps a failure in the dialog', async () => {
    api.workflowList.mockResolvedValue([workflow({ approved: true })])
    const w = mount(StartWorkflowDialog, { props: { open: true, workspaceId: 'w1' }, global: { stubs: { teleport: true } }, attachTo: document.body })
    await flushPromises()
    await find(w, '[data-start-goal]').setValue('Add checkout')
    const starting = slow()
    api.workflowStart.mockReturnValueOnce(starting.promise)
    await find(w, '[data-start-run]').trigger('click')
    expectBusy(find(w, '[data-start-run]'))
    expect(find(w, '[data-start-run]').text()).toContain('Starting…')
    await find(w, '[data-start-run]').trigger('click')
    expect(api.workflowStart).toHaveBeenCalledTimes(1)
    starting.reject({ message: 'task not found' })
    await flushPromises()
    expectIdle(find(w, '[data-start-run]'))
    expect(find(w, '.notice-error').text()).toContain('task not found')
    w.unmount()
  })
})

// ---- Gates --------------------------------------------------------------------------------------------------------

const gate = (over: Partial<Gate> = {}): Gate => ({
  id: 'g', projectId: 'p1', name: 'test', title: 'test', program: 'npm', args: ['test'], timeoutSecs: 900, updatedAt: 0, version: 1, commandHash: 'abc', approved: true, approvedAt: 1, ...over,
})

describe('GatesPanel', () => {
  it('names the running gate while it runs and blocks other runs until it ends', async () => {
    api.gatesList.mockResolvedValue([[gate(), null], [gate({ id: 'g2', name: 'lint', program: 'npx', args: ['eslint'] }), null]])
    api.gateSuggest.mockResolvedValue([])
    const w = mount(GatesPanel, { attachTo: document.body })
    await flushPromises()
    const running = slow()
    api.gateRun.mockReturnValueOnce(running.promise)
    await find(w, '[data-run-gate="lint"]').trigger('click')
    expectBusy(find(w, '[data-run-gate="lint"]'))
    expect(find(w, '[data-run-gate="lint"]').text()).toContain('Running lint…')
    expect(find(w, '[data-gate-running]').text()).toContain('Running lint…')
    expect(find(w, '[data-run-gate="test"]').attributes('disabled')).toBeDefined()
    running.resolve({ gateName: 'lint', status: 'passed', exitCode: 0, durationMs: 1000, outputTail: 'clean', startedAt: 0 })
    await flushPromises()
    expectIdle(find(w, '[data-run-gate="lint"]'))
    expectIdle(find(w, '[data-run-gate="test"]'))
    expect(w.find('[data-gate-running]').exists()).toBe(false)
    expect(find(w, '[data-gate-output]').text()).toContain('clean')
    w.unmount()
  })

  it('is busy while a gate is saved, approved, revoked, added or removed', async () => {
    const toast = vi.spyOn(useAppStore(), 'toast')
    vi.spyOn(useAppStore(), 'confirm').mockResolvedValue(true)
    api.gatesList.mockResolvedValue([[gate(), null], [gate({ id: 'g2', name: 'lint', approved: false, approvedAt: null }), null]])
    api.gateSuggest.mockResolvedValue([{ name: 'typecheck', program: 'npx', args: ['tsc'], timeoutSecs: 900 }])
    api.gateApprovalImpact.mockResolvedValue({ steps: 0, runs: 0, stillWaiting: 0 })
    const w = mount(GatesPanel, { attachTo: document.body })
    await flushPromises()

    await find(w, '[data-gate-name]').setValue('e2e')
    await find(w, '[data-gate-command]').setValue('npm run e2e')
    const saving = slow()
    api.gateUpsert.mockReturnValueOnce(saving.promise)
    await find(w, '[data-gate-save]').trigger('click')
    expectBusy(find(w, '[data-gate-save]'))
    saving.reject({ message: 'invalid name' })
    await flushPromises()
    expectIdle(find(w, '[data-gate-save]'))
    expect((find(w, '[data-gate-name]').element as HTMLInputElement).value).toBe('e2e')
    expect(toast).toHaveBeenCalledWith('error', 'Could not save the gate: invalid name')

    const steps: [string, keyof typeof api][] = [
      ['[data-approve-gate="lint"]', 'gateApprove'],
      ['[data-revoke-gate="test"]', 'gateRevoke'],
      ['[data-add-suggest="typecheck"]', 'gateUpsert'],
      ['[data-remove-gate="test"]', 'gateRemove'],
    ]
    for (const [sel, call] of steps) {
      const pending = slow()
      api[call].mockReturnValueOnce(pending.promise)
      await find(w, sel).trigger('click')
      await flushPromises()
      expectBusy(find(w, sel))
      pending.resolve(call === 'gateUpsert' ? gate({ name: 'typecheck' }) : true)
      await flushPromises()
      expectIdle(find(w, sel))
    }
    w.unmount()
  })
})

// ---- Skills -------------------------------------------------------------------------------------------------------

const skill = (over: Partial<SkillEntry> = {}): SkillEntry => ({
  scope: 'workspace', name: 'release-notes', description: 'Draft release notes.', invokeAs: 'workspace:release-notes', projectId: null, projectName: null,
  kitId: null, kitName: null, path: 'C:\\ws\\skills\\release-notes\\SKILL.md', editable: true, status: 'approved', reaches: 'every session', ...over,
})
const skills = [skill(), skill({ scope: 'project', name: 'checkout', invokeAs: 'checkout', projectId: 'p1', projectName: 'shop', status: 'native' })]
const skillFile = (e: SkillEntry, text = 'body'): SkillFile => ({ entry: e, text, hash: 'h1' })

describe('SkillsView', () => {
  beforeEach(() => {
    api.skillsList.mockResolvedValue(skills)
    api.skillsRead.mockImplementation(async (_ws: string, r: { name: string }) => skillFile(skills.find((s) => s.name === r.name)!))
  })
  const mountView = () => mount(SkillsView, { global: { stubs: { CodeEditor: EditorStub, BaseModal: ModalStub } } })

  it('is busy while refreshing, opening and saving, and usable again after a failed save', async () => {
    const toast = vi.spyOn(useAppStore(), 'toast')
    const w = mountView()
    await flushPromises()

    const refreshing = slow<SkillEntry[]>()
    api.skillsList.mockReturnValueOnce(refreshing.promise)
    await find(w, '[data-skill-refresh]').trigger('click')
    expectBusy(find(w, '[data-skill-refresh]'))
    refreshing.resolve(skills)
    await flushPromises()
    expectIdle(find(w, '[data-skill-refresh]'))

    const opening = slow<SkillFile>()
    api.skillsRead.mockReturnValueOnce(opening.promise)
    await find(w, '[data-skill="checkout"]').trigger('click')
    expectBusy(find(w, '[data-skill="checkout"]'))
    opening.resolve(skillFile(skills[1]!))
    await flushPromises()
    expectIdle(find(w, '[data-skill="checkout"]'))

    await find(w, '[data-editor-stub]').setValue('edited')
    const saving = slow()
    api.skillsSave.mockReturnValueOnce(saving.promise)
    await find(w, '[data-skill-save]').trigger('click')
    expectBusy(find(w, '[data-skill-save]'))
    expect(find(w, '[data-skill-delete]').attributes('disabled')).toBeDefined()
    saving.reject({ code: 'internal', message: 'locked' })
    await flushPromises()
    expectIdle(find(w, '[data-skill-save]'))
    expect(toast).toHaveBeenCalledWith('error', 'Not saved: locked')
  })

  it('is busy while deleting and while creating, and keeps a create failure in the dialog', async () => {
    vi.spyOn(useAppStore(), 'confirm').mockResolvedValue(true)
    const w = mountView()
    await flushPromises()
    const deleting = slow()
    api.skillsDelete.mockReturnValueOnce(deleting.promise)
    await find(w, '[data-skill-delete]').trigger('click')
    await flushPromises()
    expectBusy(find(w, '[data-skill-delete]'))
    deleting.reject({ message: 'in use' })
    await flushPromises()
    expectIdle(find(w, '[data-skill-delete]'))

    await find(w, '[data-skill-new]').trigger('click')
    await find(w, '[data-new-name]').setValue('checkout-flow')
    await find(w, '[data-new-description]').setValue('How checkout works. Use when changing payments.')
    const creating = slow()
    api.skillsCreate.mockReturnValueOnce(creating.promise)
    await find(w, '[data-new-create]').trigger('click')
    expectBusy(find(w, '[data-new-create]'))
    expect(find(w, '[data-new-create]').text()).toContain('Creating…')
    creating.reject({ message: 'name taken' })
    await flushPromises()
    expectIdle(find(w, '[data-new-create]'))
    expect(find(w, '[data-new-error]').text()).toContain('name taken')
  })
})

// ---- Knowledge documents --------------------------------------------------------------------------------------------

const knowledgeItem = (over: Partial<KnowledgeItem> = {}): KnowledgeItem =>
  ({
    id: 'n1', workspaceId: 'w1', path: 'C:\\ws\\notes\\plan.md', kind: 'note', title: 'Plan', status: null, projectId: null, priority: null, stacks: [], applies: null,
    branch: null, commit: null, privacy: 'shared', updated: null, hash: 'h1', valid: true, error: null, conflict: null, scope: 'workspace', needsApproval: true, ...over,
  }) as KnowledgeItem
const doc = { ...knowledgeItem(), fields: {}, body: 'Ship on Tuesday.', truncated: false, approved: false, relativePath: 'notes/plan.md' } as unknown as KnowledgeDoc
const mountDoc = (item: KnowledgeItem) =>
  mount(KnowledgeDocDialog, {
    props: { item, workspaceId: 'w1' },
    global: { stubs: { teleport: true, transition: false, CodeEditor: EditorStub, UsageSummary: true, SessionRow: true } },
  })

describe('KnowledgeDocDialog', () => {
  it('is busy while approving and while saving an edit; failures stay in the dialog', async () => {
    api.knowledgeRead.mockResolvedValue(doc)
    const w = mountDoc(knowledgeItem())
    await flushPromises()
    const approving = slow()
    api.knowledgeApprove.mockReturnValueOnce(approving.promise)
    await find(w, '[data-doc-approve]').trigger('click')
    expectBusy(find(w, '[data-doc-approve]'))
    approving.reject({ message: 'version changed' })
    await flushPromises()
    expectIdle(find(w, '[data-doc-approve]'))
    expect(find(w, '.notice-error').text()).toContain('version changed')

    await find(w, '[data-doc-edit-start]').trigger('click')
    await find(w, '[data-doc-edit-title]').setValue('Release plan')
    const saving = slow()
    api.knowledgeUpdate.mockReturnValueOnce(saving.promise)
    await find(w, '[data-doc-save]').trigger('click')
    expectBusy(find(w, '[data-doc-save]'))
    expect(find(w, '[data-doc-save]').text()).toContain('Saving…')
    saving.reject({ code: 'conflict', message: 'stale' })
    await flushPromises()
    expectIdle(find(w, '[data-doc-save]'))
    expect(find(w, '.notice-error').text()).toContain('changed on disk')
  })

  it('marks the checklist busy while an item is checked and keeps a conflict visible after the reload', async () => {
    const task: TaskView = {
      id: 't1', title: 'Ship login', status: 'in-progress', priority: null, projectId: null, branch: null, commit: null, labels: [], acceptance: [],
      description: '- [ ] Email', truncated: false, checklist: [{ index: 1, text: 'Email', done: false }], recentLog: [], logEntries: 0, hash: 'h1',
    }
    api.knowledgeTask.mockResolvedValue(task)
    const w = mountDoc(knowledgeItem({ id: 't1', kind: 'task', title: 'Ship login', status: 'in-progress', needsApproval: false }))
    await flushPromises()
    const toggling = slow()
    api.knowledgeUpdateTask.mockReturnValueOnce(toggling.promise)
    await find(w, '[data-checklist] input').setValue(true)
    expect(find(w, '[data-checklist]').attributes('aria-busy')).toBe('true')
    expect(find(w, '[data-checklist] input').attributes('disabled')).toBeDefined()
    toggling.reject({ code: 'conflict', message: 'stale' })
    await flushPromises()
    expect(find(w, '[data-checklist]').attributes('aria-busy')).toBeUndefined()
    expect(find(w, '[data-checklist] input').attributes('disabled')).toBeUndefined()
    expect(find(w, '.notice-error').text()).toContain('changed on disk — reloaded, try again')
  })
})

// ---- Constitutions --------------------------------------------------------------------------------------------------

const STRUCTURE: ConstitutionStructure = {
  preamble: '',
  articles: [{ title: 'Security', text: '', sections: [{ title: '', text: '', rules: [{ id: 'TC-1', title: 'Never commit credentials', enforcement: 'mandatory', enabled: true, overrides: null, text: 'Use the store.' }] }] }],
}
const constitution = (over: Partial<ConstitutionView> = {}): ConstitutionView => ({
  id: 'c1', title: 'Team charter', scope: 'workspace', stacks: [], projectId: null, taskId: null, workflowId: null, applies: 'always', status: 'active', prefix: 'TC',
  approved: false, rules: 1, mandatory: 1, errors: 0, warnings: 0, relativePath: 'rules/c1.md', hash: 'h1', updated: '2026-11-04T10:00:00Z',
  structure: JSON.parse(JSON.stringify(STRUCTURE)), problems: [], body: '## Security\n', appliesTo: [], otherRules: [], ...over,
})

describe('ConstitutionsView', () => {
  beforeEach(() => {
    api.constitutionList.mockResolvedValue([constitution()])
    api.constitutionGet.mockResolvedValue(constitution())
    api.constitutionSearch.mockResolvedValue([])
  })

  it('is busy while refreshing, opening, approving, loading history and saving', async () => {
    const w = mount(ConstitutionsView, { attachTo: document.body })
    await flushPromises()

    const refreshing = slow()
    api.constitutionList.mockReturnValueOnce(refreshing.promise)
    await find(w, '[data-constitution-refresh]').trigger('click')
    expectBusy(find(w, '[data-constitution-refresh]'))
    refreshing.resolve([constitution()])
    await flushPromises()
    expectIdle(find(w, '[data-constitution-refresh]'))

    const opening = slow()
    api.constitutionGet.mockReturnValueOnce(opening.promise)
    await find(w, '[data-constitution="Team charter"]').trigger('click')
    expectBusy(find(w, '[data-constitution="Team charter"]'))
    opening.resolve(constitution())
    await flushPromises()
    expectIdle(find(w, '[data-constitution="Team charter"]'))

    const approving = slow()
    api.knowledgeApprove.mockReturnValueOnce(approving.promise)
    await find(w, '[data-approve]').trigger('click')
    expectBusy(find(w, '[data-approve]'))
    approving.resolve({})
    await flushPromises()
    expectIdle(find(w, '[data-approve]'))

    const history = slow()
    api.constitutionHistory.mockReturnValueOnce(history.promise)
    await find(w, '[data-mode="history"]').trigger('click')
    expectBusy(find(w, '[data-mode="history"]'))
    history.resolve({ commits: [], reason: 'Not in a Git repository.' })
    await flushPromises()
    expectIdle(find(w, '[data-mode="history"]'))

    await find(w, '[data-mode="structured"]').trigger('click')
    await find(w, '[data-rule="TC-1"] input[aria-label="Rule title"]').setValue('Never commit secrets')
    const saving = slow()
    api.constitutionSave.mockReturnValueOnce(saving.promise)
    await find(w, '[data-save]').trigger('click')
    expectBusy(find(w, '[data-save]'))
    expect(find(w, '[data-discard]').attributes('disabled')).toBeDefined()
    saving.reject({ code: 'internal', message: 'locked' })
    await flushPromises()
    expectIdle(find(w, '[data-save]'))
    expectIdle(find(w, '[data-discard]'))
    w.unmount()
  })

  it('names the gate being run while linked gates are verified', async () => {
    api.constitutionEffective.mockResolvedValue({
      rules: [{ id: 'TC-1', title: 'Never commit credentials', enforcement: 'mandatory', text: 'Use the store.', article: 'Security', section: '', docId: 'c1', docTitle: 'Team charter', scope: 'workspace', replaces: null }],
      overridden: [], conflicts: [], pending: [], documents: [],
      compliance: [{ ruleId: 'TC-1', enforcement: 'mandatory', status: 'not-run', check: 'lint', at: null }],
    })
    const w = mount(ConstitutionsView, { attachTo: document.body })
    await flushPromises()
    await find(w, '[data-tab="effective"]').trigger('click')
    await find(w, '[data-eff-project]').setValue('p1')
    await flushPromises()
    const running = slow()
    api.gateRun.mockReturnValueOnce(running.promise)
    await find(w, '[data-verify-rules]').trigger('click')
    expectBusy(find(w, '[data-verify-rules]'))
    expect(find(w, '[data-verify-rules]').text()).toContain('Running lint…')
    running.resolve({ gateName: 'lint', status: 'passed' })
    await flushPromises()
    expectIdle(find(w, '[data-verify-rules]'))
    expect(find(w, '[data-verify-rules]').text()).toContain('Verify 1 linked gate(s) now')
    w.unmount()
  })
})
