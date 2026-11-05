<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ArrowRight, FolderInput, FolderPlus, GitBranch, ShieldCheck, TriangleAlert } from '@lucide/vue'
import CloneWorkspaceForm from '../components/CloneWorkspaceForm.vue'
import NewWorkspaceForm, { type WorkspaceSource } from '../components/NewWorkspaceForm.vue'
import type { SyncCloned } from '../lib/types'
import { router } from '../router'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { toIpcError } from '../lib/ipc'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const profiles = useProfileStore()
const projects = useProjectStore()

const mode = ref<'choose' | 'create' | 'clone'>('choose')
const busy = ref(false)
const error = ref<string | null>(null)
/** A folder picked to open that is not a workspace yet: offered as the place to set one up. */
const notYet = ref<string | null>(null)
/** How the create form starts (a fresh form per choice). */
const start = ref<{ source: WorkspaceSource; folder?: string; url?: string; key: number }>({ source: 'new', key: 0 })

onMounted(() => {
  profiles.detect().catch(() => undefined)
})

function createWith(source: WorkspaceSource, more: { folder?: string; url?: string } = {}) {
  start.value = { source, ...more, key: start.value.key + 1 }
  error.value = null
  notYet.value = null
  mode.value = 'create'
}

async function cloned(r: SyncCloned) {
  app.toast('success', `Workspace “${r.workspace.name}” opened from Git`)
  await router.push({ name: 'restore' })
}

async function importExisting() {
  const dir = await openDialog({ directory: true, title: 'Choose a folder that contains nexus-workspace.json' })
  if (typeof dir !== 'string') return
  busy.value = true
  error.value = null
  notYet.value = null
  try {
    const ws = await workspaces.importExisting(dir)
    await projects.load(ws.id)
    app.toast('success', `Workspace “${ws.name}” connected`)
  } catch (e) {
    const err = toIpcError(e)
    error.value = err.message
    if (err.code === 'not_found') notYet.value = dir
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="onboarding">
    <div class="glow" aria-hidden="true" />
    <div class="content">
      <div class="hero">
        <svg viewBox="0 0 1024 1024" class="mark" aria-hidden="true">
          <defs>
            <linearGradient id="ob-g" x1="0" y1="1" x2="1" y2="0">
              <stop offset="0" stop-color="#7c6cf2" />
              <stop offset="1" stop-color="#3ad1c5" />
            </linearGradient>
          </defs>
          <path d="M332 712 V312 L692 712 V312" fill="none" stroke="url(#ob-g)" stroke-width="72" stroke-linecap="round" stroke-linejoin="round" />
          <circle cx="332" cy="712" r="70" fill="#0b0d12" stroke="#7c6cf2" stroke-width="30" />
          <circle cx="332" cy="312" r="70" fill="#0b0d12" stroke="#8a82f0" stroke-width="30" />
          <circle cx="692" cy="712" r="70" fill="#0b0d12" stroke="#4fc3d6" stroke-width="30" />
          <circle cx="692" cy="312" r="70" fill="#3ad1c5" />
        </svg>
        <h1>Welcome to NEXUS</h1>
        <p>Your development universe, connected. Start with a <strong>workspace</strong> — a folder that groups your projects and the knowledge that should travel with them.</p>
      </div>

      <div v-if="mode === 'choose'" class="choices">
        <button class="choice" :disabled="busy" data-onboarding-create @click="createWith('new')">
          <FolderPlus />
          <div>
            <h3>Create a workspace</h3>
            <p>In a new folder, an empty folder, or an empty Git repository you already created for it.</p>
          </div>
          <ArrowRight class="arrow" />
        </button>
        <button class="choice" :disabled="busy" @click="importExisting">
          <FolderInput />
          <div>
            <h3>Open an existing workspace</h3>
            <p>A folder (for example a cloned repository) that already contains <code>nexus-workspace.json</code>.</p>
          </div>
          <ArrowRight class="arrow" />
        </button>
        <button class="choice" :disabled="busy" data-onboarding-clone @click="mode = 'clone'">
          <GitBranch />
          <div>
            <h3>Open a workspace from Git</h3>
            <p>Clone your workspace repository to this machine — knowledge, rules and workflows come along; profiles, sessions and usage stay machine-local.</p>
          </div>
          <ArrowRight class="arrow" />
        </button>
      </div>

      <div v-else-if="mode === 'clone'" class="card create">
        <CloneWorkspaceForm offer-setup @cloned="cloned" @setup="(url) => createWith('git', { url })" @cancel="mode = 'choose'" />
      </div>

      <div v-else class="card create">
        <NewWorkspaceForm :key="start.key" :source="start.source" :folder="start.folder" :url="start.url" @cancel="mode = 'choose'" />
      </div>

      <div v-if="error" class="notice notice-error selectable" data-onboarding-error>
        <TriangleAlert />
        <span>
          {{ error }}
          <button v-if="notYet" class="btn btn-sm" data-onboarding-setup-here @click="createWith('existing', { folder: notYet })">Set up a new workspace in this folder</button>
        </span>
      </div>

      <div v-if="profiles.detected.length" class="found">
        <ShieldCheck />
        <span>
          Found {{ profiles.detected.length }} Claude configuration{{ profiles.detected.length === 1 ? '' : 's' }} on this machine
          ({{ profiles.detected.map((d) => d.suggestedName).join(', ') }}). You can register them next — NEXUS only reads them.
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.notice .btn {
  margin-left: 8px;
}
.onboarding {
  height: 100%;
  overflow: auto;
  display: flex;
  justify-content: center;
  position: relative;
}
.glow {
  position: absolute;
  top: -30%;
  left: 50%;
  width: 900px;
  height: 700px;
  transform: translateX(-50%);
  background: radial-gradient(closest-side, var(--accent-soft), transparent);
  pointer-events: none;
}
.content {
  position: relative;
  width: 640px;
  max-width: calc(100% - 48px);
  padding: 9vh 0 48px;
  display: flex;
  flex-direction: column;
  gap: 22px;
}
.hero {
  text-align: center;
}
.mark {
  width: 72px;
  height: 72px;
}
.hero h1 {
  margin: 10px 0 8px;
  font-size: 30px;
  letter-spacing: -0.02em;
}
.hero p {
  margin: 0 auto;
  max-width: 520px;
  color: var(--text-muted);
  font-size: 15px;
}
.choices {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.choice {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 18px 20px;
  text-align: left;
  border-radius: 14px;
  border: 1px solid var(--border-strong);
  background: var(--bg-panel);
  cursor: pointer;
  transition: border-color 0.12s, transform 0.12s;
}
.choice:hover:not(:disabled) {
  border-color: var(--accent);
  transform: translateY(-1px);
}
.choice > svg {
  width: 26px;
  height: 26px;
  color: var(--accent-strong);
  flex: none;
}
.choice h3 {
  margin: 0 0 4px;
  font-size: 15px;
}
.choice p {
  margin: 0;
  color: var(--text-muted);
  font-size: 13px;
}
.choice .arrow {
  width: 18px;
  height: 18px;
  color: var(--text-faint);
  margin-left: auto;
}
.found {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  font-size: 13px;
  color: var(--text-muted);
}
.found svg {
  width: 16px;
  height: 16px;
  color: var(--green);
  flex: none;
  margin-top: 2px;
}
</style>
