<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { TriangleAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { ConstitutionView, KnowledgeItem, ScopeLevel } from '../lib/types'
import { SCOPE_LABELS, SCOPE_ORDER } from '../lib/constitution'
import { useProjectStore } from '../stores/projects'
import BaseModal from './BaseModal.vue'

const props = defineProps<{ open: boolean; workspaceId: string }>()
const emit = defineEmits<{ close: []; created: [view: ConstitutionView] }>()
const projects = useProjectStore()

const title = ref('')
const scope = ref<ScopeLevel>('workspace')
const stacks = ref('')
const projectId = ref('')
const taskId = ref('')
const prefix = ref('')
const applies = ref<'always' | 'on-demand'>('always')
const starter = ref(true)
const tasks = ref<KnowledgeItem[]>([])
const busy = ref(false)
const error = ref<string | null>(null)

watch(
  () => props.open,
  async (open) => {
    if (!open) return
    error.value = null
    title.value = ''
    const o = await api.knowledgeOverview(props.workspaceId).catch(() => null)
    tasks.value = (o?.items ?? []).filter((i) => i.valid && (i.kind === 'task' || i.kind === 'workflow') && i.status !== 'done')
  },
)

const allStacks = computed(() => [...new Set(projects.list.flatMap((p) => p.stack.stacks))].sort())
const ready = computed(
  () =>
    !!title.value.trim() &&
    (scope.value !== 'stack' || !!stacks.value.trim()) &&
    (scope.value !== 'project' || !!projectId.value) &&
    (scope.value !== 'task' || !!taskId.value),
)

async function create() {
  busy.value = true
  error.value = null
  try {
    const view = await api.constitutionCreate(props.workspaceId, {
      title: title.value.trim(),
      scope: scope.value,
      stacks: stacks.value.split(',').map((s) => s.trim()).filter(Boolean),
      projectId: scope.value === 'project' ? projectId.value : null,
      taskId: scope.value === 'task' ? taskId.value : null,
      prefix: prefix.value.trim() || null,
      applies: applies.value,
      starter: starter.value,
    })
    emit('created', view)
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="open" title="New constitution" subtitle="Structured rules for Claude: articles, sections and rules with stable ids. Stored as a Markdown file in the workspace (or on this machine for global ones)." :width="620" @close="emit('close')">
    <div class="field">
      <label for="c-title">Title</label>
      <input id="c-title" v-model="title" class="input" placeholder="Team charter" autofocus data-c-title />
    </div>
    <div class="two">
      <div class="field">
        <label for="c-scope">Scope</label>
        <select id="c-scope" v-model="scope" class="select" data-c-scope>
          <option v-for="s in SCOPE_ORDER" :key="s" :value="s">{{ SCOPE_LABELS[s] }}</option>
        </select>
      </div>
      <div class="field">
        <label for="c-applies">Delivery</label>
        <select id="c-applies" v-model="applies" class="select">
          <option value="always">Always (mandatory policies and rules at session start)</option>
          <option value="on-demand">On demand (Claude reads it when relevant)</option>
        </select>
      </div>
    </div>
    <div v-if="scope === 'stack'" class="field">
      <label for="c-stacks">Stacks <span class="faint">(comma-separated)</span></label>
      <input id="c-stacks" v-model="stacks" class="input" :placeholder="allStacks.join(', ') || 'node, godot'" />
    </div>
    <div v-if="scope === 'project'" class="field">
      <label for="c-project">Project</label>
      <select id="c-project" v-model="projectId" class="select" data-c-project>
        <option value="" disabled>Choose…</option>
        <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>
    <div v-if="scope === 'task'" class="field">
      <label for="c-task">Task or workflow</label>
      <select id="c-task" v-model="taskId" class="select">
        <option value="" disabled>Choose…</option>
        <option v-for="t in tasks" :key="t.id ?? t.path" :value="t.id">{{ t.kind === 'workflow' ? 'Workflow' : 'Task' }}: {{ t.title }}</option>
      </select>
    </div>
    <div class="two">
      <div class="field">
        <label for="c-prefix">Rule id prefix <span class="faint">(optional)</span></label>
        <input id="c-prefix" v-model="prefix" class="input mono" placeholder="from the title, e.g. TC" maxlength="12" />
      </div>
      <label class="checkbox starter"><input v-model="starter" type="checkbox" /><span>Start with example articles</span></label>
    </div>
    <p class="hint">{{ scope === 'global' ? 'Global constitutions live on this machine and apply to every workspace.' : 'Workspace files can be synced with Git; changes made elsewhere need your approval before Claude gets them.' }} Organisation-managed Claude settings always take precedence.</p>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <button class="btn btn-primary" :disabled="!ready || busy" data-c-create @click="create">{{ busy ? 'Creating…' : 'Create' }}</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
  align-items: end;
}
.starter {
  margin-bottom: 14px;
}
</style>
