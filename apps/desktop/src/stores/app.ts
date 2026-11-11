import { defineStore } from 'pinia'
import { computed, reactive, ref, watch } from 'vue'
import { api, toIpcError } from '../lib/ipc'
import type { AppInfo, UiPreferences } from '../lib/types'

export interface Toast {
  id: number
  tone: 'info' | 'success' | 'error'
  message: string
  /** The same error again while it is still shown (collapsed into one toast). */
  count?: number
}

/** At most this many error toasts stay on screen; the oldest goes first. */
const MAX_ERROR_TOASTS = 4

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
    /** Preselect "start right away" (the Learn view's guided session); unset = the person's last choice. */
    kickoff?: boolean
    /** Launch with another agent's profile (ADR-0052); a `profileId` instead means Claude Code. */
    agentProfileId?: string | null
  }>({ open: false })
  /** The push request under review (ADR-0043); null = dialog closed. */
  const pushRequestId = ref<string | null>(null)
  /** Increments after a push request was decided (views reload their lists). */
  const pushRequestsChanged = ref(0)
  /** Increments when another process (nexus CLI, MCP server) committed to the database (ADR-0009). */
  const externalChanges = ref(0)
  /** Bumped when NEXUS looked for files changed outside it (window focus, a session ended) — ADR-0048. */
  const freshness = ref(0)
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

  /**
   * Show a toast. Errors stay until the person dismisses them (ADR-0048) — an error that vanished on its own was one
   * nobody read; the same error again is counted on the toast already shown. Others go after `ms`.
   */
  function toast(tone: Toast['tone'], message: string, ms = tone === 'error' ? 0 : 3500) {
    if (tone === 'error') {
      const same = toasts.value.find((t) => t.tone === 'error' && t.message === message)
      if (same) {
        same.count = (same.count ?? 1) + 1
        return
      }
      const errors = toasts.value.filter((t) => t.tone === 'error')
      if (errors.length >= MAX_ERROR_TOASTS) dismiss(errors[0]!.id)
    }
    const id = ++toastSeq
    toasts.value.push({ id, tone, message })
    if (ms > 0) setTimeout(() => dismiss(id), ms)
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
    opts: { kickoff?: boolean; agentProfileId?: string } = {},
  ) {
    launchDialog.value = {
      open: true,
      projectId: task?.projectId ?? projectId,
      profileId,
      task: task ?? null,
      kickoff: task ? opts.kickoff : undefined,
      agentProfileId: opts.agentProfileId,
    }
  }

  return {
    pushRequestId,
    pushRequestsChanged,
    info,
    prefs,
    prefsLoaded,
    resolvedTheme,
    toasts,
    confirmRequest,
    paletteOpen,
    launchDialog,
    externalChanges,
    freshness,
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
