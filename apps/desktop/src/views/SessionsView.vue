<script setup lang="ts">
import { computed, onActivated, ref } from 'vue'
import { Info, SquareTerminal } from '@lucide/vue'
import { useSessionStore } from '../stores/sessions'
import { useWorkspaceStore } from '../stores/workspaces'
import SessionRow from '../components/SessionRow.vue'

const sessions = useSessionStore()
const workspaces = useWorkspaceStore()
type FilterKey = 'all' | 'running' | 'resumable' | 'ended'
const filter = ref<FilterKey>('all')
const allWorkspaces = ref(false)

const scoped = computed(() =>
  sessions.list.filter((s) => !s.archivedAt && (allWorkspaces.value || s.workspaceId === workspaces.active?.id)),
)
const counts = computed(() => ({
  all: scoped.value.length,
  running: scoped.value.filter((s) => s.running).length,
  resumable: scoped.value.filter((s) => s.resumable).length,
  ended: scoped.value.filter((s) => !s.running).length,
}))
const visible = computed(() =>
  scoped.value.filter((s) =>
    filter.value === 'running' ? s.running : filter.value === 'resumable' ? s.resumable : filter.value === 'ended' ? !s.running : true,
  ),
)
const labels: Record<FilterKey, string> = { all: 'All', running: 'Running', resumable: 'Resumable', ended: 'Ended' }

onActivated(() => sessions.load().catch(() => undefined))
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Sessions</h1>
        <p>Terminals NEXUS started. A Claude session's conversation can be resumed after its process ends.</p>
      </div>
      <label class="checkbox scope">
        <input v-model="allWorkspaces" type="checkbox" />
        <span>All workspaces</span>
      </label>
    </div>

    <div class="filters" role="tablist">
      <button
        v-for="k in (Object.keys(labels) as FilterKey[])"
        :key="k"
        class="filter"
        role="tab"
        :aria-selected="filter === k"
        :class="{ active: filter === k }"
        @click="filter = k"
      >
        {{ labels[k] }} <span class="faint">{{ counts[k] }}</span>
      </button>
    </div>

    <div v-if="visible.length" class="list">
      <SessionRow v-for="s in visible" :key="s.id" :session="s" />
    </div>
    <div v-else class="empty">
      <SquareTerminal class="empty-icon" />
      <h3>No sessions here</h3>
      <p>Launch Claude or open a shell from a project. Running sessions, exit codes and resumable conversations appear here.</p>
    </div>

    <p class="notice explain"><Info /> Live terminals end when NEXUS closes; their sessions then show as <em>interrupted</em>. Claude keeps the conversation, so <strong>Resume</strong> continues it with the same profile and directory.</p>
  </div>
</template>

<style scoped>
.filters {
  display: flex;
  gap: 6px;
  margin-bottom: 14px;
}
.filter {
  height: 30px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-weight: 550;
}
.filter.active {
  background: var(--accent-soft);
  border-color: var(--accent-ring);
  color: var(--text);
}
.scope {
  align-items: center;
  color: var(--text-muted);
}
.explain {
  margin-top: 20px;
}
</style>
