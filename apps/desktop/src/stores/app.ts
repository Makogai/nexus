import { defineStore } from 'pinia'
import { computed, reactive, ref, watch } from 'vue'
import { api, toIpcError } from '../lib/ipc'
import type { AppInfo, UiPreferences } from '../lib/types'

export interface Toast {
  id: number
  tone: 'info' | 'success' | 'error'
  message: string
}

export interface ConfirmRequest {
  title: string
  message: string
  confirmLabel?: string
  danger?: boolean
  resolve: (ok: boolean) => void
}

const DEFAULT_PREFS: UiPreferences = {
  theme: 'dark',
  terminalFontSize: 13,
  terminalHeight: 320,
  terminalOpen: true,
  sidebarCollapsed: false,
  defaultShell: null,
  lastLaunch: null,
}

export const useAppStore = defineStore('app', () => {
  const info = ref<AppInfo | null>(null)
  const prefs = reactive<UiPreferences>({ ...DEFAULT_PREFS })
  const prefsLoaded = ref(false)
  const toasts = ref<Toast[]>([])
  const confirmRequest = ref<ConfirmRequest | null>(null)
  const paletteOpen = ref(false)
  const launchDialog = ref<{
    open: boolean
    projectId?: string | null
    profileId?: string | null
    /** Launch for a knowledge task (ADR-0018). */
    task?: { id: string; title: string; projectId: string | null } | null
    /** Preselect "start right away" (the Learn view's guided session). */
    kickoff?: boolean
  }>({ open: false })
  /** Increments when another process (nexus CLI, MCP server) committed to the database (ADR-0009). */
  const externalChanges = ref(0)
  let toastSeq = 0

  const systemDark = window.matchMedia?.('(prefers-color-scheme: dark)')
  const resolvedTheme = computed<'dark' | 'light'>(() =>
    prefs.theme === 'system' ? (systemDark?.matches ? 'dark' : 'light') : prefs.theme,
  )

  function applyTheme() {
    document.documentElement.dataset.theme = resolvedTheme.value
  }
  systemDark?.addEventListener?.('change', applyTheme)

  async function load() {
    try {
      const [i, p] = await Promise.all([api.appInfo(), api.uiPreferencesGet()])
      info.value = i
      if (p) Object.assign(prefs, DEFAULT_PREFS, p)
    } catch (e) {
      toast('error', `Could not load application state: ${toIpcError(e).message}`)
    } finally {
      prefsLoaded.value = true
      applyTheme()
    }
  }

  let saveTimer: ReturnType<typeof setTimeout> | undefined
  watch(
    prefs,
    () => {
      applyTheme()
      if (!prefsLoaded.value) return
      clearTimeout(saveTimer)
      saveTimer = setTimeout(() => {
        api.uiPreferencesSet({ ...prefs }).catch(() => undefined)
      }, 400)
    },
    { deep: true },
  )

  function toast(tone: Toast['tone'], message: string, ms = tone === 'error' ? 7000 : 3500) {
    const id = ++toastSeq
    toasts.value.push({ id, tone, message })
    setTimeout(() => dismiss(id), ms)
  }
  function dismiss(id: number) {
    toasts.value = toasts.value.filter((t) => t.id !== id)
  }
  /** Run an action and surface failures as an error toast. Returns undefined on failure. */
  async function attempt<T>(fn: () => Promise<T>, failure = 'Action failed'): Promise<T | undefined> {
    try {
      return await fn()
    } catch (e) {
      toast('error', `${failure}: ${toIpcError(e).message}`)
      return undefined
    }
  }

  function confirm(opts: Omit<ConfirmRequest, 'resolve'>): Promise<boolean> {
    return new Promise((resolve) => {
      confirmRequest.value = { ...opts, resolve }
    })
  }
  function settleConfirm(ok: boolean) {
    confirmRequest.value?.resolve(ok)
    confirmRequest.value = null
  }

  let lastDataVersion: number | null = null
  let watching: ReturnType<typeof setInterval> | undefined
  /**
   * Record the current `PRAGMA data_version` *before* the initial data load, so a write by another process
   * between loading and the first poll is still detected (at worst one extra reload).
   */
  async function markDataBaseline() {
    lastDataVersion = await api.dbDataVersion().catch(() => null)
  }
  /** Poll `PRAGMA data_version` (a few bytes) while the window is visible. */
  function watchExternalChanges(intervalMs = 2000) {
    if (watching) return
    watching = setInterval(async () => {
      if (document.visibilityState !== 'visible') return
      try {
        const v = await api.dbDataVersion()
        if (lastDataVersion === null || v !== lastDataVersion) externalChanges.value++
        lastDataVersion = v
      } catch {
        /* transient */
      }
    }, intervalMs)
  }

  function openLaunch(
    projectId?: string | null,
    profileId?: string | null,
    task?: { id: string; title: string; projectId: string | null } | null,
    opts: { kickoff?: boolean } = {},
  ) {
    launchDialog.value = { open: true, projectId: task?.projectId ?? projectId, profileId, task: task ?? null, kickoff: !!task && !!opts.kickoff }
  }

  return {
    info,
    prefs,
    prefsLoaded,
    resolvedTheme,
    toasts,
    confirmRequest,
    paletteOpen,
    launchDialog,
    externalChanges,
    markDataBaseline,
    watchExternalChanges,
    load,
    toast,
    dismiss,
    attempt,
    confirm,
    settleConfirm,
    openLaunch,
  }
})
