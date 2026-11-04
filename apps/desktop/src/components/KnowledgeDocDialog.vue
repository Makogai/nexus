<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { FolderOpen, GitBranch, ShieldCheck, Sparkles, TriangleAlert } from '@lucide/vue'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { api, toIpcError } from '../lib/ipc'
import { STATUS_LABELS } from '../lib/knowledge'
import type { KnowledgeDoc, KnowledgeItem, TaskView } from '../lib/types'
import { useSessionStore } from '../stores/sessions'
import BaseModal from './BaseModal.vue'
import SessionRow from './SessionRow.vue'

const props = defineProps<{ item: KnowledgeItem | null; workspaceId: string }>()
const emit = defineEmits<{ close: []; approved: []; changed: []; launch: [item: KnowledgeItem] }>()
const sessions = useSessionStore()

const doc = ref<KnowledgeDoc | null>(null)
const task = ref<TaskView | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

async function load(item: KnowledgeItem | null) {
  doc.value = null
  task.value = null
  error.value = null
  if (!item?.id) return
  try {
    if (item.kind === 'task') task.value = await api.knowledgeTask(props.workspaceId, item.id)
    else doc.value = await api.knowledgeRead(props.workspaceId, item.id)
  } catch (e) {
    error.value = toIpcError(e).message
  }
}
watch(() => props.item, load, { immediate: true })

const hidden = new Set(['nexus', 'id', 'title', 'kind'])
const fields = computed(() =>
  Object.entries(doc.value?.fields ?? {})
    .filter(([k]) => !hidden.has(k))
    .map(([k, v]) => [k, Array.isArray(v) ? v.join(', ') : String(v ?? '')] as const),
)
const taskSessions = computed(() => (task.value ? sessions.list.filter((s) => s.taskId === task.value!.id && !s.archivedAt) : []))
const prose = computed(() =>
  (task.value?.description ?? '')
    .split('\n')
    .filter((l) => !/^\s*[-*] \[( |x|X)\] /.test(l))
    .join('\n')
    .trim(),
)

async function toggle(index: number, done: boolean) {
  if (!task.value || !props.item?.id) return
  busy.value = true
  try {
    await api.knowledgeUpdateTask(props.workspaceId, props.item.id, {
      check: done ? [index] : [],
      uncheck: done ? [] : [index],
      expectedHash: task.value.hash,
    })
    emit('changed')
  } catch (e) {
    const err = toIpcError(e)
    error.value = err.code === 'conflict' ? 'The task file changed on disk — reloaded, try again.' : err.message
  } finally {
    busy.value = false
    await load(props.item)
  }
}

async function approve() {
  if (!props.item?.id) return
  busy.value = true
  try {
    await api.knowledgeApprove(props.workspaceId, props.item.id)
    emit('approved')
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="!!item" :title="item?.title ?? ''" :subtitle="doc?.relativePath" :width="720" @close="emit('close')">
    <div v-if="item?.needsApproval" class="notice notice-warn">
      <TriangleAlert />
      <span>This always-on rule is not given to Claude until you approve this exact version on this machine. Read it first — workspace files can change through sync.</span>
    </div>

    <template v-if="task">
      <div class="task-head">
        <span class="badge">{{ STATUS_LABELS[task.status] ?? task.status }}</span>
        <span v-if="task.priority" class="badge">{{ task.priority }}</span>
        <span v-if="task.branch" class="chip mono row"><GitBranch class="ic" /> {{ task.branch }}<template v-if="task.commit"> @ {{ task.commit.slice(0, 10) }}</template></span>
      </div>
      <pre v-if="prose" class="body selectable">{{ prose }}</pre>
      <div v-if="task.checklist.length" class="checklist" data-checklist>
        <div class="label">Checklist <span class="faint">{{ task.checklist.filter((c) => c.done).length }}/{{ task.checklist.length }}</span></div>
        <label v-for="c in task.checklist" :key="c.index" class="checkbox">
          <input type="checkbox" :checked="c.done" :disabled="busy" @change="toggle(c.index, ($event.target as HTMLInputElement).checked)" />
          <span>{{ c.text }}</span>
        </label>
      </div>
      <div v-if="task.recentLog.length" class="log">
        <div class="label">Progress <span class="faint">last {{ task.recentLog.length }} of {{ task.logEntries }}</span></div>
        <ul>
          <li v-for="(l, n) in task.recentLog" :key="n" class="selectable">{{ l }}</li>
        </ul>
      </div>
      <div v-if="taskSessions.length" class="sessions">
        <div class="label">Sessions for this task</div>
        <SessionRow v-for="s in taskSessions" :key="s.id" :session="s" />
      </div>
    </template>

    <template v-else>
      <dl v-if="fields.length" class="fields">
        <template v-for="[k, v] in fields" :key="k">
          <dt>{{ k }}</dt>
          <dd class="selectable">{{ v }}</dd>
        </template>
      </dl>
      <pre v-if="doc" class="body selectable">{{ doc.body.trim() || '(empty)' }}</pre>
      <p v-if="doc?.truncated" class="hint">Showing the first 200,000 characters.</p>
    </template>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <button v-if="item" class="btn btn-ghost" @click="revealItemInDir(item.path)"><FolderOpen /> Show file</button>
      <span class="spacer" />
      <button v-if="item?.needsApproval" class="btn btn-primary" :disabled="busy || !doc" @click="approve"><ShieldCheck /> Approve this version</button>
      <button v-if="task && task.status !== 'done' && item" class="btn btn-primary" data-launch @click="emit('launch', item)"><Sparkles /> Launch Claude</button>
      <button class="btn" @click="emit('close')">Close</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.fields {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 6px 14px;
  margin: 0 0 14px;
  font-size: 13px;
}
dt {
  color: var(--text-muted);
}
dd {
  margin: 0;
}
.body {
  white-space: pre-wrap;
  word-break: break-word;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 13px;
  line-height: 1.55;
  background: var(--bg-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 14px;
  max-height: 40vh;
  overflow: auto;
  margin: 0 0 14px;
}
.task-head {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
.checklist,
.log,
.sessions {
  margin-bottom: 14px;
}
.checklist .checkbox {
  display: flex;
  margin: 4px 0;
}
.log ul {
  margin: 4px 0 0;
  padding-left: 18px;
  font-size: 13px;
}
.ic {
  width: 13px;
  height: 13px;
}
</style>
