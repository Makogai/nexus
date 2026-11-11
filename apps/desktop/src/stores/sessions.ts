import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { api, SESSION_EVENT } from '../lib/ipc'
import type { SessionView, StartAgent, StartClaude } from '../lib/types'
import { useAppStore } from './app'

/** Last measured terminal size, so new PTYs start at the size they will be shown at. */
const lastSize = { cols: 120, rows: 32 }

export const useSessionStore = defineStore('sessions', () => {
  const list = ref<SessionView[]>([])
  const loaded = ref(false)
  /** Terminal tabs open in the panel (session ids), in display order. */
  const tabs = ref<string[]>([])
  const activeTab = ref<string | null>(null)
  const focusMode = ref(false)

  const byId = computed(() => Object.fromEntries(list.value.map((s) => [s.id, s])))
  const running = computed(() => list.value.filter((s) => s.running))
  const resumable = computed(() => list.value.filter((s) => s.resumable && !s.archivedAt))

  function upsert(s: SessionView) {
    const i = list.value.findIndex((x) => x.id === s.id)
    if (i === -1) list.value.unshift(s)
    else list.value[i] = s
  }

  async function load(workspaceId?: string) {
    list.value = await api.sessionList({ workspaceId, limit: 300 })
    loaded.value = true
  }

  let unlisten: (() => void) | null = null
  async function subscribe() {
    if (unlisten) return
    unlisten = await listen<SessionView>(SESSION_EVENT, (e) => {
      upsert(e.payload)
      // A session that ended after its tab was closed: free the backend buffer now.
      if (!e.payload.running && e.payload.attached && !tabs.value.includes(e.payload.id)) {
        api.sessionRelease(e.payload.id).catch(() => undefined)
      }
    })
  }

  function openTab(id: string) {
    if (!tabs.value.includes(id)) tabs.value.push(id)
    activeTab.value = id
    useAppStore().prefs.terminalOpen = true
  }

  async function closeTab(id: string) {
    const s = byId.value[id]
    if (s?.running) {
      const app = useAppStore()
      const ok = await app.confirm({
        title: 'Close a running terminal?',
        message: `"${s.title}" is still running. Closing the tab terminates the process${
          s.kind === 'claude' ? ' — the conversation stays resumable' : ''
        }.`,
        confirmLabel: 'Terminate and close',
        danger: true,
      })
      if (!ok) return
      await api.sessionTerminate(id).catch(() => undefined)
    }
    const idx = tabs.value.indexOf(id)
    tabs.value = tabs.value.filter((t) => t !== id)
    if (activeTab.value === id) activeTab.value = tabs.value[Math.max(0, idx - 1)] ?? null
    // Exited terminals free their buffer; running ones exit shortly and are released by the backend later.
    setTimeout(() => api.sessionRelease(id).catch(() => undefined), 500)
  }

  function rememberSize(cols: number, rows: number) {
    lastSize.cols = cols
    lastSize.rows = rows
  }

  async function startClaude(args: Omit<StartClaude, 'cols' | 'rows'>) {
    const s = await api.sessionStartClaude({ ...args, ...lastSize })
    upsert(s)
    openTab(s.id)
    return s
  }

  /** A Codex (or later agent) session (ADR-0052). */
  async function startAgent(args: Omit<StartAgent, 'cols' | 'rows'>) {
    const s = await api.sessionStartAgent({ ...args, ...lastSize })
    upsert(s)
    openTab(s.id)
    return s
  }

  async function continueTask(id: string, taskId: string) {
    const s = await api.sessionContinueTask(id, taskId)
    upsert(s)
    openTab(s.id)
    return s
  }

  async function resume(id: string) {
    const s = await api.sessionResume(id, lastSize.cols, lastSize.rows)
    upsert(s)
    openTab(s.id)
    return s
  }

  async function openShell(opts: { shellId?: string | null; projectId?: string | null; cwd?: string | null }) {
    const s = await api.sessionOpenShell({ ...opts, ...lastSize })
    upsert(s)
    openTab(s.id)
    return s
  }

  async function rename(id: string, title: string) {
    upsert(await api.sessionRename(id, title))
  }

  async function archive(id: string) {
    upsert(await api.sessionSetArchived(id, true))
    list.value = list.value.filter((s) => s.id !== id)
  }

  return {
    continueTask,
    list,
    loaded,
    byId,
    running,
    resumable,
    tabs,
    activeTab,
    focusMode,
    load,
    subscribe,
    upsert,
    openTab,
    closeTab,
    rememberSize,
    startClaude,
    startAgent,
    resume,
    openShell,
    rename,
    archive,
  }
})
