// Notification center: real components + Pinia stores; IPC, router and native APIs are mocked (ADR-0025).
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { createPinia, setActivePinia } from 'pinia'
import type { AppNotification, SessionView } from '../src/lib/types'

const api = vi.hoisted(() => ({
  notificationsList: vi.fn(),
  notificationsCounts: vi.fn(),
  notificationsMarkRead: vi.fn(),
  notificationsDismiss: vi.fn(),
  notificationsSnooze: vi.fn(),
  notificationTarget: vi.fn(),
  workspaceActivate: vi.fn(),
  workspaceList: vi.fn(),
  projectList: vi.fn(),
  sessionList: vi.fn(),
  uiPreferencesSet: vi.fn(),
}))
const push = vi.hoisted(() => vi.fn())

vi.mock('../src/lib/ipc', () => ({
  api,
  SESSION_EVENT: 'nexus://session',
  NOTIFICATIONS_EVENT: 'nexus://notifications',
  NOTIFICATION_ACTIVATED_EVENT: 'nexus://notification-activated',
  toIpcError: (e: unknown) => ({ code: 'internal', message: String((e as { message?: string })?.message ?? e) }),
}))
vi.mock('../src/router', () => ({ router: { push } }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => () => undefined) }))
vi.mock('vue-router', () => ({ useRouter: () => ({ push }), useRoute: () => ({ query: {} }) }))

import NotificationBell from '../src/components/NotificationBell.vue'
import { useAppStore } from '../src/stores/app'
import { useNotificationStore } from '../src/stores/notifications'
import { useSessionStore } from '../src/stores/sessions'
import { useWorkspaceStore } from '../src/stores/workspaces'

function n(over: Partial<AppNotification> = {}): AppNotification {
  return {
    id: 'n1',
    category: 'permission-requested',
    priority: 'action',
    source: 'hook',
    title: 'Permission requested: Bash',
    body: 'Session “Checkout” waits for your decision in its terminal.',
    workspaceId: 'w1',
    projectId: null,
    profileId: null,
    sessionId: 's1',
    taskId: null,
    workflowId: null,
    targetKind: 'session',
    targetId: 's1',
    dedupeKey: 'permission:s1',
    occurrences: 2,
    createdAt: Date.now(),
    updatedAt: Date.now(),
    readAt: null,
    dismissedAt: null,
    snoozedUntil: null,
    resolvedAt: null,
    desktopState: 'sent',
    desktopReason: null,
    ...over,
  }
}

function live(id: string, status: SessionView['status']['status'], title = id): SessionView {
  return {
    id,
    kind: 'claude',
    host: 'desktop',
    hostId: 'h',
    workspaceId: 'w1',
    projectId: null,
    profileId: null,
    claudeSessionId: null,
    title,
    cwd: '/x',
    branch: null,
    workProfile: null,
    effort: null,
    model: null,
    program: 'claude',
    pid: 1,
    state: 'running',
    exitCode: null,
    error: null,
    resumedFrom: null,
    startedAt: 0,
    endedAt: null,
    archivedAt: null,
    attached: true,
    running: true,
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
    conversationRecorded: null,
    resumable: false,
    status: { status, certainty: 'reported', since: Date.now(), detail: null },
  }
}

const target = (over = {}) => ({
  kind: 'session',
  id: 's1',
  exists: true,
  workspaceId: 'w1',
  projectId: null,
  attached: true,
  liveElsewhere: false,
  resumable: false,
  ...over,
})

beforeEach(() => {
  setActivePinia(createPinia())
  for (const fn of Object.values(api)) fn.mockReset()
  push.mockReset()
  api.notificationsList.mockResolvedValue([n()])
  api.notificationsCounts.mockResolvedValue({ unread: 3, actionRequired: 1 })
  api.notificationsMarkRead.mockResolvedValue(1)
  api.sessionList.mockResolvedValue([])
  api.uiPreferencesSet.mockResolvedValue(undefined)
  useWorkspaceStore().active = { id: 'w1', name: 'W' } as never
  useWorkspaceStore().list = [{ id: 'w1', name: 'W' }, { id: 'w2', name: 'Other' }] as never
})

describe('NotificationBell', () => {
  it('shows the action count, the sessions that need the user (most urgent first) and recent notifications', async () => {
    const store = useNotificationStore()
    useSessionStore().list = [live('a', 'running', 'Quiet'), live('b', 'idle', 'Done turn'), live('c', 'permission-required', 'Needs OK')]
    await store.refresh()
    const w = mount(NotificationBell, { attachTo: document.body })
    expect(w.find('[data-testid="bell-action"]').text()).toBe('1')
    await w.find('[data-testid="bell"]').trigger('click')
    const lines = w.findAll('.session-line').map((l) => l.text())
    expect(lines).toHaveLength(2) // a running session is not listed
    expect(lines[0]).toContain('Permission required')
    expect(lines[1]).toContain('Your turn')
    expect(w.find('[data-notification="n1"]').text()).toContain('×2')
    await w.find('.head .btn-ghost').trigger('click') // Mark all read
    expect(api.notificationsMarkRead).toHaveBeenCalledWith([], true)
  })
})

describe('opening a notification', () => {
  it('opens the session terminal when it is attached here', async () => {
    api.notificationTarget.mockResolvedValue(target())
    await useNotificationStore().open({ id: 'n1' })
    expect(useSessionStore().activeTab).toBe('s1')
    expect(push).not.toHaveBeenCalled()
  })

  it('switches to the session’s workspace first', async () => {
    api.notificationTarget.mockResolvedValue(target({ workspaceId: 'w2' }))
    api.workspaceActivate.mockResolvedValue({ id: 'w2', name: 'Other' })
    api.workspaceList.mockResolvedValue([{ id: 'w1' }, { id: 'w2' }])
    api.projectList.mockResolvedValue([])
    await useNotificationStore().open({ id: 'n1' })
    expect(api.workspaceActivate).toHaveBeenCalledWith('w2')
    expect(useSessionStore().activeTab).toBe('s1')
  })

  it('explains a session that no longer exists or has ended, and routes review queues', async () => {
    const app = useAppStore()
    api.notificationTarget.mockResolvedValue(target({ exists: false }))
    await useNotificationStore().open({ id: 'n1' })
    expect(app.toasts.at(-1)?.message).toBe('That session no longer exists.')
    expect(push).toHaveBeenLastCalledWith({ name: 'notifications' })

    api.notificationTarget.mockResolvedValue(target({ attached: false, resumable: true }))
    await useNotificationStore().open({ id: 'n1' })
    expect(push).toHaveBeenLastCalledWith({ name: 'sessions', query: { focus: 's1' } })
    expect(app.toasts.at(-1)?.message).toContain('resume its conversation')

    api.notificationTarget.mockResolvedValue(target({ kind: 'proposals', id: 'w1' }))
    await useNotificationStore().open({ id: 'n1' })
    expect(push).toHaveBeenLastCalledWith({ name: 'knowledge', query: { tab: 'proposals' } })

    api.notificationTarget.mockRejectedValue({ message: 'no notification n1' })
    await useNotificationStore().open({ id: 'n1' })
    await flushPromises()
    expect(app.toasts.at(-1)?.message).toContain('no longer exists')
  })
})
