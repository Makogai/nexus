<script setup lang="ts">
// Create a workspace (onboarding and Settings): in a new folder, in an existing empty folder (for example a clone of an
// empty repository), or straight from an empty Git repository the person prepared — NEXUS clones it and the first
// reviewed commit and push go there. A folder with other content (a project) never becomes a workspace (core check).
import { computed, onMounted, ref, watch } from 'vue'
import { FolderOpen, FolderPlus, GitBranch, Sparkles } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, toIpcError } from '../lib/ipc'
import type { TemplateSummary, Workspace } from '../lib/types'
import { router } from '../router'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

export type WorkspaceSource = 'new' | 'existing' | 'git'

const props = withDefaults(defineProps<{ source?: WorkspaceSource; folder?: string; url?: string; backLabel?: string }>(), {
  source: 'new',
  folder: '',
  url: '',
  backLabel: 'Back',
})
const emit = defineEmits<{ created: [Workspace]; cancel: [] }>()

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()

const source = ref<WorkspaceSource>(props.source)
const name = ref('')
const parent = ref('')
const folder = ref(props.source === 'existing' ? props.folder : '')
const url = ref(props.url)
const cloneName = ref('')
const templates = ref<TemplateSummary[]>([])
/** Starter template (ADR-0037); null = an empty workspace. */
const template = ref<string | null>('developer')
const busy = ref(false)
const error = ref<string | null>(null)

const sep = computed(() => (app.info?.platform === 'windows' ? '\\' : '/'))
const trimDir = (d: string) => d.trim().replace(/[\\/]+$/, '')
const baseName = (p: string) => trimDir(p).split(/[\\/:]/).pop()?.replace(/\.git$/i, '') ?? ''
const slug = computed(() => name.value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'workspace')
const repoFolderName = computed(() => (cloneName.value.trim() || baseName(url.value) || slug.value))
const urlValid = computed(() => /^(https:\/\/|ssh:\/\/|file:\/\/|git@[^:]+:)/.test(url.value.trim()))

/** The folder the workspace will live in. */
const target = computed(() => {
  if (source.value === 'existing') return trimDir(folder.value)
  if (!trimDir(parent.value)) return ''
  return `${trimDir(parent.value)}${sep.value}${source.value === 'git' ? repoFolderName.value : slug.value}`
})
const ready = computed(() => !!name.value.trim() && !!target.value && (source.value !== 'git' || urlValid.value) && !busy.value)

// A sensible default name from the folder or repository the person picked (never overwrites what they typed).
const typedName = ref(false)
watch([folder, url, source], () => {
  if (typedName.value) return
  const from = source.value === 'existing' ? baseName(folder.value) : source.value === 'git' ? baseName(url.value) : ''
  name.value = from ? from.charAt(0).toUpperCase() + from.slice(1) : name.value || 'Personal'
}, { immediate: true })

onMounted(() => {
  api.templateList().then((t) => (templates.value = t)).catch(() => undefined)
})

async function browse(kind: 'parent' | 'folder') {
  const dir = await openDialog({
    directory: true,
    title: kind === 'folder' ? 'Choose an empty folder (or a clone of an empty repository)' : 'Where should the workspace folder be created?',
  })
  if (typeof dir === 'string') (kind === 'folder' ? (folder.value = dir) : (parent.value = dir))
}

async function create() {
  if (!ready.value) return
  busy.value = true
  error.value = null
  try {
    const ws =
      source.value === 'git'
        ? await workspaces.createFromRepository(url.value.trim(), target.value, name.value.trim(), template.value)
        : await workspaces.create(target.value, name.value.trim(), undefined, template.value)
    emit('created', ws)
    await projects.load(ws.id)
    if (source.value === 'git') {
      app.toast('success', `Workspace “${ws.name}” set up in your repository — review, commit and push it from Sync`)
      await router.push({ name: 'sync' })
    } else {
      app.toast('success', `Workspace “${ws.name}” created`)
    }
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="new-ws" data-new-ws>
    <div class="field">
      <label>Where</label>
      <div class="sources" role="radiogroup" aria-label="Where the workspace lives">
        <button class="source" :class="{ selected: source === 'new' }" role="radio" :aria-checked="source === 'new'" data-new-ws-source="new" @click="source = 'new'">
          <FolderPlus /> <span><strong>New folder</strong><small>NEXUS creates it</small></span>
        </button>
        <button class="source" :class="{ selected: source === 'existing' }" role="radio" :aria-checked="source === 'existing'" data-new-ws-source="existing" @click="source = 'existing'">
          <FolderOpen /> <span><strong>Existing empty folder</strong><small>e.g. a clone of an empty repository</small></span>
        </button>
        <button class="source" :class="{ selected: source === 'git' }" role="radio" :aria-checked="source === 'git'" data-new-ws-source="git" @click="source = 'git'">
          <GitBranch /> <span><strong>Empty Git repository</strong><small>one you created on your Git host</small></span>
        </button>
      </div>
    </div>

    <div v-if="source === 'git'" class="field">
      <label for="new-ws-url">Repository URL</label>
      <input id="new-ws-url" v-model="url" class="input mono" placeholder="https://github.com/you/workspace.git or git@host:you/workspace.git" data-new-ws-url />
      <span class="hint">An empty repository — a README, licence or .gitignore from your Git host is fine. Your Git tool handles sign-in; NEXUS never asks for credentials.</span>
    </div>

    <div class="field">
      <label for="new-ws-name">Workspace name</label>
      <input id="new-ws-name" v-model="name" class="input" data-new-ws-name @input="typedName = true" @keydown.enter="create" />
      <span class="hint">Keep work and personal projects in separate workspaces.</span>
    </div>

    <div v-if="source === 'existing'" class="field">
      <label for="new-ws-folder">Folder</label>
      <div class="input-row">
        <input id="new-ws-folder" v-model="folder" class="input mono" placeholder="Choose an empty folder…" data-new-ws-folder />
        <button class="btn" @click="browse('folder')">Browse…</button>
      </div>
      <span class="hint">Empty, or only what a Git host adds to a new repository (README, licence, .gitignore — they stay). A project folder is added to a workspace later; it never becomes one.</span>
    </div>
    <div v-else class="field">
      <label for="new-ws-parent">{{ source === 'git' ? 'Clone into' : 'Location' }}</label>
      <div class="input-row">
        <input id="new-ws-parent" v-model="parent" class="input mono" placeholder="Choose a parent folder…" data-new-ws-parent />
        <button class="btn" @click="browse('parent')">Browse…</button>
      </div>
      <input v-if="source === 'git'" v-model="cloneName" class="input mono" :placeholder="repoFolderName" aria-label="Folder name" data-new-ws-clone-name />
      <span v-if="target" class="hint mono" data-new-ws-target>{{ target }}{{ source === 'git' ? ' (must be new or empty)' : '' }}</span>
    </div>

    <div v-if="templates.length" class="field">
      <label>Starter template</label>
      <div class="templates" role="radiogroup" aria-label="Starter template">
        <button
          v-for="t in templates"
          :key="t.id"
          class="template"
          :class="{ selected: template === t.id }"
          role="radio"
          :aria-checked="template === t.id"
          :data-template="t.id"
          @click="template = t.id"
        >
          <strong>{{ t.title }}</strong>
          <span class="muted">{{ t.summary }}</span>
          <span class="counts">
            {{ t.constitutions }} constitutions<template v-if="t.stackRuleSets"> · {{ t.stackRuleSets }} stack rule sets</template
            ><template v-if="t.skills"> · {{ t.skills }} skills</template><template v-if="t.workflows"> · {{ t.workflows }} workflows</template>
          </span>
        </button>
        <button class="template" :class="{ selected: template === null }" role="radio" :aria-checked="template === null" data-template="none" @click="template = null">
          <strong>Empty</strong>
          <span class="muted">The standard folders only — add constitutions, skills and workflows yourself.</span>
        </button>
      </div>
      <span class="hint">Only a small core agreement is always in Claude's context; the rest is read when relevant. You can add or update a template later.</span>
    </div>

    <p v-if="error" class="notice notice-error selectable" data-new-ws-error>{{ error }}</p>
    <div class="row">
      <button class="btn btn-ghost" @click="emit('cancel')">{{ backLabel }}</button>
      <span class="spacer" />
      <button class="btn btn-primary" :disabled="!ready" data-new-ws-create @click="create">
        <Sparkles /> {{ busy ? (source === 'git' ? 'Cloning…' : 'Creating…') : 'Create workspace' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.new-ws {
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
.sources {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}
.source {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  text-align: left;
  padding: 9px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text);
  cursor: pointer;
  font: inherit;
}
.source svg {
  width: 16px;
  height: 16px;
  flex: none;
  margin-top: 2px;
  color: var(--accent-strong);
}
.source span {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.source strong {
  font-size: 13px;
}
.source small {
  font-size: 11px;
  color: var(--text-faint);
}
.source.selected,
.template.selected {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.templates {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}
.template {
  display: flex;
  flex-direction: column;
  gap: 4px;
  text-align: left;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text);
  cursor: pointer;
  font: inherit;
}
.template:hover,
.source:hover {
  border-color: var(--border-strong);
}
.template .muted {
  font-size: 12px;
  line-height: 1.45;
}
.template .counts {
  font-size: 11px;
  color: var(--text-faint);
}
</style>
