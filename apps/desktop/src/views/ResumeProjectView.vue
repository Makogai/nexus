<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowLeft, GitBranch, History, Lightbulb, RefreshCw, RotateCcw, ShieldCheck, Sparkles, TriangleAlert } from '@lucide/vue'
import { relativeTime } from '../lib/format'
import { api, toIpcError } from '../lib/ipc'
import { STATUS_LABELS } from '../lib/knowledge'
import type { ResumeSummary, TaskStatus } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'

// Resume project (ADR-0021): what Git says now (verified) is kept apart from what was saved earlier (history).
const props = defineProps<{ id: string }>()
const app = useAppStore()
const projects = useProjectStore()
const sessions = useSessionStore()
const router = useRouter()

const summary = ref<ResumeSummary | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)
const busy = ref(false)

async function load() {
  loading.value = true
  try {
    summary.value = await api.projectResume(props.id)
    error.value = null
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    loading.value = false
  }
}
watch(() => props.id, load, { immediate: true })
onActivated(load)
watch(() => app.externalChanges, load)

const v = computed(() => summary.value?.verified ?? null)
const next = computed(() => summary.value?.next ?? null)
const nextButton = computed(() => {
  switch (next.value?.kind) {
    case 'resume-session':
      return 'Resume session'
    case 'launch-task':
      return 'Launch for this task'
    default:
      return 'Launch'
  }
})

async function runNext() {
  const n = next.value
  if (!n || !summary.value) return
  if (n.kind === 'resume-session' && n.sessionId) {
    busy.value = true
    await app.attempt(() => sessions.resume(n.sessionId!), 'Could not resume the session')
    busy.value = false
    return
  }
  if (n.kind === 'launch-task' && n.taskId) {
    const t = summary.value.saved.tasks.find((x) => x.id === n.taskId)
    app.openLaunch(props.id, null, { id: n.taskId, title: t?.title ?? n.label, projectId: props.id })
    return
  }
  app.openLaunch(props.id)
}

function launchTask(id: string, title: string) {
  app.openLaunch(props.id, null, { id, title, projectId: props.id })
}

async function resume(sessionId: string) {
  await app.attempt(() => sessions.resume(sessionId), 'Could not resume the session')
}

const source = (e: string) => (e.startsWith('verified:') ? 'verified' : 'saved')
const fact = (e: string) => e.replace(/^(verified|saved): /, '')
</script>

<template>
  <div class="page">
    <button class="btn btn-ghost btn-sm back" @click="router.push(`/projects/${id}`)"><ArrowLeft /> {{ projects.byId[id]?.name ?? 'Project' }}</button>
    <div class="page-header">
      <div>
        <h1>Resume {{ summary?.projectName ?? projects.byId[id]?.name }}</h1>
        <p>Rebuilt from Git and your saved tasks, notes and checkpoints. Nothing here is guessed by a model.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="loading" title="Read Git and the saved context again" @click="load"><RefreshCw :class="{ spin: loading }" /> Refresh</button>
      </div>
    </div>

    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>

    <template v-if="summary && next">
      <section class="card next" data-next :data-kind="next.kind">
        <div class="next-head">
          <div>
            <div class="label">Recommended next step</div>
            <h2>{{ next.label }}</h2>
          </div>
          <button class="btn btn-primary" :disabled="busy" data-next-action @click="runNext">
            <RotateCcw v-if="next.kind === 'resume-session'" /><Sparkles v-else /> {{ nextButton }}
          </button>
        </div>
        <ul class="evidence">
          <li v-for="e in next.evidence" :key="e">
            <span class="badge" :class="source(e) === 'verified' ? 'badge-ok' : ''">{{ source(e) }}</span>
            <span class="selectable">{{ fact(e) }}</span>
          </li>
        </ul>
        <p v-if="next.kind === 'pull'" class="hint">NEXUS does not change your repository — pull in your terminal or Git tool.</p>
      </section>

      <div v-for="w in summary.warnings" :key="w" class="notice notice-warn"><TriangleAlert /> {{ w }}</div>
      <div v-if="summary.saved.pendingProposals" class="notice notice-accent" data-pending-proposals>
        <Lightbulb />
        <span>{{ summary.saved.pendingProposals }} memory proposal(s) from past sessions are waiting for review —
          <RouterLink to="/knowledge?tab=proposals">review them</RouterLink> (they are not knowledge until approved).</span>
      </div>

      <div class="grid grid-2 cols">
        <section class="card" data-section="verified">
          <h2 class="card-title"><ShieldCheck /> Verified now <span class="faint">Git · {{ relativeTime(v!.checkedAt) }}</span></h2>
          <p v-if="!v!.available" class="faint">The project folder is not on this machine.</p>
          <p v-else-if="!v!.repo" class="faint">Not a Git repository.</p>
          <template v-else>
            <dl class="facts">
              <dt>Branch</dt>
              <dd class="row mono"><GitBranch class="ic" /> {{ v!.branch ?? '(detached)' }}</dd>
              <template v-if="v!.head">
                <dt>HEAD</dt>
                <dd class="selectable"><code>{{ v!.head.oid.slice(0, 8) }}</code> {{ v!.head.summary }} <span class="faint">{{ relativeTime(v!.head.at) }}</span></dd>
              </template>
              <dt>Working tree</dt>
              <dd>{{ v!.staged }} staged · {{ v!.unstaged }} modified · {{ v!.untracked }} untracked<template v-if="v!.conflicted"> · <strong>{{ v!.conflicted }} conflicted</strong></template></dd>
              <template v-if="v!.upstream">
                <dt>Upstream</dt>
                <dd>{{ v!.ahead }} ahead · {{ v!.behind }} behind <span class="mono">{{ v!.upstream }}</span> <span class="faint">(as of the last fetch)</span></dd>
              </template>
            </dl>
            <div v-if="v!.recentCommits.length" class="label sub">Recent commits</div>
            <ul class="commits">
              <li v-for="c in v!.recentCommits" :key="c.oid"><code>{{ c.oid.slice(0, 8) }}</code> <span class="selectable">{{ c.summary }}</span> <span class="faint">{{ relativeTime(c.at) }}</span></li>
            </ul>
          </template>
        </section>

        <section class="card" data-section="saved">
          <h2 class="card-title"><History /> Saved context <span class="faint">history, not re-verified</span></h2>
          <div class="label sub">Open tasks</div>
          <div v-for="t in summary.saved.tasks" :key="t.id" class="saved-row" :data-task="t.id">
            <span class="badge">{{ STATUS_LABELS[t.status as TaskStatus] ?? t.status }}</span>
            <div class="grow">
              <div class="title">{{ t.title }}<span v-if="t.checklistTotal" class="faint"> · {{ t.checklistDone }}/{{ t.checklistTotal }}</span></div>
              <div v-if="t.lastProgress" class="faint small selectable">{{ t.lastProgress }}</div>
            </div>
            <button class="btn btn-sm" @click="launchTask(t.id, t.title)"><Sparkles /> Launch</button>
          </div>
          <p v-if="!summary.saved.tasks.length" class="faint">No open tasks.</p>

          <template v-if="summary.saved.notes.length || summary.saved.decisions.length">
            <div class="label sub">Notes and decisions for {{ v?.branch ?? 'this project' }}</div>
            <div v-for="n in [...summary.saved.notes, ...summary.saved.decisions]" :key="n.id" class="saved-row">
              <span class="badge">{{ n.kind }}</span>
              <span class="grow title">{{ n.title }}</span>
              <span v-if="n.relevance === 'this-branch'" class="chip mono">this branch</span>
              <span v-else-if="n.relevance === 'inherited'" class="chip mono">from {{ n.branch }}</span>
            </div>
          </template>
          <p v-if="summary.saved.hiddenOtherBranch" class="faint small">{{ summary.saved.hiddenOtherBranch }} note(s) of other branches hidden.</p>

          <template v-if="summary.saved.checkpoints.length">
            <div class="label sub">Latest checkpoints</div>
            <div v-for="c in summary.saved.checkpoints" :key="c.at + c.summary" class="saved-row">
              <span class="badge" :class="{ 'badge-ok': c.kind === 'done', 'badge-warn': c.kind === 'blocker' }">{{ c.kind }}</span>
              <div class="grow">
                <div class="selectable">{{ c.summary }}</div>
                <div class="faint small">{{ c.sessionTitle }} · {{ relativeTime(c.at) }}<template v-if="c.otherBranch"> · recorded on {{ c.branch }}</template></div>
              </div>
            </div>
          </template>

          <template v-if="summary.saved.sessions.length">
            <div class="label sub">Recent sessions</div>
            <div v-for="s in summary.saved.sessions" :key="s.id" class="saved-row" :data-session="s.id">
              <span class="badge">{{ s.state }}</span>
              <span class="grow title">{{ s.title }} <span class="faint small">{{ relativeTime(s.endedAt ?? s.startedAt) }}</span></span>
              <button v-if="s.resumable" class="btn btn-sm" @click="resume(s.id)"><RotateCcw /> Resume</button>
            </div>
          </template>
        </section>
      </div>
    </template>
    <p v-else-if="loading" class="faint">Reading Git and the saved context…</p>
  </div>
</template>

<style scoped>
.back {
  margin: -8px 0 10px -8px;
}
.next {
  margin-bottom: 14px;
  border-color: var(--accent-ring);
}
.next-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}
.next h2 {
  margin: 2px 0 0;
  font-size: 18px;
}
.evidence {
  list-style: none;
  padding: 0;
  margin: 12px 0 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 13px;
}
.evidence li {
  display: flex;
  gap: 8px;
  align-items: baseline;
}
.cols {
  margin-top: 14px;
}
.facts {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 6px 14px;
  margin: 0 0 12px;
  font-size: 13px;
}
dt {
  color: var(--text-muted);
}
dd {
  margin: 0;
}
.sub {
  margin: 14px 0 6px;
}
.commits {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
}
.saved-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 0;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
}
.grow {
  flex: 1;
  min-width: 0;
}
.title {
  font-weight: 550;
}
.small {
  font-size: 12px;
}
.ic {
  width: 14px;
  height: 14px;
}
</style>
