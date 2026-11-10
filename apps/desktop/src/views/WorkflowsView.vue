<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { listen } from '@tauri-apps/api/event'
import { Info, LoaderCircle, Play, Plus, Save, ShieldCheck, TriangleAlert, Workflow } from '@lucide/vue'
import { api, WORKFLOW_EVENT } from '../lib/ipc'
import type { Rigor, RunView, WorkflowRun, WorkflowSummary, WorkflowTemplate, WorkflowView } from '../lib/types'
import { relativeTime } from '../lib/format'
import { KIND_LABELS, RIGOR_LEVELS, includedSteps } from '../lib/workflow'
import { useAction } from '../composables/useAction'
import { useAppStore } from '../stores/app'
import { useWorkspaceStore } from '../stores/workspaces'
import ActionButton from '../components/ActionButton.vue'
import GatesPanel from '../components/GatesPanel.vue'
import StartWorkflowDialog from '../components/StartWorkflowDialog.vue'
import WorkflowRunDetail from '../components/WorkflowRunDetail.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const route = useRoute()

type Tab = 'library' | 'runs' | 'gates'
const tab = ref<Tab>('library')
const list = ref<WorkflowSummary[]>([])
const templates = ref<WorkflowTemplate[]>([])
const selected = ref<WorkflowView | null>(null)
const raw = ref('')
const editing = ref(false)
const previewRigor = ref<Rigor>('balanced')
const skillsText = ref('')
const runs = ref<WorkflowRun[]>([])
const run = ref<RunView | null>(null)
const starting = ref<{ workflowId?: string | null; projectId?: string | null; taskId?: string | null } | null>(null)
const picking = ref(false)

const wsId = computed(() => workspaces.active?.id ?? null)
const included = computed(() => (selected.value ? includedSteps(selected.value.definition, previewRigor.value) : []))

async function loadLibrary() {
  if (!wsId.value) return
  list.value = (await app.attempt(() => api.workflowList(wsId.value!), 'Could not load workflows')) ?? list.value
  if (!templates.value.length) templates.value = await api.workflowTemplates().catch(() => [])
}
async function select(id: string) {
  if (!wsId.value) return
  const v = await app.attempt(() => api.workflowGet(wsId.value!, id), 'Could not open the workflow')
  if (v) {
    selected.value = v
    raw.value = v.body
    editing.value = false
    previewRigor.value = v.rigor
    skillsText.value = v.skills.join(', ')
  }
}
const opening = useAction(select, { key: (id: string) => id })
// The picker stays open (its card busy) until the workflow exists, so a failure can be retried.
const creating = useAction(
  async (t: WorkflowTemplate) => {
    if (!wsId.value) return
    const v = await api.workflowCreate(wsId.value, { template: t.id })
    picking.value = false
    await loadLibrary()
    await select(v.id)
    app.toast('success', `Created “${v.title}” — a Markdown file in workflows/, yours to edit`)
  },
  { failure: 'Could not create the workflow', key: (t: WorkflowTemplate) => t.id },
)
const savingRaw = useAction(
  async () => {
    if (!wsId.value || !selected.value) return
    selected.value = await api.workflowSaveRaw(wsId.value, selected.value.id, raw.value, selected.value.hash)
    editing.value = false
    app.toast('success', 'Workflow saved')
    await loadLibrary()
  },
  { failure: 'Not saved' },
)
const savingSettings = useAction(
  async () => {
    if (!wsId.value || !selected.value) return
    const skills = skillsText.value.split(',').map((s) => s.trim()).filter(Boolean)
    selected.value = await api.workflowSave(wsId.value, selected.value.id, selected.value.definition, previewRigor.value, skills, selected.value.hash)
    app.toast('success', 'Default rigor and skills saved')
  },
  { failure: 'Could not save' },
)
const approving = useAction(
  async () => {
    if (!wsId.value || !selected.value) return
    const id = selected.value.id
    await api.knowledgeApprove(wsId.value, id)
    await select(id)
    await loadLibrary()
  },
  { failure: 'Could not approve' },
)

async function loadRuns() {
  if (!wsId.value) return
  runs.value = (await api.workflowRuns(wsId.value, { limit: 100 }).catch(() => runs.value)) ?? []
  if (run.value) run.value = await api.workflowRun(run.value.id).catch(() => run.value)
}
async function openRun(id: string) {
  tab.value = 'runs'
  run.value = (await app.attempt(() => api.workflowRun(id), 'Could not open the run')) ?? null
}
const runOpening = useAction(openRun, { key: (id: string) => id })
async function started(v: RunView) {
  starting.value = null
  await loadRuns()
  await openRun(v.id)
}

let unlisten: (() => void) | null = null
onMounted(async () => {
  unlisten = await listen(WORKFLOW_EVENT, () => loadRuns())
})
onBeforeUnmount(() => unlisten?.())
watch(() => app.externalChanges, () => tab.value === 'runs' && loadRuns())
watch(tab, (t) => t === 'runs' && loadRuns())
watch(wsId, () => {
  selected.value = null
  run.value = null
  loadLibrary()
})
watch(
  () => [route.name, route.query.run, route.query.start],
  async () => {
    if (route.name !== 'workflows') return
    if (typeof route.query.run === 'string') await openRun(route.query.run)
    if (route.query.start) {
      starting.value = {
        workflowId: typeof route.query.workflow === 'string' ? route.query.workflow : null,
        projectId: typeof route.query.project === 'string' ? route.query.project : null,
        taskId: typeof route.query.task === 'string' ? route.query.task : null,
      }
    }
  },
  { immediate: true },
)
onActivated(loadLibrary)
loadLibrary()
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Workflows</h1>
        <p>Repeatable procedures on your tasks: Claude works and reports each step, NEXUS verifies with your project's gates, and you sign off where it matters. Nothing here replaces Claude's own permissions.</p>
      </div>
      <div class="page-actions">
        <button class="btn" data-new-workflow @click="picking = !picking"><Plus /> New from template</button>
        <button class="btn btn-primary" data-start-dialog @click="starting = { workflowId: selected?.id ?? null }"><Play /> Start workflow</button>
      </div>
    </div>

    <div v-if="picking" class="templates" data-templates>
      <button
        v-for="t in templates"
        :key="t.id"
        class="card tpl"
        :disabled="creating.pending.value"
        :aria-busy="creating.isPending(t.id) ? 'true' : undefined"
        :data-template="t.id"
        @click="creating.run(t)"
      >
        <strong>{{ t.title }} <LoaderCircle v-if="creating.isPending(t.id)" class="spin pending-ic" aria-hidden="true" /></strong>
        <span class="badge">{{ t.category }}</span>
        <span class="small faint">{{ t.summary }}</span>
      </button>
    </div>

    <div class="tabs" role="tablist">
      <button v-for="t in (['library', 'runs', 'gates'] as Tab[])" :key="t" role="tab" class="tab" :class="{ active: tab === t }" :aria-selected="tab === t" :data-tab="t" @click="tab = t">
        {{ t === 'library' ? 'Library' : t === 'runs' ? 'Runs' : 'Gates' }}
      </button>
    </div>

    <!-- Library -->
    <template v-if="tab === 'library'">
      <div v-if="!list.length" class="empty">
        <Workflow class="empty-icon" />
        <h3>No workflows yet</h3>
        <p>Start from a template — feature implementation, bug investigation, code review, refactoring, test coverage or release preparation.</p>
        <button class="btn btn-primary" @click="picking = true"><Plus /> New from template</button>
      </div>
      <div v-else class="layout">
        <nav class="wf-list" aria-label="Workflows">
          <button
            v-for="w in list"
            :key="w.id"
            class="wf"
            :class="{ active: selected?.id === w.id }"
            :disabled="opening.isPending(w.id)"
            :aria-busy="opening.isPending(w.id) ? 'true' : undefined"
            :data-workflow="w.title"
            @click="opening.run(w.id)"
          >
            <span class="wf-title">{{ w.title }} <LoaderCircle v-if="opening.isPending(w.id)" class="spin pending-ic" aria-hidden="true" /></span>
            <span class="faint small">{{ w.category }} · {{ w.steps }} steps · {{ w.rigor }}</span>
            <span v-if="!w.approved" class="badge badge-warn">needs approval</span>
            <span v-else-if="w.errors" class="badge badge-error">errors</span>
          </button>
        </nav>
        <section v-if="selected" class="detail" data-workflow-detail>
          <header class="detail-head">
            <div>
              <h2>{{ selected.title }}</h2>
              <p class="faint small mono">{{ selected.relativePath }}</p>
            </div>
            <div class="row gap">
              <button class="btn btn-sm" @click="editing = !editing">{{ editing ? 'Visual' : 'Markdown' }}</button>
              <button class="btn btn-sm btn-primary" :disabled="!selected.approved || selected.errors > 0" data-start-selected @click="starting = { workflowId: selected.id }"><Play /> Start</button>
            </div>
          </header>
          <div v-if="!selected.approved" class="notice notice-warn approve" data-review-workflow>
            <TriangleAlert /> <span>This version changed outside NEXUS (e.g. through Git). Review it — it cannot run until you approve this exact version.</span>
            <ActionButton class="btn-sm btn-primary" :pending="approving.pending.value" pending-label="Approving…" data-approve-workflow @click="approving.run()"><template #icon><ShieldCheck /></template> Approve</ActionButton>
          </div>
          <div v-for="p in selected.problems.filter((x) => x.severity === 'error')" :key="p.message" class="notice notice-error"><TriangleAlert /> {{ p.message }}</div>

          <div class="settings">
            <label class="field-inline"><span>Default rigor</span>
              <select v-model="previewRigor" class="select" data-preview-rigor>
                <option v-for="r in RIGOR_LEVELS" :key="r.id" :value="r.id">{{ r.name }}</option>
              </select>
            </label>
            <label class="field-inline grow"><span>Skills for its sessions only</span><input v-model="skillsText" class="input mono" placeholder="kit:skill, kit:other" /></label>
            <ActionButton class="btn-sm" :pending="savingSettings.pending.value" pending-label="Saving…" data-save-settings @click="savingSettings.run()"><template #icon><Save /></template> Save</ActionButton>
          </div>

          <ol v-if="!editing" class="steps" data-steps>
            <li v-for="s in selected.definition.steps" :key="s.id" class="step" :class="{ excluded: !included.includes(s.id) }" :data-def-step="s.id">
              <div class="row-head">
                <span class="mono sid">{{ s.id }}</span>
                <strong>{{ s.title }}</strong>
                <span class="badge" :class="{ 'badge-live': s.kind === 'check', 'badge-accent': s.kind === 'review' }">{{ KIND_LABELS[s.kind] }}</span>
                <span v-for="g in s.gates" :key="g" class="chip">gate: {{ g }}</span>
                <span v-if="s.rigor" class="faint small">from {{ s.rigor }}</span>
                <span v-if="!included.includes(s.id)" class="faint small">— not at {{ previewRigor }}</span>
              </div>
              <p v-if="s.needs.length" class="small faint">after {{ s.needs.join(', ') }}</p>
              <p v-if="s.text" class="small text">{{ s.text }}</p>
              <ul v-if="s.criteria.length" class="small crit"><li v-for="c in s.criteria" :key="c">☐ {{ c }}</li></ul>
            </li>
          </ol>
          <template v-else>
            <p class="hint">## step title · <code>&lt;!-- step: id=… kind=claude|check|review needs=a,b gates=test,lint rigor=professional --&gt;</code> · ### Acceptance with - items. Gates are names only.</p>
            <textarea v-model="raw" class="textarea mono raw" spellcheck="false" data-workflow-raw />
            <div class="row gap">
              <ActionButton class="btn-primary" :pending="savingRaw.pending.value" pending-label="Saving…" data-save-workflow @click="savingRaw.run()"><template #icon><Save /></template> Save</ActionButton>
              <button class="btn" :disabled="savingRaw.pending.value" @click="(raw = selected.body), (editing = false)">Discard</button>
            </div>
          </template>
          <p class="notice small"><Info /> Claude steps are <em>AI-reported</em> unless gates verify them. At critical rigor, an AI-reported step without gates also needs your sign-off.</p>
        </section>
      </div>
    </template>

    <!-- Runs -->
    <div v-else-if="tab === 'runs'" class="layout">
      <nav class="wf-list" aria-label="Runs">
        <button
          v-for="r in runs"
          :key="r.id"
          class="wf"
          :class="{ active: run?.id === r.id }"
          :disabled="runOpening.isPending(r.id)"
          :aria-busy="runOpening.isPending(r.id) ? 'true' : undefined"
          :data-run-item="r.id"
          @click="runOpening.run(r.id)"
        >
          <span class="wf-title">{{ r.workflowTitle }} <LoaderCircle v-if="runOpening.isPending(r.id)" class="spin pending-ic" aria-hidden="true" /></span>
          <span class="faint small">{{ r.goal || r.rigor }} · {{ relativeTime(r.updatedAt) }}</span>
          <span class="badge" :class="{ 'badge-ok': r.status === 'completed', 'badge-error': r.status === 'failed', 'badge-live': r.status === 'running', 'badge-warn': r.status === 'waiting' }">{{ r.status }}</span>
        </button>
        <p v-if="!runs.length" class="faint small">No runs yet.</p>
      </nav>
      <WorkflowRunDetail v-if="run" :run="run" @changed="(v) => ((run = v), loadRuns())" />
    </div>

    <GatesPanel v-else />

    <StartWorkflowDialog v-if="wsId && starting" :open="!!starting" :workspace-id="wsId" :workflow-id="starting.workflowId" :project-id="starting.projectId" :task-id="starting.taskId" @close="starting = null" @started="started" />
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 6px;
  margin: 12px 0 14px;
}
.tab {
  height: 30px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: transparent;
  cursor: pointer;
}
.tab.active {
  background: var(--bg-active);
  border-color: var(--border-strong);
  font-weight: 600;
}
.templates {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
  margin-bottom: 8px;
}
.tpl {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  text-align: left;
  cursor: pointer;
}
.tpl:hover {
  border-color: var(--accent);
}
.layout {
  display: grid;
  grid-template-columns: 280px 1fr;
  gap: 16px;
}
.wf-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.wf {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: pointer;
  text-align: left;
}
.wf.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.wf-title {
  font-weight: 600;
}
.pending-ic {
  width: 13px;
  height: 13px;
  vertical-align: -2px;
}
.small {
  font-size: 12.5px;
}
.detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.detail-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
}
.detail-head h2 {
  margin: 0;
}
.gap {
  gap: 8px;
}
.approve {
  align-items: center;
}
.settings {
  display: flex;
  align-items: center;
  gap: 12px;
}
.field-inline {
  display: flex;
  align-items: center;
  gap: 8px;
}
.field-inline.grow {
  flex: 1;
}
.field-inline.grow .input {
  flex: 1;
}
.steps {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.step {
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
}
.step.excluded {
  opacity: 0.5;
}
.row-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.sid {
  color: var(--accent-strong);
  font-size: 12px;
}
.text {
  white-space: pre-wrap;
  color: var(--text-muted);
  margin: 6px 0 0;
}
.crit {
  list-style: none;
  padding: 0;
  margin: 6px 0 0;
}
.raw {
  min-height: 420px;
  font-size: 12.5px;
}
</style>
