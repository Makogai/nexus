<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { TriangleAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { KIND_LABELS } from '../lib/knowledge'
import type { KnowledgeEntry, KnowledgeKind, Project } from '../lib/types'
import BaseModal from './BaseModal.vue'

const props = defineProps<{ open: boolean; workspaceId: string; projects: Project[]; initialKind?: KnowledgeKind }>()
const emit = defineEmits<{ close: []; created: [item: KnowledgeEntry] }>()

const kind = ref<KnowledgeKind>('task')
const title = ref('')
const body = ref('')
const projectId = ref<string>('')
const where = ref<'shared' | 'local' | 'global'>('shared')
const priority = ref('medium')
const applies = ref<'always' | 'on-demand'>('on-demand')
const stacks = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

watch(
  () => props.open,
  (o) => {
    if (!o) return
    kind.value = props.initialKind ?? 'task'
    title.value = ''
    body.value = ''
    projectId.value = ''
    where.value = 'shared'
    priority.value = 'medium'
    applies.value = 'on-demand'
    stacks.value = ''
    error.value = null
  },
)

const isRule = computed(() => kind.value === 'rule')
const canGlobal = computed(() => kind.value === 'rule' || kind.value === 'workflow' || kind.value === 'note')
const scopeHint = computed(() => {
  if (where.value === 'global') return 'Global: every workspace on this machine (e.g. a constitution). Stored in the NEXUS data folder.'
  if (where.value === 'local') return 'Local only: stored in .nexus-local/ (git-ignored), never synced.'
  return 'Shared: a Markdown file in the workspace folder — synced with it (e.g. through Git).'
})

watch(canGlobal, (ok) => {
  if (!ok && where.value === 'global') where.value = 'shared'
})

async function create() {
  busy.value = true
  error.value = null
  try {
    const item = await api.knowledgeCreate(props.workspaceId, {
      kind: kind.value,
      title: title.value.trim(),
      body: body.value,
      projectId: where.value === 'global' ? null : projectId.value || null,
      localOnly: where.value === 'local',
      global: where.value === 'global',
      priority: kind.value === 'task' ? priority.value : null,
      applies: isRule.value ? applies.value : null,
      stacks: isRule.value
        ? stacks.value
            .split(',')
            .map((s) => s.trim())
            .filter(Boolean)
        : [],
    })
    emit('created', item)
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="open" title="New knowledge document" subtitle="Saved as a Markdown file with YAML frontmatter; the file is the source of truth." :width="620" @close="emit('close')">
    <div class="grid2">
      <div class="field">
        <label class="label" for="k-kind">Kind</label>
        <select id="k-kind" v-model="kind" class="select">
          <option v-for="(label, k) in KIND_LABELS" :key="k" :value="k">{{ label.replace(/s$/, '') }}</option>
        </select>
      </div>
      <div class="field">
        <label class="label" for="k-where">Where</label>
        <select id="k-where" v-model="where" class="select">
          <option value="shared">Workspace (shared)</option>
          <option value="local">This machine only</option>
          <option v-if="canGlobal" value="global">Global (all workspaces)</option>
        </select>
      </div>
    </div>
    <p class="hint">{{ scopeHint }}</p>
    <div class="field">
      <label class="label" for="k-title">Title</label>
      <input id="k-title" v-model="title" class="input" maxlength="200" autofocus />
    </div>
    <div class="grid2">
      <div v-if="where !== 'global'" class="field">
        <label class="label" for="k-project">Project</label>
        <select id="k-project" v-model="projectId" class="select">
          <option value="">Whole workspace</option>
          <option v-for="p in projects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </div>
      <div v-if="kind === 'task'" class="field">
        <label class="label" for="k-priority">Priority</label>
        <select id="k-priority" v-model="priority" class="select">
          <option v-for="p in ['low', 'medium', 'high', 'urgent']" :key="p" :value="p">{{ p }}</option>
        </select>
      </div>
      <template v-if="isRule">
        <div class="field">
          <label class="label" for="k-applies">Delivery</label>
          <select id="k-applies" v-model="applies" class="select">
            <option value="on-demand">On demand — Claude looks it up when relevant</option>
            <option value="always">Always — in every session's system prompt</option>
          </select>
        </div>
        <div v-if="where !== 'global'" class="field">
          <label class="label" for="k-stacks">Stacks (optional)</label>
          <input id="k-stacks" v-model="stacks" class="input" placeholder="rust, vue" />
        </div>
      </template>
    </div>
    <div class="field">
      <label class="label" for="k-body">Content (Markdown)</label>
      <textarea id="k-body" v-model="body" class="textarea" rows="8" />
    </div>
    <p v-if="isRule && applies === 'always'" class="hint">Always-on rules share a 1,500-character budget per session; keep them short.</p>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <button class="btn btn-primary" :disabled="!title.trim() || busy" @click="create">Create</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.grid2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 14px;
}
.hint {
  margin: -4px 0 12px;
}
</style>
