<script setup lang="ts">
import { computed, ref } from 'vue'
import { Lock } from '@lucide/vue'
import { STATUS_LABELS, TASK_STATUSES, boardColumns } from '../lib/knowledge'
import type { KnowledgeItem, TaskStatus } from '../lib/types'

const props = defineProps<{ tasks: KnowledgeItem[]; projectNames?: Record<string, string> }>()
const emit = defineEmits<{ move: [task: KnowledgeItem, status: TaskStatus]; open: [task: KnowledgeItem] }>()

const columns = computed(() => boardColumns(props.tasks))
const dragging = ref<KnowledgeItem | null>(null)
const over = ref<TaskStatus | null>(null)

function drop(status: TaskStatus) {
  const t = dragging.value
  dragging.value = null
  over.value = null
  if (t && t.status !== status) emit('move', t, status)
}

function onSelect(task: KnowledgeItem, e: Event) {
  const status = (e.target as HTMLSelectElement).value as TaskStatus
  if (status !== task.status) emit('move', task, status)
}
</script>

<template>
  <div class="board" role="list" aria-label="Task board">
    <section
      v-for="s in TASK_STATUSES"
      :key="s"
      class="column"
      :class="{ over: over === s }"
      :data-status="s"
      role="listitem"
      @dragover.prevent="over = s"
      @dragleave="over = over === s ? null : over"
      @drop.prevent="drop(s)"
    >
      <header>
        <span>{{ STATUS_LABELS[s] }}</span>
        <span class="faint">{{ columns[s].length }}</span>
      </header>
      <article
        v-for="t in columns[s]"
        :key="t.path"
        class="task"
        draggable="true"
        :data-task="t.id"
        @dragstart="dragging = t"
        @dragend="dragging = null"
      >
        <button class="title" @click="emit('open', t)">{{ t.title }}</button>
        <div class="meta">
          <span v-if="t.priority && t.priority !== 'medium'" class="badge" :class="{ 'badge-error': t.priority === 'urgent', 'badge-warn': t.priority === 'high' }">{{ t.priority }}</span>
          <span v-if="t.projectId && projectNames?.[t.projectId]" class="chip">{{ projectNames[t.projectId] }}</span>
          <Lock v-if="t.privacy === 'local-only'" class="ic" aria-label="Local only" />
          <span class="spacer" />
          <select class="select status" :value="t.status ?? 'backlog'" :aria-label="`Status of ${t.title}`" @change="onSelect(t, $event)">
            <option v-for="o in TASK_STATUSES" :key="o" :value="o">{{ STATUS_LABELS[o] }}</option>
          </select>
        </div>
      </article>
    </section>
  </div>
</template>

<style scoped>
.board {
  display: grid;
  grid-template-columns: repeat(6, minmax(180px, 1fr));
  gap: 10px;
  overflow-x: auto;
  padding-bottom: 6px;
}
.column {
  background: var(--bg-raised);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 8px;
  min-height: 160px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.column.over {
  border-color: var(--accent-ring);
  background: var(--accent-soft);
}
header {
  display: flex;
  justify-content: space-between;
  font-weight: 650;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-muted);
  padding: 2px 4px;
}
.task {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 10px;
  cursor: grab;
}
.title {
  display: block;
  width: 100%;
  text-align: left;
  border: none;
  background: none;
  color: var(--text);
  font-weight: 550;
  padding: 0;
  cursor: pointer;
}
.title:hover {
  text-decoration: underline;
}
.meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
}
.status {
  height: 26px;
  font-size: 12px;
  padding: 0 6px;
  width: auto;
}
.ic {
  width: 14px;
  height: 14px;
  color: var(--text-muted);
}
</style>
