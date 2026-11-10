<script setup lang="ts">
// The project page's Tasks tab (ADR-0048): the project's tasks as a list (ready for review first, Done / Reopen,
// Launch / Continue) or as the board, and a new task for this project. Done and Reopen update the list at once and
// roll back when the file cannot be written.
import { computed, ref, watch } from 'vue'
import { Check, Columns3, LayoutList, Plus, RotateCcw, Sparkles } from '@lucide/vue'
import TaskBoard from './TaskBoard.vue'
import KnowledgeDialog from './KnowledgeDialog.vue'
import KnowledgeDocDialog from './KnowledgeDocDialog.vue'
import { api, toIpcError } from '../lib/ipc'
import { STATUS_LABELS } from '../lib/knowledge'
import type { KnowledgeEntry, KnowledgeItem, ProjectKnowledge, TaskStatus } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const props = defineProps<{ projectId: string; available: boolean; knowledge: ProjectKnowledge | null }>()
const emit = defineEmits<{ changed: [] }>()
const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const wsId = computed(() => workspaces.active?.id ?? null)

/** Local copies, so Done / Reopen show at once (and roll back on an error). */
const open = ref<KnowledgeItem[]>([])
const done = ref<KnowledgeItem[]>([])
watch(
  () => props.knowledge,
  (k) => {
    open.value = [...(k?.tasks ?? [])]
    done.value = [...(k?.doneTasks ?? [])]
  },
  { immediate: true },
)
const inReview = computed(() => open.value.filter((t) => t.status === 'review').length)

const view = ref<'list' | 'board'>('list')
const board = ref<KnowledgeItem[]>([])
async function loadBoard() {
  if (!wsId.value) return
  const o = await api.knowledgeOverview(wsId.value, false).catch(() => null)
  if (o) board.value = o.items.filter((i) => i.kind === 'task' && i.valid && !i.conflict && i.projectId === props.projectId)
}
watch(view, (v) => v === 'board' && loadBoard())
watch(
  () => props.knowledge,
  () => view.value === 'board' && loadBoard(),
)

const busy = ref<string | null>(null)
async function setStatus(t: KnowledgeItem, status: 'done' | 'ready') {
  const ws = wsId.value
  if (!ws || !t.id || busy.value) return
  busy.value = t.id
  const before = { open: open.value, done: done.value }
  // Optimistic: move the row now.
  if (status === 'done') {
    open.value = open.value.filter((x) => x.id !== t.id)
    done.value = [{ ...t, status: 'done' }, ...done.value]
  } else {
    done.value = done.value.filter((x) => x.id !== t.id)
    open.value = [...open.value, { ...t, status: 'ready' }]
  }
  try {
    await api.knowledgeUpdateTask(ws, t.id, { status, log: status === 'done' ? 'Marked done' : 'Reopened' })
    app.toast('success', status === 'done' ? `Done: ${t.title}` : `Reopened: ${t.title}`)
    emit('changed')
  } catch (e) {
    open.value = before.open
    done.value = before.done
    app.toast('error', `Could not update “${t.title}”: ${toIpcError(e).message}`)
  } finally {
    busy.value = null
  }
}

async function move(t: KnowledgeItem, status: TaskStatus) {
  if (!wsId.value || !t.id) return
  try {
    await api.knowledgeUpdateTask(wsId.value, t.id, { status, expectedHash: t.hash })
  } catch (e) {
    const err = toIpcError(e)
    app.toast('error', err.code === 'conflict' ? `“${t.title}” changed on disk — reloaded, try again` : `Could not move the task: ${err.message}`)
  }
  emit('changed')
  await loadBoard()
}

function launch(t: KnowledgeItem) {
  if (t.id) app.openLaunch(props.projectId, null, { id: t.id, title: t.title, projectId: t.projectId })
}

const creating = ref(false)
const viewing = ref<KnowledgeItem | null>(null)
/** A task created here opens at once (its file is the place to add detail). */
async function created(item: KnowledgeEntry) {
  emit('changed')
  if (!wsId.value || !item.id) return
  const o = await api.knowledgeOverview(wsId.value, false).catch(() => null)
  viewing.value = o?.items.find((i) => i.id === item.id) ?? null
}
</script>

<template>
  <div class="project-tasks">
    <div class="toolbar">
      <span class="faint" data-project-task-counts>{{ open.length }} open<template v-if="inReview"> · <strong class="review-count">{{ inReview }} ready for review</strong></template></span>
      <span class="spacer" />
      <div class="seg" role="radiogroup" aria-label="Task view">
        <button class="seg-btn" role="radio" :aria-checked="view === 'list'" :class="{ active: view === 'list' }" data-tasks-view="list" @click="view = 'list'"><LayoutList /> List</button>
        <button class="seg-btn" role="radio" :aria-checked="view === 'board'" :class="{ active: view === 'board' }" data-tasks-view="board" @click="view = 'board'"><Columns3 /> Board</button>
      </div>
      <button class="btn btn-primary" :disabled="!wsId || !workspaces.active?.compat?.writable" data-project-new-task @click="creating = true"><Plus /> New task</button>
    </div>

    <TaskBoard v-if="view === 'board'" :tasks="board" @move="move" @open="viewing = $event" @launch="launch" />
    <template v-else>
      <div v-if="open.length" class="list" data-project-tasks>
        <div v-for="t in open" :key="t.path" class="list-row task-row" :class="{ 'for-review': t.status === 'review' }" :data-task="t.id">
          <span class="badge" :class="{ 'badge-accent': t.status === 'review' }">{{ t.status === 'review' ? 'Ready for review' : (STATUS_LABELS[t.status as TaskStatus] ?? t.status) }}</span>
          <button class="task-title link-like" :title="`Open “${t.title}”`" @click="viewing = t">{{ t.title }}</button>
          <span v-if="t.priority && t.priority !== 'medium'" class="badge" :class="{ 'badge-warn': t.priority === 'high', 'badge-error': t.priority === 'urgent' }">{{ t.priority }}</span>
          <span v-if="t.branch" class="chip mono">{{ t.branch }}</span>
          <button class="btn btn-sm" :class="t.status === 'review' ? 'btn-primary' : 'btn-ghost'" title="Mark this task done" :disabled="busy === t.id" :aria-busy="busy === t.id" data-task-done @click="setStatus(t, 'done')"><Check /> Done</button>
          <button class="btn btn-sm" :class="t.status === 'review' ? 'btn-ghost' : 'btn-primary'" :disabled="!available" @click="launch(t)"><Sparkles /> {{ t.status === 'review' ? 'Continue' : 'Launch' }}</button>
        </div>
      </div>
      <div v-else class="empty">
        <p>No open tasks for this project. Create one here, or let Claude create them with the NEXUS tools.</p>
      </div>
      <details v-if="done.length" class="done-tasks" data-project-done>
        <summary>Recently done <span class="count">{{ done.length }}</span></summary>
        <div class="list">
          <div v-for="t in done" :key="t.path" class="list-row task-row" :data-done-task="t.id">
            <span class="badge badge-ok">Done</span>
            <span class="task-title">{{ t.title }}</span>
            <span v-if="t.branch" class="chip mono">{{ t.branch }}</span>
            <button class="btn btn-sm btn-ghost" title="Reopen this task" :disabled="busy === t.id" :aria-busy="busy === t.id" data-task-reopen @click="setStatus(t, 'ready')"><RotateCcw /> Reopen</button>
          </div>
        </div>
      </details>
    </template>

    <KnowledgeDialog v-if="wsId" :open="creating" :workspace-id="wsId" :projects="projects.list" initial-kind="task" :initial-project="projectId" @close="creating = false" @created="created" />
    <KnowledgeDocDialog v-if="wsId" :item="viewing" :workspace-id="wsId" @close="viewing = null" @changed="emit('changed')" @approved="emit('changed')" @launch="(t) => { viewing = null; launch(t) }" />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.spacer {
  flex: 1;
}
.review-count {
  color: var(--accent-strong);
}
.seg {
  display: inline-flex;
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}
.seg-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-muted);
  font: inherit;
  font-size: 12.5px;
  padding: 5px 10px;
  cursor: pointer;
}
.seg-btn svg {
  width: 14px;
  height: 14px;
}
.seg-btn:last-child {
  border-right: none;
}
.seg-btn.active {
  background: var(--bg-active);
  color: var(--text);
  font-weight: 600;
}
.task-row .btn {
  margin-left: auto;
}
.task-row .btn + .btn {
  margin-left: 6px;
}
.task-row.for-review {
  border-left: 3px solid var(--accent);
}
.task-title {
  font-weight: 550;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.link-like {
  background: none;
  border: none;
  padding: 0;
  color: inherit;
  font: inherit;
  font-weight: 550;
  cursor: pointer;
  text-align: left;
}
.link-like:hover {
  text-decoration: underline;
}
.done-tasks {
  margin-top: 10px;
}
.done-tasks summary {
  cursor: pointer;
  color: var(--text-muted);
  font-size: 13px;
  margin-bottom: 8px;
}
.done-tasks .task-title {
  color: var(--text-muted);
}
.count {
  color: var(--text-faint);
}
</style>
