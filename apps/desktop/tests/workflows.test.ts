// Workflow Manager (ADR-0029): helpers and components with real Pinia stores; IPC mocked.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { RunStep, RunView, WorkflowSummary } from '../src/lib/types'
import { includedSteps, splitArgs, verificationLabel } from '../src/lib/workflow'

const api = vi.hoisted(() => ({
  workflowList: vi.fn(),
  workflowStart: vi.fn(),
  workflowReview: vi.fn(),
  workflowRetry: vi.fn(),
  workflowCancel: vi.fn(),
  knowledgeOverview: vi.fn(),
  sessionStartClaude: vi.fn(),
  gatesList: vi.fn(),
  gateSuggest: vi.fn(),
  gateUpsert: vi.fn(),
  gateApprove: vi.fn(),
  gateRevoke: vi.fn(),
  gateRun: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
vi.mock('../src/lib/ipc', () => ({ api, SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String((e as { message?: string })?.message ?? e) }) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))

import WorkflowRunDetail from '../src/components/WorkflowRunDetail.vue'
import StartWorkflowDialog from '../src/components/StartWorkflowDialog.vue'
import GatesPanel from '../src/components/GatesPanel.vue'
import ConfirmDialog from '../src/components/ConfirmDialog.vue'
import { useProfileStore } from '../src/stores/profiles'
import { useProjectStore } from '../src/stores/projects'
import { useSessionStore } from '../src/stores/sessions'

function step(over: Partial<RunStep>): RunStep {
  return {
    stepId: 's', position: 1, kind: 'claude', status: 'pending', verification: null, attempts: 0, maxAttempts: 3, report: null,
    criteriaMet: [], lastFailure: null, updatedAt: 0, blockedOn: null, title: 'Step', text: '', needs: [], gates: [], criteria: [], results: [], ...over,
  }
}
function run(steps: RunStep[], over: Partial<RunView> = {}): RunView {
  return {
    id: 'r1', workspaceId: 'w1', projectId: 'p1', workflowId: 'wf', workflowTitle: 'Feature', workflowHash: 'h', taskId: 't1', goal: 'Add checkout',
    rigor: 'balanced', profileId: 'work', skills: [], status: 'waiting', error: null, createdAt: Date.now(), updatedAt: Date.now(), endedAt: null,
    steps, progress: { total: steps.length, done: 2, verified: 1, aiReported: 1, human: 0, skipped: 1 }, ...over,
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  useProjectStore().list = [{ id: 'p1', name: 'shop', stack: { stacks: [] } }] as never
  useProfileStore().list = [{ id: 'work', name: 'Work' }] as never
  api.uiPreferencesSet.mockResolvedValue(undefined)
})

describe('workflow helpers', () => {
  it('splits commands without a shell and labels how a step was verified', () => {
    expect(splitArgs(`npm run "test:unit" --silent ''`)).toEqual(['npm', 'run', 'test:unit', '--silent', ''])
    expect(verificationLabel({ status: 'passed', verification: 'verified' })?.text).toBe('Verified by gates')
    expect(verificationLabel({ status: 'passed', verification: 'ai-reported' })?.text).toBe('AI-reported — not verified')
    expect(verificationLabel({ status: 'passed', verification: 'human' })?.text).toBe('Signed off by you')
    expect(verificationLabel({ status: 'ready', verification: null })).toBeNull()
    const d = { preamble: '', steps: [{ id: 'a', rigor: null }, { id: 'b', rigor: 'professional' }, { id: 'c', rigor: 'critical' }] as never }
    expect(includedSteps(d, 'professional')).toEqual(['a', 'b'])
  })
})

describe('WorkflowRunDetail', () => {
  it('distinguishes verified, AI-reported and skipped steps and handles reviews and retries', async () => {
    api.workflowReview.mockResolvedValue(run([]))
    api.workflowRetry.mockResolvedValue(run([]))
    const v = run([
      step({ stepId: 'plan', status: 'passed', verification: 'ai-reported', report: 'Plan recorded' }),
      step({ stepId: 'impl', status: 'passed', verification: 'verified', gates: ['test'], results: [{ id: 'g1', projectId: 'p1', runId: 'r1', stepId: 'impl', gateName: 'test', command: 'npm test', status: 'passed', exitCode: 0, durationMs: 1200, gateVersion: 1, outputTail: 'ok 12 tests', startedAt: Date.now(), endedAt: Date.now() }] }),
      step({ stepId: 'docs', status: 'skipped', verification: 'skipped' }),
      step({ stepId: 'checks', kind: 'check', status: 'blocked', lastFailure: 'gate lint failed (exit 1)' }),
      step({ stepId: 'review', kind: 'review', status: 'awaiting-review' }),
    ])
    const w = mount(WorkflowRunDetail, { props: { run: v }, attachTo: document.body })
    expect(w.find('[data-step="plan"] [data-verification]').text()).toBe('AI-reported — not verified')
    expect(w.find('[data-step="impl"] [data-verification]').text()).toBe('Verified by gates')
    expect(w.find('[data-step="docs"] [data-verification]').text()).toBe('Skipped (rigor)')
    expect(w.find('[data-progress]').text()).toContain('1 verified by gates')
    await w.find('[data-step="impl"] [data-gate-result="test"] .btn').trigger('click')
    expect(w.find('[data-step="impl"] .output').text()).toBe('ok 12 tests')
    await w.find('[data-step="review"] input').setValue('Looks good')
    await w.find('[data-approve-step]').trigger('click')
    await flushPromises()
    expect(api.workflowReview).toHaveBeenCalledWith('r1', 'review', true, 'Looks good')
    await w.find('[data-step="checks"] [data-retry-step]').trigger('click')
    await flushPromises()
    expect(api.workflowRetry).toHaveBeenCalledWith('r1', 'checks')
  })
})

describe('StartWorkflowDialog', () => {
  it('starts a run and launches Claude for its task at the run’s rigor', async () => {
    const wf: WorkflowSummary = { id: 'wf', title: 'Feature', category: 'feature', rigor: 'professional', skills: ['team:tdd'], steps: 6, gates: ['test'], approved: true, errors: 0, status: 'active', relativePath: 'workflows/x.md', hash: 'h', projectId: null }
    api.workflowList.mockResolvedValue([wf])
    api.knowledgeOverview.mockResolvedValue({ items: [] })
    api.workflowStart.mockResolvedValue(run([], { rigor: 'professional', taskId: 't9', workflowTitle: 'Feature', goal: 'Add checkout' }))
    api.sessionStartClaude.mockResolvedValue({ id: 's1', running: true, kind: 'claude' })
    const w = mount(StartWorkflowDialog, { props: { open: true, workspaceId: 'w1' }, global: { stubs: { teleport: true } }, attachTo: document.body })
    await flushPromises()
    expect((w.find('[data-start-rigor]').element as HTMLSelectElement).value).toBe('professional')
    expect(w.text()).toContain('skills for this session only: team:tdd')
    await w.find('[data-start-goal]').setValue('Add checkout')
    await w.find('[data-start-run]').trigger('click')
    await flushPromises()
    expect(api.workflowStart).toHaveBeenCalledWith('w1', expect.objectContaining({ workflowId: 'wf', projectId: 'p1', taskId: null, goal: 'Add checkout', rigor: 'professional', profileId: 'work' }))
    expect(api.sessionStartClaude).toHaveBeenCalledWith(expect.objectContaining({ profileId: 'work', taskId: 't9', workProfile: 'professional' }))
    expect(useSessionStore().activeTab).toBe('s1')
  })
})

describe('GatesPanel', () => {
  it('defines gates as argument vectors and runs them on request', async () => {
    api.gatesList.mockResolvedValue([])
    api.gateSuggest.mockResolvedValue([{ name: 'test', program: 'npm', args: ['test'], timeoutSecs: 900 }])
    api.gateUpsert.mockImplementation(async (_p, g) => ({ id: 'g', projectId: 'p1', updatedAt: 0, title: g.name, timeoutSecs: 900, ...g }))
    const w = mount(GatesPanel, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-suggest="test"]').text()).toContain('npm test')
    await w.find('[data-gate-name]').setValue('lint')
    await w.find('[data-gate-command]').setValue('npx eslint "src dir" --max-warnings 0')
    await w.find('[data-gate-save]').trigger('click')
    await flushPromises()
    expect(api.gateUpsert).toHaveBeenCalledWith('p1', { name: 'lint', program: 'npx', args: ['eslint', 'src dir', '--max-warnings', '0'], timeoutSecs: 900 })
  })

  it('runs a gate only after its exact command version is approved (separate from workflows)', async () => {
    const gate = { id: 'g', projectId: 'p1', name: 'test', title: 'test', program: 'npm', args: ['test'], timeoutSecs: 900, updatedAt: 0, version: 2, commandHash: 'abc', approved: false, approvedAt: null }
    api.gatesList.mockResolvedValue([[gate, null]])
    api.gateSuggest.mockResolvedValue([])
    api.gateApprove.mockResolvedValue({ ...gate, approved: true, approvedAt: 1 })
    const w = mount(GatesPanel, { attachTo: document.body })
    const dialog = mount(ConfirmDialog, { attachTo: document.body })
    await flushPromises()
    expect(w.find('[data-gate="test"]').text()).toContain('v2 needs approval')
    expect(w.find('[data-run-gate="test"]').attributes('disabled')).toBeDefined()
    await w.find('[data-approve-gate="test"]').trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-confirm-message]')?.textContent).toContain('npm test')
    ;(document.querySelector('[data-confirm-ok]') as HTMLButtonElement).click()
    await flushPromises()
    expect(api.gateApprove).toHaveBeenCalledWith('p1', 'test', 'abc')
    dialog.unmount()
    w.unmount()
  })
})
