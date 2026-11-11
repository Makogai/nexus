// Project page (ADR-0048): eight tabs kept in `?tab=`, counts in the tab labels (running/total sessions with live and
// attention dots, open tasks and those in review), Ctrl+1…8, and the "Needs you" strip on the Overview.
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { KnowledgeItem, NeedsYouItem, Project, ProjectKnowledge, SessionView, Workspace } from '../src/lib/types'

const api = vi.hoisted(() => ({
  pushRequestOpen: vi.fn(),
  pushRequestDismiss: vi.fn(),
  needsYou: vi.fn(),
  projectKnowledge: vi.fn(),
  projectCheckpoints: vi.fn(),
  projectGitStatus: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
const route = vi.hoisted(() => ({ name: 'project', params: { id: 'p1' }, query: {} as Record<string, string> }))
const router = vi.hoisted(() => ({ push: vi.fn(), replace: vi.fn() }))
vi.mock('../src/lib/ipc', () => ({ api, SESSION_EVENT: 'nexus://session', toIpcError: (e: unknown) => ({ code: 'internal', message: String(e) }) }))
vi.mock('vue-router', () => ({ useRoute: () => route, useRouter: () => router, RouterLink: { template: '<a><slot /></a>' } }))
vi.mock('@tauri-apps/plugin-opener', () => ({ revealItemInDir: vi.fn() }))
vi.mock('../src/router', () => ({ router }))

import ProjectDetailView from '../src/views/ProjectDetailView.vue'
import { useProjectStore } from '../src/stores/projects'
import { useSessionStore } from '../src/stores/sessions'
import { useWorkspaceStore } from '../src/stores/workspaces'

const stub = (name: string) => ({ template: `<div data-stub="${name}" />` })
const stubs = {
  ProjectSessions: stub('sessions'),
  ProjectTasks: stub('tasks'),
  ProjectLearn: stub('learn'),
  ProjectFiles: stub('files'),
  ProjectCommits: stub('commits'),
  UsageSummary: stub('usage'),
  InstructionsPanel: stub('instructions'),
  GitSummary: stub('git'),
}

const project: Project = { id: 'p1', name: 'anilist', localPath: 'D:\\anilist', available: true, stack: { stacks: [], packageManagers: [], scripts: [], hasClaudeMd: false, hasAgentsMd: false, hasClaudeDir: false, hasReadme: false } } as unknown as Project
const task = (id: string, status: string) => ({ id, title: id, status, path: `${id}.md`, kind: 'task' }) as unknown as KnowledgeItem
const knowledge: ProjectKnowledge = { git: { branch: 'main', head: null }, tasks: [task('a', 'review'), task('b', 'in-progress'), task('c', 'review')], doneTasks: [], notes: [], hiddenOtherBranch: 0 }
const session = (id: string, running: boolean, status = 'running') => ({ id, projectId: 'p1', kind: 'claude', title: id, running, liveElsewhere: false, archivedAt: null, startedAt: 1, status: { status, certainty: 'reported', since: null, detail: null } }) as unknown as SessionView
const needs: NeedsYouItem[] = [{ kind: 'task-review', id: 'a', title: 'Fix login', detail: 'ready for review', projectId: 'p1', projectName: 'anilist', docKind: null, since: null }]

function mountPage(tab?: string) {
  route.query = tab ? { tab } : {}
  return mount(ProjectDetailView, { props: { id: 'p1' }, global: { stubs } })
}

beforeEach(() => {
  setActivePinia(createPinia())
  Object.values(api).forEach((f) => f.mockReset())
  router.replace.mockReset()
  api.pushRequestOpen.mockResolvedValue([])
  api.needsYou.mockResolvedValue(needs)
  api.projectKnowledge.mockResolvedValue(knowledge)
  api.projectCheckpoints.mockResolvedValue([])
  api.projectGitStatus.mockResolvedValue({ isRepo: true, branch: 'main', conflicted: 0, ahead: 0, behind: 0, staged: 0, unstaged: 0, untracked: 0, upstream: null, lastCommit: null, operation: null })
  useProjectStore().list = [project]
  useWorkspaceStore().active = { id: 'w1', compat: { writable: true } } as unknown as Workspace
  useSessionStore().list = [session('s1', true, 'waiting-for-input'), session('s2', false, 'completed'), session('s3', false, 'completed')]
})

describe('project page tabs', () => {
  it('opens the tab named in ?tab= and falls back to the Overview', async () => {
    let w = mountPage('sessions')
    await flushPromises()
    expect(w.find('[data-stub="sessions"]').exists()).toBe(true)
    expect(w.find('[data-project-tab="sessions"]').attributes('aria-selected')).toBe('true')
    w.unmount()
    w = mountPage('nonsense')
    await flushPromises()
    expect(w.find('[data-project-tab="overview"]').attributes('aria-selected')).toBe('true')
    expect(w.find('[data-needs-you-item="task-review"]').text()).toContain('Fix login')
    w.unmount()
  })

  it('shows counts in the tab labels: running/total sessions with live and attention dots, open and review tasks', async () => {
    const w = mountPage()
    await flushPromises()
    expect(w.find('[data-tab-count="sessions"]').text()).toBe('1/3')
    expect(w.find('[data-project-tab="sessions"] .tab-dot.live').exists()).toBe(true)
    expect(w.find('[data-tab-attention]').exists()).toBe(true)
    expect(w.find('[data-tab-count="tasks"]').text()).toBe('3')
    expect(w.find('[data-tab-review]').text()).toBe('2 review')
    w.unmount()
  })

  it('switches tabs by click and with Ctrl+1…8, keeping the tab in the URL', async () => {
    const w = mountPage()
    await flushPromises()
    await w.find('[data-project-tab="tasks"]').trigger('click')
    expect(router.replace).toHaveBeenLastCalledWith({ query: { tab: 'tasks' } })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '8', ctrlKey: true }))
    expect(router.replace).toHaveBeenLastCalledWith({ query: { tab: 'settings' } })
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '1', ctrlKey: true }))
    expect(router.replace).toHaveBeenLastCalledWith({ query: { tab: undefined } })
    w.unmount()
    // Gone with the view: no tab switching from another page.
    router.replace.mockReset()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: '2', ctrlKey: true }))
    expect(router.replace).not.toHaveBeenCalled()
  })

  it('keeps the header calm: Launch and Shell, the rest in a menu with their selectors', async () => {
    const w = mountPage()
    await flushPromises()
    const actions = w.findAll('.page-actions .btn').map((b) => b.text().trim())
    expect(actions).toEqual(['', 'More', 'Shell', 'Launch'].map((t) => expect.stringContaining(t)))
    expect(actions.at(-1)).toBe('Launch') // any agent: the dialog offers Claude Code and Codex (ADR-0052)
    for (const sel of ['[data-resume-project]', '[data-open-history]', '[data-project-context]', '[data-project-workflow]']) expect(w.find(`.menu ${sel}`).exists()).toBe(true)
    w.unmount()
  })
})

describe('Tasks tab', () => {
  it('moves a task to Recently done at once and rolls back when the file cannot be written', async () => {
    const { default: ProjectTasks } = await import('../src/components/ProjectTasks.vue')
    let fail: (e: Error) => void = () => undefined
    const update = vi.fn(() => new Promise((_, reject) => (fail = reject)))
    Object.assign(api, { knowledgeUpdateTask: update })
    const w = mount(ProjectTasks, { props: { projectId: 'p1', available: true, knowledge }, global: { stubs: { KnowledgeDialog: true, KnowledgeDocDialog: true, TaskBoard: true } } })
    await w.find('[data-task="b"] [data-task-done]').trigger('click')
    // Optimistic: gone from the open list, in Recently done, while the write is still pending (button busy).
    expect(w.find('[data-project-tasks] [data-task="b"]').exists()).toBe(false)
    expect(w.find('[data-done-task="b"]').exists()).toBe(true)
    fail(new Error('the file changed on disk'))
    await flushPromises()
    expect(w.find('[data-project-tasks] [data-task="b"]').exists()).toBe(true)
    expect(w.find('[data-done-task="b"]').exists()).toBe(false)
    expect(update).toHaveBeenCalledWith('w1', 'b', { status: 'done', log: 'Marked done' })
  })
})
