<script setup lang="ts">
// Development History (Portable Workspace P6, ADR-0038): a project's development records — verified Git and gate
// facts, observed hook facts and Claude's own (unverified) reports kept apart — with their sync state, plus sessions,
// tasks, decisions, recent commits, gate results and blockers. Read from the stores; nothing is generated here.
import { computed, onMounted, ref, watch } from 'vue'
import { ArrowLeft, CircleAlert, FileText, GitCommitHorizontal, History, ListTodo, ShieldCheck } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { DevHistory, KnowledgeDoc } from '../lib/types'
import { relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const props = defineProps<{ id: string }>()
const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const project = computed(() => projects.byId[props.id] ?? null)
const h = ref<DevHistory | null>(null)
const open = ref<KnowledgeDoc | null>(null)
const busy = ref(false)

const SYNC: Record<string, { label: string; tone: string }> = {
  local: { label: 'not committed', tone: 'badge-warn' },
  committed: { label: 'committed, not pushed', tone: '' },
  synced: { label: 'synced', tone: 'badge-ok' },
  'no-repository': { label: 'workspace not in Git', tone: '' },
}

async function load() {
  const ws = workspaces.active
  if (!ws) return
  busy.value = true
  try {
    h.value = await api.projectHistory(ws.id, props.id)
  } catch (e) {
    app.toast('error', `Could not load the history: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
onMounted(load)
watch(() => props.id, load)

async function show(id: string | null) {
  const ws = workspaces.active
  if (!ws || !id) return
  open.value = open.value?.id === id ? null : await api.knowledgeRead(ws.id, id)
}

/** A record body's sections, so Verified / Observed / Reported read as separate panels. */
function sections(body: string): { title: string; text: string }[] {
  return body
    .split(/^## /m)
    .slice(1)
    .map((chunk) => {
      const nl = chunk.indexOf('\n')
      return { title: chunk.slice(0, nl).trim(), text: chunk.slice(nl + 1).trim() }
    })
}
const openTasks = computed(() => (h.value?.tasks ?? []).filter((t) => t[2] !== 'done'))
</script>

<template>
  <div class="page" data-history-view>
    <div class="page-header">
      <div>
        <RouterLink :to="{ name: 'project', params: { id } }" class="back"><ArrowLeft /> {{ project?.name ?? 'Project' }}</RouterLink>
        <h1>Development history</h1>
        <p>What happened in this project, session by session. <strong>Verified</strong> comes from Git and the checks NEXUS ran; <strong>observed</strong> from Claude Code's hooks; <strong>reported</strong> is Claude's own account and is not verified.</p>
      </div>
      <div class="page-actions"><button class="btn btn-ghost" :disabled="busy" @click="load">Refresh</button></div>
    </div>

    <template v-if="h">
      <section class="card" data-history-records>
        <h2 class="card-title"><History /> Development records <span class="count">{{ h.records.length }}</span></h2>
        <p v-if="!h.records.length" class="faint">No records yet — a record is written when a Claude session in this project ends after changing files, committing, running checks or recording checkpoints.</p>
        <div v-for="r in h.records" :key="r.path" class="record" :data-record="r.id">
          <button class="record-head" @click="show(r.id)">
            <FileText class="ic" />
            <span class="title">{{ r.title }}</span>
            <span v-if="r.branch" class="chip mono">{{ r.branch }}</span>
            <span class="badge" :class="SYNC[r.sync]?.tone" :data-record-sync="r.sync">{{ SYNC[r.sync]?.label ?? r.sync }}</span>
          </button>
          <div v-if="open && open.id === r.id" class="record-body" data-record-body>
            <div v-for="s in sections(open.body)" :key="s.title" class="section" :class="{ verified: s.title.startsWith('Verified'), reported: s.title.startsWith('Reported') }">
              <h3>{{ s.title }}</h3>
              <pre class="selectable">{{ s.text }}</pre>
            </div>
          </div>
        </div>
      </section>

      <div class="grid grid-2">
        <section class="card" data-history-tasks>
          <h2 class="card-title"><ListTodo /> Open tasks <span class="count">{{ openTasks.length }}</span></h2>
          <ul class="plain"><li v-for="t in openTasks" :key="t[0]"><span class="badge">{{ t[2] }}</span> {{ t[1] }}</li></ul>
          <h2 class="card-title sub">Decisions</h2>
          <ul class="plain"><li v-for="d in h.decisions" :key="d[0]">{{ d[0] }} <span class="faint">{{ d[1] }}</span></li></ul>
          <p v-if="!h.decisions.length" class="faint">None recorded.</p>
        </section>
        <section class="card" data-history-git>
          <h2 class="card-title"><GitCommitHorizontal /> Recent commits</h2>
          <ul class="plain"><li v-for="c in h.commits.slice(0, 10)" :key="c[0]"><code>{{ c[0].slice(0, 10) }}</code> {{ c[1] }} <span class="faint">{{ relativeTime(c[2]) }}</span></li></ul>
          <h2 class="card-title sub"><ShieldCheck /> Checks NEXUS ran</h2>
          <ul class="plain"><li v-for="(g, i) in h.gates" :key="i"><span class="badge" :class="g[1] === 'passed' ? 'badge-ok' : 'badge-error'">{{ g[1] }}</span> {{ g[0] }} <span class="faint">{{ relativeTime(g[2]) }}</span></li></ul>
          <p v-if="!h.gates.length" class="faint">No quality gates have run.</p>
        </section>
      </div>

      <section v-if="h.blockers.length" class="card" data-history-blockers>
        <h2 class="card-title"><CircleAlert /> Blockers reported</h2>
        <ul class="plain"><li v-for="(b, i) in h.blockers" :key="i">{{ b[0] }} <span class="faint">{{ relativeTime(b[1]) }}</span></li></ul>
      </section>

      <section class="card" data-history-sessions>
        <h2 class="card-title">Sessions</h2>
        <ul class="plain">
          <li v-for="s in h.sessions" :key="s.id">{{ s.title }} <span class="faint">{{ relativeTime(s.startedAt) }} · {{ s.state }}</span> <span v-if="s.record" class="badge badge-ok">record</span></li>
        </ul>
      </section>
    </template>
  </div>
</template>

<style scoped>
.back {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12.5px;
  color: var(--text-muted);
  text-decoration: none;
}
.back svg {
  width: 14px;
  height: 14px;
}
.count {
  color: var(--text-faint);
  font-weight: 400;
  margin-left: 4px;
}
.record {
  border-top: 1px solid var(--border);
}
.record-head {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 0;
  background: none;
  border: 0;
  color: var(--text);
  text-align: left;
  cursor: pointer;
  font: inherit;
}
.record-head .title {
  flex: 1;
}
.ic {
  width: 15px;
  height: 15px;
  color: var(--text-faint);
}
.record-body {
  display: grid;
  gap: 8px;
  padding: 4px 0 12px;
}
.section {
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.section.verified {
  border-left: 3px solid var(--green);
}
.section.reported {
  border-left: 3px solid var(--amber);
}
.section h3 {
  margin: 0 0 4px;
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-muted);
}
.section pre {
  margin: 0;
  white-space: pre-wrap;
  font-family: var(--font-ui);
  font-size: 12.5px;
  line-height: 1.55;
}
.plain {
  list-style: none;
  padding: 0;
  margin: 6px 0;
  display: grid;
  gap: 4px;
  font-size: 13px;
}
.sub {
  margin-top: 12px;
}
</style>
