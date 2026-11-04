import { defineStore } from 'pinia'
import { computed, ref, watch } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { api, NOTIFICATION_ACTIVATED_EVENT, NOTIFICATIONS_EVENT, toIpcError } from '../lib/ipc'
import type { AppNotification, NotificationCounts, NotificationPrefs, NotifierAvailability } from '../lib/types'
import { attentionRank, needsUser } from '../lib/format'
import { router } from '../router'
import { useAppStore } from './app'
import { useProjectStore } from './projects'
import { useSessionStore } from './sessions'
import { useWorkspaceStore } from './workspaces'

/** Notification center state (ADR-0025). Rows come from every NEXUS process; the backend tells us when they change. */
export const useNotificationStore = defineStore('notifications', () => {
  const recent = ref<AppNotification[]>([])
  const counts = ref<NotificationCounts>({ unread: 0, actionRequired: 0 })
  const prefs = ref<NotificationPrefs | null>(null)
  const availability = ref<NotifierAvailability | null>(null)
  const centerOpen = ref(false)
  /** Bumps when the set changed, so views with their own filtered lists reload. */
  const revision = ref(0)

  async function refresh() {
    const [list, c] = await Promise.all([api.notificationsList({ limit: 50 }), api.notificationsCounts()])
    recent.value = list
    counts.value = c
    revision.value++
  }

  async function loadPrefs() {
    const [p, a] = await Promise.all([api.notificationsPrefs(), api.notificationsAvailability()])
    prefs.value = p
    availability.value = a
  }

  async function savePrefs(p: NotificationPrefs) {
    prefs.value = await api.notificationsSetPrefs(p)
  }

  async function markRead(ids: string[]) {
    if (ids.length === 0) return
    await api.notificationsMarkRead(ids)
    await refresh()
  }
  async function markAllRead() {
    await api.notificationsMarkRead([], true)
    await refresh()
  }
  async function dismiss(ids: string[]) {
    await api.notificationsDismiss(ids)
    await refresh()
  }
  async function snooze(id: string, minutes: number) {
    await api.notificationsSnooze(id, minutes)
    await refresh()
  }

  /** Live sessions, most urgent first — the multi-session attention list. */
  const attention = computed(() => {
    const sessions = useSessionStore()
    return sessions.list
      .filter((s) => (s.running || s.liveElsewhere) && s.kind === 'claude')
      .sort((a, b) => (attentionRank[a.status.status] ?? 9) - (attentionRank[b.status.status] ?? 9) || (b.status.since ?? 0) - (a.status.since ?? 0))
  })
  const waitingCount = computed(() => attention.value.filter((s) => needsUser(s.status)).length)

  /** Open what a notification is about: the session's terminal (switching workspace if needed), a task, a queue. */
  async function open(n: Pick<AppNotification, 'id'>) {
    const app = useAppStore()
    const sessions = useSessionStore()
    const workspaces = useWorkspaceStore()
    centerOpen.value = false
    let t
    try {
      t = await api.notificationTarget(n.id)
    } catch (e) {
      app.toast('error', `This notification no longer exists (${toIpcError(e).message}).`)
      await refresh()
      return
    }
    if (t.workspaceId && t.workspaceId !== workspaces.active?.id && workspaces.list.some((w) => w.id === t.workspaceId)) {
      await workspaces.activate(t.workspaceId)
      await Promise.all([useProjectStore().load(t.workspaceId), sessions.load()])
    }
    switch (t.kind) {
      case 'session': {
        if (!t.exists || !t.id) {
          app.toast('info', 'That session no longer exists.')
          await router.push({ name: 'notifications' })
          break
        }
        if (t.attached) {
          await sessions.load().catch(() => undefined)
          sessions.openTab(t.id)
        } else {
          await router.push({ name: 'sessions', query: { focus: t.id } })
          app.toast(
            'info',
            t.liveElsewhere
              ? 'This session runs in another terminal (nexus launch) — answer it there.'
              : t.resumable
                ? 'This session has ended. You can resume its conversation from the session list.'
                : 'This session has ended.',
          )
        }
        break
      }
      case 'task':
        await router.push({ name: 'knowledge', query: { task: t.id ?? undefined } })
        break
      case 'proposals':
        await router.push({ name: 'knowledge', query: { tab: 'proposals' } })
        break
      case 'kit':
        await router.push({ name: 'kits' })
        break
      case 'rules':
        await router.push({ name: 'constitutions', query: { pending: '1' } })
        break
      case 'profile':
        await router.push({ name: 'profiles' })
        break
      case 'workflow':
        if (t.id) await router.push({ name: 'workflows', query: { run: t.id } })
        break
      case 'project':
        if (t.id) await router.push({ name: 'project', params: { id: t.id } })
        break
      default:
        await router.push({ name: 'notifications' })
    }
    await refresh()
  }

  /** Tell the backend which terminal is visible (no desktop toast for what the user is looking at). */
  function reportView() {
    const sessions = useSessionStore()
    const app = useAppStore()
    const visible = app.prefs.terminalOpen && document.visibilityState === 'visible' ? sessions.activeTab : null
    api.notificationsSetView(visible, new Date().getTimezoneOffset()).catch(() => undefined)
  }

  let started = false
  async function start() {
    if (started) return
    started = true
    const sessions = useSessionStore()
    const app = useAppStore()
    await listen(NOTIFICATIONS_EVENT, () => {
      refresh().catch(() => undefined)
      // Attention changes reported by hooks (other processes) also change session status.
      sessions.load().catch(() => undefined)
    })
    // The backend already brought the window to the front.
    await listen<string>(NOTIFICATION_ACTIVATED_EVENT, (e) => open({ id: e.payload }))
    watch(() => [sessions.activeTab, app.prefs.terminalOpen], reportView)
    document.addEventListener('visibilitychange', reportView)
    reportView()
    await Promise.all([refresh(), loadPrefs()])
    // NEXUS was started by a click on a notification (nexus://notification/<id>).
    const pending = await api.notificationsTakeActivation().catch(() => null)
    if (pending) await open({ id: pending })
  }

  return {
    recent,
    counts,
    prefs,
    availability,
    centerOpen,
    revision,
    attention,
    waitingCount,
    refresh,
    loadPrefs,
    savePrefs,
    markRead,
    markAllRead,
    dismiss,
    snooze,
    open,
    start,
  }
})
