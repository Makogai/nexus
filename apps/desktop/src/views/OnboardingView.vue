<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { ArrowRight, FolderInput, FolderPlus, GitBranch, ShieldCheck, Sparkles, TriangleAlert } from '@lucide/vue'
import CloneWorkspaceForm from '../components/CloneWorkspaceForm.vue'
import type { SyncCloned, TemplateSummary } from '../lib/types'
import { router } from '../router'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, toIpcError } from '../lib/ipc'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const profiles = useProfileStore()
const projects = useProjectStore()

const mode = ref<'choose' | 'create' | 'clone'>('choose')
const name = ref('Personal')
const parentDir = ref<string | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)
const templates = ref<TemplateSummary[]>([])
/** Starter template (ADR-0037); null = an empty workspace. */
const template = ref<string | null>('developer')

const sep = computed(() => (app.info?.platform === 'windows' ? '\\' : '/'))
const slug = computed(() => name.value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'workspace')
const targetPath = computed(() => (parentDir.value ? `${parentDir.value}${sep.value}${slug.value}` : null))

onMounted(() => {
  profiles.detect().catch(() => undefined)
  api.templateList().then((t) => (templates.value = t)).catch(() => undefined)
})

async function pickParent() {
  const dir = await openDialog({ directory: true, title: 'Where should the workspace folder be created?' })
  if (typeof dir === 'string') parentDir.value = dir
}

async function create() {
  if (!targetPath.value) return
  busy.value = true
  error.value = null
  try {
    const ws = await workspaces.create(targetPath.value, name.value.trim(), undefined, template.value)
    await projects.load(ws.id)
    app.toast('success', `Workspace “${ws.name}” created`)
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
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
  try {
    const ws = await workspaces.importExisting(dir)
    await projects.load(ws.id)
    app.toast('success', `Workspace “${ws.name}” connected`)
  } catch (e) {
    error.value = toIpcError(e).message
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
        <button class="choice" :disabled="busy" @click="mode = 'create'">
          <FolderPlus />
          <div>
            <h3>Create a workspace</h3>
            <p>A new folder you can later turn into a Git repository to sync between machines.</p>
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
        <CloneWorkspaceForm @cloned="cloned" @cancel="mode = 'choose'" />
      </div>

      <div v-else class="card create">
        <div class="field">
          <label for="ws-name">Workspace name</label>
          <input id="ws-name" v-model="name" class="input" autofocus @keydown.enter="create" />
          <span class="hint">Keep work and personal projects in separate workspaces.</span>
        </div>
        <div class="field">
          <label>Location</label>
          <div class="input-row">
            <input class="input mono" :value="targetPath ?? ''" placeholder="Choose a parent folder…" readonly @click="pickParent" />
            <button class="btn" @click="pickParent">Browse…</button>
          </div>
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
              <span class="muted">Only the workspace manifest — add constitutions, skills and workflows yourself.</span>
            </button>
          </div>
          <span class="hint">Only a small core agreement is always in Claude's context; the rest is read when relevant. You can add or update a template later.</span>
        </div>
        <div class="row">
          <button class="btn btn-ghost" @click="mode = 'choose'">Back</button>
          <span class="spacer" />
          <button class="btn btn-primary" :disabled="!targetPath || !name.trim() || busy" @click="create">
            <Sparkles /> Create workspace
          </button>
        </div>
      </div>

      <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>

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
.template:hover {
  border-color: var(--border-strong);
}
.template.selected {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.template .muted {
  font-size: 12px;
  line-height: 1.45;
}
.template .counts {
  font-size: 11px;
  color: var(--text-faint);
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
