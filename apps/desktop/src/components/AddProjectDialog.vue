<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { Download, FolderOpen, GitBranch, Link, TriangleAlert } from '@lucide/vue'
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

/** `folder`: register a folder already on this computer; `git`: clone a repository first. */
const mode = ref<'folder' | 'git'>('folder')
const inspection = ref<PathInspection | null>(null)
const name = ref('')
const busy = ref(false)
const error = ref<string | null>(null)
const url = ref('')
/** Empty = the workspace's clone folder (`projects/<name>`, portable); otherwise a folder of the person's choice. */
const into = ref('')

watch(
  () => props.open,
  (o) => {
    if (o) {
      mode.value = 'folder'
      inspection.value = null
      name.value = ''
      error.value = null
      url.value = ''
      into.value = ''
    }
  },
)

const sep = computed(() => (app.info?.platform === 'windows' ? '\\' : '/'))
const urlValid = computed(() => /^(https:\/\/|ssh:\/\/|file:\/\/|git@[^:]+:)/.test(url.value.trim()))
const repoName = computed(() => url.value.trim().replace(/[\\/]+$/, '').split(/[\\/:]/).pop()?.replace(/\.git$/i, '') ?? '')
// Same rule as the core's `context::slug` (lowercase ASCII letters/digits and `-`, at most 48 characters).
const slug = computed(() => (name.value.trim() || repoName.value).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+/, '').slice(0, 48).replace(/-+$/, '') || 'project')
/** Where the clone will go, shown before anything happens. */
const cloneTarget = computed(() => into.value.trim() || (workspaces.active ? `${workspaces.active.rootPath}${sep.value}projects${sep.value}${slug.value}` : ''))

async function pickInto() {
  const dir = await openDialog({ directory: true, title: 'Choose an empty folder for the clone' })
  if (typeof dir === 'string') into.value = dir
}

async function cloneAndAdd() {
  if (!urlValid.value) return
  busy.value = true
  error.value = null
  try {
    const p = await projects.cloneNew(url.value.trim(), name.value.trim() || null, into.value.trim() || null)
    app.toast('success', `${p.name} cloned and added to ${workspaces.active?.name}`)
    emit('close')
    router.push(`/projects/${p.id}`)
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

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
  <BaseModal :open="open" title="Add a project" subtitle="NEXUS only reads the project — nothing in the repository is changed or executed." :width="600" @close="emit('close')">
    <div class="modes" role="tablist" aria-label="How to add the project">
      <button class="mode" :class="{ selected: mode === 'folder' }" role="tab" :aria-selected="mode === 'folder'" data-add-project-mode="folder" @click="mode = 'folder'">
        <FolderOpen /> Folder on this computer
      </button>
      <button class="mode" :class="{ selected: mode === 'git' }" role="tab" :aria-selected="mode === 'git'" data-add-project-mode="git" @click="mode = 'git'">
        <Download /> Clone from Git
      </button>
    </div>

    <div v-if="mode === 'git'" class="git" data-add-project-git>
      <div class="field">
        <label for="project-url">Repository URL</label>
        <input id="project-url" v-model="url" class="input mono" placeholder="https://github.com/you/app.git or git@host:you/app.git" data-add-project-url />
        <span class="hint">Your Git tool and its credential manager handle sign-in — NEXUS never asks for or stores credentials.</span>
      </div>
      <div class="field">
        <label for="project-clone-name">Name</label>
        <input id="project-clone-name" v-model="name" class="input" :placeholder="repoName || 'From the repository'" data-add-project-name />
      </div>
      <div class="field">
        <label for="project-into">Clone into</label>
        <div class="input-row">
          <input id="project-into" v-model="into" class="input mono" placeholder="The workspace's projects/ folder (recommended)" data-add-project-into />
          <button class="btn" @click="pickInto">Browse…</button>
        </div>
        <span v-if="cloneTarget" class="hint mono" data-add-project-target>{{ cloneTarget }}</span>
        <span class="hint">{{ into.trim() ? 'Outside projects/, the location is remembered on this computer only.' : 'In projects/ (ignored by the workspace repository), another computer can clone it again from its remote with one click.' }}</span>
      </div>
    </div>

    <button v-if="mode === 'folder'" class="btn picker" :disabled="busy" @click="choose">
      <FolderOpen /> {{ inspection ? 'Choose a different folder…' : 'Choose folder…' }}
    </button>

    <div v-if="mode === 'folder' && inspection" class="preview">
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
      <button v-if="mode === 'git'" class="btn btn-primary" :disabled="!urlValid || busy" data-add-project-clone @click="cloneAndAdd">
        <Download /> {{ busy ? 'Cloning…' : 'Clone and add' }}
      </button>
      <button v-else class="btn btn-primary" :disabled="!inspection || !!inspection.registeredAs || busy" @click="register">Add project</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.modes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-bottom: 14px;
}
.mode {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 36px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-muted);
  cursor: pointer;
  font: inherit;
  font-size: 13px;
}
.mode svg {
  width: 15px;
  height: 15px;
}
.mode.selected {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--text);
}
.git {
  display: flex;
  flex-direction: column;
  gap: 12px;
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
