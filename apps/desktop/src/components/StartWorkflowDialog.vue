<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { TriangleAlert } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { KnowledgeItem, Rigor, RunView, WorkflowSummary } from '../lib/types'
import { RIGOR_LEVELS } from '../lib/workflow'
import { useAction } from '../composables/useAction'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import ActionButton from './ActionButton.vue'
import BaseModal from './BaseModal.vue'

const props = defineProps<{
  open: boolean
  workspaceId: string
  workflowId?: string | null
  projectId?: string | null
  taskId?: string | null
}>()
const emit = defineEmits<{ close: []; started: [run: RunView] }>()
const app = useAppStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const sessions = useSessionStore()

const workflows = ref<WorkflowSummary[]>([])
const tasks = ref<KnowledgeItem[]>([])
const workflowId = ref('')
const projectId = ref('')
const taskMode = ref<'new' | 'existing'>('new')
const taskId = ref('')
const goal = ref('')
const rigor = ref<Rigor>('balanced')
const profileId = ref('')
const launch = ref(true)
const loaded = ref(false)

const wf = computed(() => workflows.value.find((w) => w.id === workflowId.value) ?? null)
const ready = computed(() => !!wf.value && wf.value.approved && !wf.value.errors && !!projectId.value && (taskMode.value === 'new' || !!taskId.value) && (!launch.value || !!profileId.value))

// Errors stay in the dialog until the next attempt (or until it is opened again).
const starting = useAction(async () => {
  if (!wf.value) return
  const run = await api.workflowStart(props.workspaceId, {
    workflowId: wf.value.id,
    projectId: projectId.value || null,
    taskId: taskMode.value === 'existing' ? taskId.value : null,
    goal: goal.value.trim(),
    rigor: rigor.value,
    profileId: launch.value ? profileId.value : null,
  })
  if (launch.value && profileId.value) {
    await sessions.startClaude({ profileId: profileId.value, projectId: run.projectId, taskId: run.taskId, workProfile: run.rigor, title: run.goal ? `${run.workflowTitle}: ${run.goal}`.slice(0, 80) : run.workflowTitle })
  }
  emit('started', run)
})

watch(
  () => props.open,
  async (open) => {
    if (!open) return
    loaded.value = false
    starting.clearError()
    goal.value = ''
    workflows.value = await api.workflowList(props.workspaceId).catch(() => [])
    const o = await api.knowledgeOverview(props.workspaceId).catch(() => null)
    tasks.value = (o?.items ?? []).filter((i) => i.valid && i.kind === 'task' && i.status !== 'done')
    workflowId.value = props.workflowId ?? workflows.value.find((w) => w.approved && !w.errors)?.id ?? ''
    rigor.value = wf.value?.rigor ?? 'balanced'
    projectId.value = props.projectId ?? projects.list[0]?.id ?? ''
    taskMode.value = props.taskId ? 'existing' : 'new'
    taskId.value = props.taskId ?? ''
    profileId.value = app.prefs.lastLaunch?.profileId ?? profiles.list[0]?.id ?? ''
    loaded.value = true
  },
  { immediate: true },
)
// Choosing another workflow applies its default rigor (the user can still change it afterwards).
function pickWorkflow(id: string) {
  workflowId.value = id
  if (wf.value) rigor.value = wf.value.rigor
}
</script>

<template>
  <BaseModal :open="open" title="Start a workflow" subtitle="A run follows the workflow's steps on a task. Claude reports steps; NEXUS verifies them with your project's gates; review steps wait for you." :width="620" @close="emit('close')">
    <div class="field">
      <label for="wf-pick">Workflow</label>
      <select id="wf-pick" :value="workflowId" class="select" data-start-workflow :data-loaded="loaded || undefined" @change="pickWorkflow(($event.target as HTMLSelectElement).value)">
        <option value="" disabled>Choose…</option>
        <option v-for="w in workflows" :key="w.id" :value="w.id" :disabled="!w.approved || w.errors > 0">{{ w.title }}{{ !w.approved ? ' (needs approval)' : w.errors ? ' (has errors)' : '' }}</option>
      </select>
    </div>
    <div class="two">
      <div class="field">
        <label for="wf-project">Project</label>
        <select id="wf-project" v-model="projectId" class="select" data-start-project>
          <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </div>
      <div class="field">
        <label for="wf-rigor">Rigor (work profile)</label>
        <select id="wf-rigor" v-model="rigor" class="select" data-start-rigor>
          <option v-for="r in RIGOR_LEVELS" :key="r.id" :value="r.id">{{ r.name }}</option>
        </select>
      </div>
    </div>
    <div class="segments" role="radiogroup" aria-label="Task">
      <button class="segment" :class="{ selected: taskMode === 'new' }" role="radio" :aria-checked="taskMode === 'new'" @click="taskMode = 'new'">New task</button>
      <button class="segment" :class="{ selected: taskMode === 'existing' }" role="radio" :aria-checked="taskMode === 'existing'" @click="taskMode = 'existing'">Existing task</button>
    </div>
    <div v-if="taskMode === 'new'" class="field">
      <label for="wf-goal">Goal</label>
      <textarea id="wf-goal" v-model="goal" class="textarea" rows="2" placeholder="What should this run achieve?" data-start-goal />
    </div>
    <div v-else class="field">
      <label for="wf-task">Task</label>
      <select id="wf-task" v-model="taskId" class="select">
        <option value="" disabled>Choose…</option>
        <option v-for="t in tasks" :key="t.id ?? t.path" :value="t.id">{{ t.title }}</option>
      </select>
    </div>
    <div class="two">
      <label class="checkbox"><input v-model="launch" type="checkbox" /><span>Launch Claude for it now</span></label>
      <select v-if="launch" v-model="profileId" class="select" aria-label="Claude profile" data-start-profile>
        <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>
    <p v-if="wf" class="hint">
      {{ wf.steps }} steps · gates: {{ wf.gates.join(', ') || 'none' }}<template v-if="wf.skills.length"> · skills for this session only: {{ wf.skills.join(', ') }}</template>
    </p>
    <div v-if="starting.error.value" class="notice notice-error selectable" data-start-error><TriangleAlert /> {{ starting.error.value }}</div>
    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <ActionButton class="btn-primary" :pending="starting.pending.value" :disabled="!ready" pending-label="Starting…" data-start-run @click="starting.run()">{{ launch ? 'Start and launch Claude' : 'Start run' }}</ActionButton>
    </template>
  </BaseModal>
</template>

<style scoped>
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  align-items: center;
  margin-bottom: 12px;
}
.segments {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
  margin-bottom: 12px;
}
.segment {
  height: 30px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  font-weight: 600;
  cursor: pointer;
}
.segment.selected {
  background: var(--accent-soft);
  color: var(--accent-strong);
}
</style>
