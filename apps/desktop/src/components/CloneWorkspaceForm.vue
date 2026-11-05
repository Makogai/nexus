<script setup lang="ts">
// Onboard a workspace from an existing Git repository (ADR-0031): clone into a new folder, check it, open it.
// Used on the welcome screen and in the Sync Center. Credentials stay with Git; NEXUS never asks for them.
import { computed, ref } from 'vue'
import { GitBranch } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, toIpcError } from '../lib/ipc'
import type { SyncCloned } from '../lib/types'
import { useWorkspaceStore } from '../stores/workspaces'
import { useProjectStore } from '../stores/projects'

const emit = defineEmits<{ cloned: [SyncCloned]; cancel: [] }>()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const url = ref('')
const parent = ref('')
const name = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

const suggested = computed(() => {
  const base = url.value.trim().replace(/[\\/]+$/, '').split(/[\\/:]/).pop() ?? ''
  return base.replace(/\.git$/i, '') || 'workspace'
})
const folder = computed(() => {
  if (!parent.value) return ''
  const sep = parent.value.includes('\\') ? '\\' : '/'
  return `${parent.value.replace(/[\\/]+$/, '')}${sep}${(name.value || suggested.value).trim()}`
})
const valid = computed(() => /^(https:\/\/|ssh:\/\/|file:\/\/|git@[^:]+:)/.test(url.value.trim()) && !!folder.value)

async function pick() {
  const dir = await openDialog({ directory: true, title: 'Where should the workspace folder be created?' })
  if (typeof dir === 'string') parent.value = dir
}

async function start() {
  busy.value = true
  error.value = null
  try {
    const r = await api.syncClone(url.value.trim(), folder.value)
    // Tell the parent first: loading the stores makes the onboarding screen give way to the app, which would unmount
    // the listener before it could navigate to the restoration steps.
    emit('cloned', r)
    await workspaces.load()
    await projects.load(r.workspace.id)
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="clone" data-sync-clone>
    <div class="field">
      <label for="clone-url">Repository URL</label>
      <input id="clone-url" v-model="url" class="input mono" placeholder="https://github.com/team/workspace.git or git@host:team/workspace.git" data-sync-clone-url />
      <span class="hint">Your Git tool and its credential manager handle sign-in — NEXUS never asks for or stores credentials.</span>
    </div>
    <div class="field">
      <label>Folder on this machine</label>
      <div class="input-row">
        <input v-model="parent" class="input mono" placeholder="Choose a parent folder…" data-sync-clone-parent />
        <button class="btn" @click="pick">Browse…</button>
      </div>
      <input v-model="name" class="input mono" :placeholder="suggested" aria-label="Folder name" data-sync-clone-name />
      <span v-if="folder" class="hint mono">{{ folder }} (must be new or empty)</span>
    </div>
    <p v-if="error" class="notice notice-error selectable" data-sync-clone-error>{{ error }}</p>
    <div class="row">
      <button class="btn btn-ghost" @click="emit('cancel')">Back</button>
      <span class="spacer" />
      <button class="btn btn-primary" :disabled="!valid || busy" data-sync-clone-start @click="start"><GitBranch /> {{ busy ? 'Cloning…' : 'Open from Git' }}</button>
    </div>
  </div>
</template>

<style scoped>
.clone {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field label {
  font-size: 13px;
  font-weight: 600;
}
.hint {
  font-size: 12px;
  color: var(--text-faint);
}
.input-row {
  display: flex;
  gap: 8px;
}
.row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.spacer {
  flex: 1;
}
</style>
