<script setup lang="ts">
// The project page's Sessions tab (ADR-0048): every session of the project, the ones running and waiting for the
// person first; filters for running, ended and archived; Launch and Shell at the top.
import { computed, ref, watch } from 'vue'
import { Sparkles, SquareTerminal } from '@lucide/vue'
import SessionRow from './SessionRow.vue'
import { api } from '../lib/ipc'
import { needsUser } from '../lib/format'
import type { SessionView } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'

const props = defineProps<{ projectId: string; available: boolean }>()
const app = useAppStore()
const sessions = useSessionStore()

type SessionFilter = 'all' | 'running' | 'ended' | 'archived'
const filter = ref<SessionFilter>('all')
const archived = ref<SessionView[]>([])
const loadingArchived = ref(false)

/** Live: the store's list (updated by session events); archived ones are read on demand. */
const mine = computed(() => sessions.list.filter((s) => s.projectId === props.projectId && !s.archivedAt))
const live = (s: SessionView) => s.running || s.liveElsewhere
/** Waiting for the person, then running, then the rest by start time (newest first). */
function rank(s: SessionView) {
  if (live(s) && s.kind === 'claude' && needsUser(s.status)) return 0
  return live(s) ? 1 : 2
}
const shown = computed(() => {
  const list = filter.value === 'archived' ? archived.value : mine.value.filter((s) => (filter.value === 'running' ? live(s) : filter.value === 'ended' ? !live(s) : true))
  return [...list].sort((a, b) => rank(a) - rank(b) || b.startedAt - a.startedAt)
})
const counts = computed(() => ({ all: mine.value.length, running: mine.value.filter(live).length, ended: mine.value.filter((s) => !live(s)).length }))

watch(
  () => [filter.value, props.projectId],
  async () => {
    if (filter.value !== 'archived') return
    loadingArchived.value = true
    try {
      archived.value = (await api.sessionList({ projectId: props.projectId, includeArchived: true, limit: 300 })).filter((s) => s.archivedAt)
    } catch {
      archived.value = []
    } finally {
      loadingArchived.value = false
    }
  },
)

async function openShell() {
  await app.attempt(() => sessions.openShell({ projectId: props.projectId, shellId: app.prefs.defaultShell }), 'Could not open a shell')
}
</script>

<template>
  <div class="project-sessions" data-project-sessions>
    <div class="toolbar">
      <div class="seg" role="radiogroup" aria-label="Show sessions">
        <button v-for="f in (['all', 'running', 'ended', 'archived'] as const)" :key="f" class="seg-btn" role="radio" :aria-checked="filter === f" :class="{ active: filter === f }" :data-sessions-filter="f" @click="filter = f">
          {{ f === 'all' ? 'All' : f === 'running' ? 'Running' : f === 'ended' ? 'Ended' : 'Archived' }}<span v-if="f !== 'archived'" class="count">{{ counts[f] }}</span>
        </button>
      </div>
      <span class="spacer" />
      <button class="btn" :disabled="!available" @click="openShell"><SquareTerminal /> Shell</button>
      <button class="btn btn-primary" :disabled="!available" @click="app.openLaunch(projectId)"><Sparkles /> Launch</button>
    </div>
    <div v-if="shown.length" class="list">
      <SessionRow v-for="s in shown" :key="s.id" :session="s" :show-project="false" :data-session="s.id" />
    </div>
    <div v-else class="empty">
      <p v-if="loadingArchived">Reading archived sessions…</p>
      <p v-else-if="filter === 'archived'">No archived sessions.</p>
      <p v-else-if="filter === 'running'">Nothing runs for this project right now.</p>
      <p v-else>No sessions yet. Launch one here — Claude Code or Codex — and NEXUS records it so you can resume later.</p>
    </div>
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
.seg {
  display: inline-flex;
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}
.seg-btn {
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-muted);
  font: inherit;
  font-size: 12.5px;
  padding: 5px 10px;
  cursor: pointer;
  display: inline-flex;
  gap: 6px;
  align-items: center;
}
.seg-btn:last-child {
  border-right: none;
}
.seg-btn .count {
  color: var(--text-faint);
  font-weight: 500;
}
.seg-btn.active {
  background: var(--bg-active);
  color: var(--text);
  font-weight: 600;
}
</style>
