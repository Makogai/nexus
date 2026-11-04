<script setup lang="ts">
import { ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { FolderOpen, GitBranch, Link, TriangleAlert } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, toIpcError } from '../lib/ipc'
import type { PathInspection } from '../lib/types'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import { useAppStore } from '../stores/app'
import BaseModal from './BaseModal.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ close: [] }>()
const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const router = useRouter()

const inspection = ref<PathInspection | null>(null)
const name = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

watch(
  () => props.open,
  (o) => {
    if (o) {
      inspection.value = null
      name.value = ''
      error.value = null
    }
  },
)

async function choose() {
  const dir = await openDialog({ directory: true, title: 'Choose a project folder (usually a Git repository)' })
  if (typeof dir !== 'string' || !workspaces.active) return
  busy.value = true
  error.value = null
  try {
    inspection.value = await api.projectInspectPath(workspaces.active.id, dir)
    name.value = inspection.value.suggestedName
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

async function register() {
  if (!inspection.value) return
  busy.value = true
  error.value = null
  try {
    const p = await projects.register(inspection.value.path, name.value.trim() || undefined)
    app.toast('success', `${p.name} added to ${workspaces.active?.name}`)
    emit('close')
    router.push(`/projects/${p.id}`)
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="open" title="Add a project" subtitle="Register an existing folder. NEXUS only reads it — nothing in the repository is changed or executed." :width="600" @close="emit('close')">
    <button class="btn picker" :disabled="busy" @click="choose">
      <FolderOpen /> {{ inspection ? 'Choose a different folder…' : 'Choose folder…' }}
    </button>

    <div v-if="inspection" class="preview">
      <div class="field">
        <label for="project-name">Name</label>
        <input id="project-name" v-model="name" class="input" />
      </div>
      <dl>
        <dt>Folder</dt>
        <dd class="mono selectable">{{ inspection.path }}</dd>
        <dt>Repository</dt>
        <dd>
          <span v-if="inspection.gitRoot" class="row"><GitBranch class="ic" /> Git repository</span>
          <span v-else class="faint">Not a Git repository (you can still register it)</span>
        </dd>
        <template v-if="inspection.remoteUrl">
          <dt>Remote</dt>
          <dd class="row mono selectable"><Link class="ic" /> {{ inspection.remoteUrl }}</dd>
        </template>
        <dt>Detected</dt>
        <dd class="chips">
          <span v-for="s in inspection.stack.stacks" :key="s" class="chip">{{ s }}</span>
          <span v-for="pm in inspection.stack.packageManagers" :key="pm" class="chip">{{ pm }}</span>
          <span v-if="inspection.stack.hasClaudeMd" class="chip">CLAUDE.md</span>
          <span v-if="inspection.stack.hasAgentsMd" class="chip">AGENTS.md</span>
          <span v-if="!inspection.stack.stacks.length && !inspection.stack.packageManagers.length" class="faint">nothing recognised</span>
        </dd>
        <dt>Portability</dt>
        <dd class="faint">
          {{ inspection.insideWorkspace
            ? 'Inside the workspace folder — its relative path is stored in the workspace manifest.'
            : 'Outside the workspace folder — the location is remembered on this machine only.' }}
        </dd>
      </dl>
      <div v-if="inspection.registeredAs" class="notice notice-warn"><TriangleAlert /> This folder is already registered in this workspace.</div>
    </div>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>

    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <button class="btn btn-primary" :disabled="!inspection || !!inspection.registeredAs || busy" @click="register">Add project</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.picker {
  width: 100%;
  height: 44px;
  border-style: dashed;
}
.preview {
  margin-top: 18px;
}
dl {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 10px 14px;
  margin: 0 0 12px;
  font-size: 13px;
}
dt {
  color: var(--text-faint);
}
dd {
  margin: 0;
  min-width: 0;
  word-break: break-all;
}
.ic {
  width: 14px;
  height: 14px;
  flex: none;
}
</style>
