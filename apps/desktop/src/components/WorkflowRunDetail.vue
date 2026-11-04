<script setup lang="ts">
import { computed, ref } from 'vue'
import { Ban, CircleCheck, CircleDashed, CircleX, Eye, Loader, RotateCcw, ShieldCheck, Sparkles, TriangleAlert } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { RunStep, RunView } from '../lib/types'
import { relativeTime } from '../lib/format'
import { KIND_LABELS, STATUS_LABELS, verificationLabel } from '../lib/workflow'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useSessionStore } from '../stores/sessions'

const props = defineProps<{ run: RunView }>()
const emit = defineEmits<{ changed: [run: RunView] }>()
const app = useAppStore()
const sessions = useSessionStore()
const profiles = useProfileStore()
const notes = ref<Record<string, string>>({})
const openOutput = ref<string | null>(null)
const busy = ref(false)

const r = computed(() => props.run)
const active = computed(() => r.value.status === 'running' || r.value.status === 'waiting')
const pct = computed(() => (r.value.progress.total ? Math.round((r.value.progress.done / r.value.progress.total) * 100) : 0))
const liveSession = computed(() => sessions.list.find((s) => s.taskId === r.value.taskId && (s.running || s.liveElsewhere)) ?? null)
const lastSession = computed(() => sessions.list.find((s) => s.taskId === r.value.taskId) ?? null)

async function act(fn: () => Promise<RunView>, failure: string) {
  busy.value = true
  const v = await app.attempt(fn, failure)
  busy.value = false
  if (v) emit('changed', v)
}
const review = (s: RunStep, approve: boolean) => act(() => api.workflowReview(r.value.id, s.stepId, approve, notes.value[s.stepId] ?? ''), 'Could not record the review')
const retry = (s: RunStep) => act(() => api.workflowRetry(r.value.id, s.stepId), 'Could not retry')
async function cancel() {
  const ok = await app.confirm({ title: 'Cancel this run?', message: 'Running gates are stopped. The task and its Claude session stay; start a new run to continue later.', confirmLabel: 'Cancel run', danger: true })
  if (ok) await act(() => api.workflowCancel(r.value.id), 'Could not cancel')
}
async function openSession() {
  if (liveSession.value) {
    sessions.openTab(liveSession.value.id)
    return
  }
  if (lastSession.value?.resumable) {
    await app.attempt(() => sessions.resume(lastSession.value!.id), 'Could not resume the session')
    return
  }
  const profileId = r.value.profileId ?? app.prefs.lastLaunch?.profileId ?? profiles.list[0]?.id
  if (!profileId) return app.toast('error', 'Register a Claude profile first.')
  await app.attempt(() => sessions.startClaude({ profileId, projectId: r.value.projectId, taskId: r.value.taskId, workProfile: r.value.rigor, title: r.value.workflowTitle }), 'Could not launch Claude')
}
function icon(s: RunStep) {
  switch (s.status) {
    case 'passed':
      return CircleCheck
    case 'failed':
    case 'cancelled':
      return CircleX
    case 'verifying':
      return Loader
    case 'awaiting-review':
      return Eye
    case 'blocked':
      return TriangleAlert
    default:
      return CircleDashed
  }
}
</script>

<template>
  <section class="run" :data-run="r.id" :data-run-status="r.status">
    <header class="head">
      <div>
        <h2>{{ r.workflowTitle }}</h2>
        <p class="small faint">{{ r.goal || 'No goal text' }} · {{ r.rigor }} rigor · started {{ relativeTime(r.createdAt) }}</p>
      </div>
      <span class="badge" :class="{ 'badge-ok': r.status === 'completed', 'badge-error': r.status === 'failed', 'badge-live': r.status === 'running', 'badge-warn': r.status === 'waiting' }" data-run-badge>{{ r.status }}</span>
    </header>
    <div class="bar" :title="`${r.progress.done} of ${r.progress.total} steps done`"><div class="fill" :style="{ width: `${pct}%` }" /></div>
    <p class="small" data-progress>
      <strong>{{ r.progress.verified }}</strong> verified by gates · <strong>{{ r.progress.human }}</strong> signed off · <strong>{{ r.progress.aiReported }}</strong> AI-reported only · {{ r.progress.skipped }} not in this rigor
    </p>
    <p v-if="r.error" class="notice notice-error small"><TriangleAlert /> {{ r.error }}</p>
    <div class="actions">
      <button class="btn btn-sm btn-primary" data-open-session @click="openSession"><Sparkles /> {{ liveSession ? 'Open Claude session' : lastSession?.resumable ? 'Resume Claude session' : 'Launch Claude' }}</button>
      <button v-if="active" class="btn btn-sm btn-ghost" :disabled="busy" data-cancel-run @click="cancel"><Ban /> Cancel run</button>
    </div>

    <ol class="steps">
      <li v-for="s in r.steps" :key="s.stepId" class="step" :class="[`st-${s.status}`]" :data-step="s.stepId" :data-step-status="s.status">
        <component :is="icon(s)" class="st-icon" :class="{ spin: s.status === 'verifying' }" />
        <div class="body">
          <div class="row-head">
            <strong>{{ s.title }}</strong>
            <span class="badge">{{ KIND_LABELS[s.kind] }}</span>
            <span class="faint small">{{ STATUS_LABELS[s.status] }}</span>
            <span v-if="verificationLabel(s)" class="badge" :class="{ 'badge-ok': verificationLabel(s)!.tone === 'ok', 'badge-accent': verificationLabel(s)!.tone === 'accent', 'badge-warn': verificationLabel(s)!.tone === 'warn' }" data-verification>{{ verificationLabel(s)!.text }}</span>
            <span v-if="s.attempts" class="faint small">try {{ s.attempts }}/{{ s.maxAttempts }}</span>
          </div>
          <p v-if="s.gates.length" class="small faint">Gates: {{ s.gates.join(', ') }}<template v-if="s.needs.length"> · after {{ s.needs.join(', ') }}</template></p>
          <p v-else-if="s.needs.length" class="small faint">After {{ s.needs.join(', ') }}</p>
          <ul v-if="s.criteria.length" class="criteria small">
            <li v-for="(c, i) in s.criteria" :key="i" :class="{ met: s.criteriaMet.includes(i + 1) }">{{ s.criteriaMet.includes(i + 1) ? '☑' : '☐' }} {{ c }} <span v-if="s.criteriaMet.includes(i + 1)" class="faint">(Claude says met)</span></li>
          </ul>
          <p v-if="s.report" class="report small"><span class="faint">Claude:</span> {{ s.report }}</p>
          <pre v-if="s.lastFailure && s.status !== 'passed'" class="failure small selectable">{{ s.lastFailure }}</pre>
          <div v-for="g in s.results" :key="g.id" class="gate small" :data-gate-result="g.gateName">
            <span class="badge" :class="{ 'badge-ok': g.status === 'passed', 'badge-error': g.status === 'failed' || g.status === 'timeout' || g.status === 'error', 'badge-live': g.status === 'running' }">{{ g.gateName }}: {{ g.status }}</span>
            <code>{{ g.command }}</code>
            <span class="faint">exit {{ g.exitCode ?? '–' }} · {{ g.durationMs != null ? `${(g.durationMs / 1000).toFixed(1)} s` : '…' }} · {{ relativeTime(g.startedAt) }}</span>
            <button v-if="g.outputTail" class="btn btn-ghost btn-sm" @click="openOutput = openOutput === g.id ? null : g.id">{{ openOutput === g.id ? 'Hide output' : 'Output' }}</button>
            <pre v-if="openOutput === g.id" class="output selectable">{{ g.outputTail }}</pre>
          </div>
          <div v-if="s.status === 'awaiting-review'" class="review" data-review-step>
            <input v-model="notes[s.stepId]" class="input" placeholder="Note (optional for approval, shown to Claude on rejection)" />
            <button class="btn btn-sm btn-primary" :disabled="busy" data-approve-step @click="review(s, true)"><ShieldCheck /> Approve</button>
            <button class="btn btn-sm" :disabled="busy" data-reject-step @click="review(s, false)">Request changes</button>
          </div>
          <button v-if="(s.status === 'failed' || s.status === 'blocked') && r.status !== 'cancelled'" class="btn btn-sm" :disabled="busy" data-retry-step @click="retry(s)"><RotateCcw /> Retry</button>
        </div>
      </li>
    </ol>
  </section>
</template>

<style scoped>
.run {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}
.head h2 {
  margin: 0;
}
.small {
  font-size: 12.5px;
}
.bar {
  height: 6px;
  border-radius: 999px;
  background: var(--bg-input);
  overflow: hidden;
}
.fill {
  height: 100%;
  background: linear-gradient(90deg, var(--accent), var(--cyan));
}
.actions {
  display: flex;
  gap: 8px;
}
.steps {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.step {
  display: flex;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
}
.step.st-skipped,
.step.st-cancelled {
  opacity: 0.55;
}
.st-icon {
  width: 17px;
  height: 17px;
  flex: none;
  margin-top: 2px;
  color: var(--text-faint);
}
.st-passed .st-icon {
  color: var(--green);
}
.st-failed .st-icon {
  color: var(--red);
}
.st-blocked .st-icon,
.st-awaiting-review .st-icon {
  color: var(--amber);
}
.st-verifying .st-icon,
.st-ready .st-icon {
  color: var(--cyan);
}
.body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.row-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.criteria {
  list-style: none;
  padding: 0;
  margin: 0;
}
.criteria .met {
  color: var(--text);
}
.report {
  margin: 0;
}
.failure,
.output {
  margin: 0;
  max-height: 240px;
  overflow: auto;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--bg-input);
  white-space: pre-wrap;
  font-size: 12px;
}
.gate {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.gate .output {
  flex-basis: 100%;
}
.review {
  display: flex;
  gap: 8px;
}
.review .input {
  flex: 1;
}
</style>
