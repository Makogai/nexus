// Signed updates (ADR-0044; owner: "an update checker and automatic updates, like Discord"). Release builds check the
// latest *published* release on start and every few hours; the Tauri updater verifies every download against the
// public key built into NEXUS before installing. On start (no sessions run yet) an update installs by itself unless
// the person turned that off; later ones wait for a click — never while Claude sessions run without asking first.
import { defineStore } from 'pinia'
import { ref } from 'vue'
import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { useAppStore } from './app'
import { useSessionStore } from './sessions'

const RECHECK_MS = 6 * 60 * 60 * 1000

export type UpdatePhase = 'idle' | 'checking' | 'available' | 'downloading' | 'installing' | 'error'

export const useUpdateStore = defineStore('updates', () => {
  const phase = ref<UpdatePhase>('idle')
  const available = ref<{ version: string; notes: string | null; date: string | null } | null>(null)
  const progress = ref<number | null>(null)
  const error = ref<string | null>(null)
  const checkedAt = ref<number | null>(null)
  let update: Update | null = null
  let timer: ReturnType<typeof setInterval> | null = null

  /** Look for a newer published release. `installNow`: install it right away (the start of NEXUS). */
  async function checkNow(installNow = false) {
    const app = useAppStore()
    if (!app.info?.updates || phase.value === 'downloading' || phase.value === 'installing') return
    phase.value = 'checking'
    error.value = null
    try {
      update = await check()
      checkedAt.value = Date.now()
      if (!update) {
        available.value = null
        phase.value = 'idle'
        return
      }
      available.value = { version: update.version, notes: update.body ?? null, date: update.date ?? null }
      phase.value = 'available'
      if (installNow && app.prefs.autoUpdate !== false && !runningSessions()) await install(false)
    } catch (e) {
      // Offline, or no release with an update manifest yet: say so in Settings, never in the way.
      error.value = e instanceof Error ? e.message : String(e)
      phase.value = available.value ? 'available' : 'error'
    }
  }

  function runningSessions(): number {
    return useSessionStore().list.filter((s) => s.running && s.attached).length
  }

  /** Download, verify and install; NEXUS restarts. Asks first while sessions run (their terminals end). */
  async function install(ask = true) {
    const app = useAppStore()
    if (!update) return
    const running = runningSessions()
    if (ask && running) {
      const ok = await app.confirm({
        title: `Update NEXUS to v${update.version}?`,
        message: `${running} session${running === 1 ? ' is' : 's are'} running. Updating restarts NEXUS: their terminals end, and Claude keeps the conversations, so you can resume them afterwards.`,
        confirmLabel: 'Update and restart',
      })
      if (!ok) return
    }
    phase.value = 'downloading'
    progress.value = 0
    let total = 0
    let done = 0
    try {
      await update.downloadAndInstall((ev) => {
        if (ev.event === 'Started') total = ev.data.contentLength ?? 0
        else if (ev.event === 'Progress') {
          done += ev.data.chunkLength
          progress.value = total ? Math.min(100, Math.round((done / total) * 100)) : null
        } else if (ev.event === 'Finished') phase.value = 'installing'
      })
      phase.value = 'installing'
      await relaunch()
    } catch (e) {
      error.value = e instanceof Error ? e.message : String(e)
      phase.value = 'error'
      app.toast('error', `The update could not be installed: ${error.value}`)
    }
  }

  /** On start: check (and install), then check again every few hours (offer only). */
  function start() {
    if (timer) return
    void checkNow(true)
    timer = setInterval(() => void checkNow(false), RECHECK_MS)
  }

  return { phase, available, progress, error, checkedAt, checkNow, install, start }
})
