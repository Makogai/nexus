<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { listen } from '@tauri-apps/api/event'
import { useAppStore } from './stores/app'
import { useUpdateStore } from './stores/updates'
import { useWorkspaceStore } from './stores/workspaces'
import { useProjectStore } from './stores/projects'
import { useProfileStore } from './stores/profiles'
import { useSessionStore } from './stores/sessions'
import { useNotificationStore } from './stores/notifications'
import { KNOWLEDGE_EVENT, api, toIpcError } from './lib/ipc'
import AppSidebar from './components/AppSidebar.vue'
import AppTopBar from './components/AppTopBar.vue'
import WorkspaceBanner from './components/WorkspaceBanner.vue'
import TerminalPanel from './components/TerminalPanel.vue'
import LaunchDialog from './components/LaunchDialog.vue'
import CommandPalette from './components/CommandPalette.vue'
import ConfirmDialog from './components/ConfirmDialog.vue'
import PushRequestDialog from './components/PushRequestDialog.vue'
import ToastHost from './components/ToastHost.vue'
import OnboardingView from './views/OnboardingView.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const sessions = useSessionStore()
const notifications = useNotificationStore()

const ready = ref(false)
const fatal = ref<string | null>(null)

const showContent = computed(() => !(sessions.focusMode && app.prefs.terminalOpen && sessions.tabs.length > 0))

async function boot() {
  try {
    await app.load()
    if (app.info?.recovery) {
      app.toast(
        'error',
        `NEXUS's database could not be read and was kept as ${app.info.recovery.quarantinedTo}. A fresh database was ` +
          (app.info.recovery.latestBackup
            ? `created — you can restore the latest backup (${app.info.recovery.latestBackup}) under Settings → Backups.`
            : 'created — reopen your workspaces from Settings (their projects and portable data live in the workspace folders).'),
        60_000,
      )
    }
    if (app.info?.restored) app.toast('success', `Backup ${app.info.restored} restored. The previous state was backed up and kept.`, 15_000)
    if (app.info?.restoreError) app.toast('error', `The scheduled restore did not happen — your data is unchanged: ${app.info.restoreError}`, 60_000)
    await app.markDataBaseline()
    await Promise.all([workspaces.load(), profiles.load(), sessions.subscribe()])
    await sessions.load()
    if (workspaces.active) await projects.load(workspaces.active.id)
    await notifications.start().catch(() => undefined)
    app.watchExternalChanges()
    ready.value = true
    // Signed updates (ADR-0044): release builds check now (and install by themselves at start), then every few hours.
    useUpdateStore().start()
  } catch (e) {
    fatal.value = toIpcError(e).message
  }
}

// Sessions started by `nexus launch` and checkpoints recorded through MCP come from other processes.
watch(
  () => app.externalChanges,
  () => {
    sessions.load().catch(() => undefined)
  },
)

watch(
  () => workspaces.active?.id,
  async (id, prev) => {
    if (id && id !== prev && ready.value) await projects.load(id)
  },
)

/**
 * Knowledge freshness: files change outside NEXUS — an editor, `git pull`, Claude. The desktop watches the active
 * workspace's knowledge folders and refreshes the index itself (ADR-0049); its event moves `app.externalChanges` (lists)
 * and `app.freshness` (open documents) at once. Window focus and the end of a session still refresh (ADR-0048), for
 * folders whose changes are not reported (incremental: unchanged files are not read).
 */
let lastFresh = 0
async function refreshFromDisk() {
  const ws = workspaces.active
  if (!ws || !ready.value || Date.now() - lastFresh < 5000) return
  lastFresh = Date.now()
  await api.knowledgeOverview(ws.id, false).catch(() => undefined)
  app.freshness++
}
watch(
  () => sessions.running.length,
  (n, before) => {
    if (n < before) refreshFromDisk()
  },
)

function onKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey
  const inTerminal = (e.target as HTMLElement | null)?.closest?.('.xterm') != null
  if (mod && e.shiftKey && e.key.toLowerCase() === 'p') {
    e.preventDefault()
    app.paletteOpen = true
  } else if (mod && !e.shiftKey && e.key.toLowerCase() === 'k' && !inTerminal) {
    e.preventDefault()
    app.paletteOpen = true
  } else if (mod && e.key === '`') {
    e.preventDefault()
    app.prefs.terminalOpen = !app.prefs.terminalOpen
  } else if (mod && e.shiftKey && e.key.toLowerCase() === 'l') {
    e.preventDefault()
    app.openLaunch()
  }
}

let unlistenClose: (() => void) | null = null
let unlistenKnowledge: (() => void) | null = null
onMounted(async () => {
  window.addEventListener('keydown', onKey)
  window.addEventListener('focus', refreshFromDisk)
  // Not awaited: the app starts even if the event channel does not.
  listen(KNOWLEDGE_EVENT, () => {
    app.externalChanges++
    app.freshness++
  })
    .then((u) => (unlistenKnowledge = u))
    .catch(() => undefined)
  await boot()
  // Closing the window ends every running terminal: ask first when something is still running.
  unlistenClose = await getCurrentWindow().onCloseRequested(async (event) => {
    if (sessions.running.length === 0) return
    event.preventDefault()
    const ok = await app.confirm({
      title: 'Quit NEXUS?',
      message: `${sessions.running.length} terminal session(s) are still running and will be terminated. Claude conversations remain resumable.`,
      confirmLabel: 'Quit',
      danger: true,
    })
    if (ok) await getCurrentWindow().destroy()
  })
})
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  window.removeEventListener('focus', refreshFromDisk)
  unlistenClose?.()
  unlistenKnowledge?.()
})
</script>

<template>
  <div v-if="fatal" class="fatal">
    <h1>NEXUS could not start</h1>
    <p class="selectable">{{ fatal }}</p>
    <button class="btn" @click="() => { fatal = null; boot() }">Try again</button>
  </div>
  <div v-else-if="!ready" class="booting">
    <div class="boot-mark" />
  </div>
  <OnboardingView v-else-if="!workspaces.active" />
  <div v-else class="shell" :class="{ collapsed: app.prefs.sidebarCollapsed }">
    <AppSidebar />
    <div class="main">
      <AppTopBar />
      <WorkspaceBanner />
      <div class="work">
        <main v-show="showContent" class="content">
          <RouterView v-slot="{ Component }">
            <KeepAlive :max="4">
              <component :is="Component" />
            </KeepAlive>
          </RouterView>
        </main>
        <TerminalPanel :fill="!showContent" />
      </div>
    </div>
  </div>
  <LaunchDialog v-if="ready" />
  <CommandPalette v-if="ready" />
  <PushRequestDialog v-if="ready" />
  <ConfirmDialog />
  <ToastHost />
</template>

<style scoped>
.shell {
  display: grid;
  grid-template-columns: var(--sidebar-w) 1fr;
  height: 100%;
}
.shell.collapsed {
  grid-template-columns: 64px 1fr;
}
.main {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
.work {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.booting,
.fatal {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
}
.fatal p {
  max-width: 640px;
  color: var(--text-muted);
  text-align: center;
}
.boot-mark {
  width: 36px;
  height: 36px;
  border-radius: 10px;
  background: linear-gradient(135deg, var(--accent), var(--cyan));
  animation: pulse-mark 1.2s ease-in-out infinite;
}
@keyframes pulse-mark {
  50% {
    opacity: 0.4;
    transform: scale(0.9);
  }
}
</style>
