<script setup lang="ts">
// Usage dashboard (ADR-0030): token usage read locally from Claude Code's session transcripts.
// Measured counts, estimates and unavailable values are always labelled apart.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { listen } from '@tauri-apps/api/event'
import { BarChart3, Info, RefreshCw, ShieldCheck, X } from '@lucide/vue'
import { api, USAGE_EVENT } from '../lib/ipc'
import type { SessionUsage, UsageDimension, UsageFilter, UsageGroup, UsageView } from '../lib/types'
import { relativeTime } from '../lib/format'
import { DIMENSIONS, PERIODS, STATUS, buckets, compact, costText, grouped, percent, totalTokens, type PeriodId } from '../lib/usage'
import UsageTrendChart from '../components/UsageTrendChart.vue'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const workspaces = useWorkspaceStore()
const route = useRoute()
const router = useRouter()

const q = (k: string) => (typeof route.query[k] === 'string' ? (route.query[k] as string) : '')
const period = ref<PeriodId>((PERIODS.find((p) => p.id === q('period'))?.id ?? '30d') as PeriodId)
const projectId = ref(q('project'))
const profileId = ref(q('profile'))
const strategy = ref(q('strategy'))
const model = ref(q('model'))
const taskId = ref(q('task'))
const workflowRunId = ref(q('workflow'))
const by = ref<UsageDimension>((DIMENSIONS.find((d) => d.id === q('by'))?.id ?? 'project') as UsageDimension)

const main = ref<UsageView | null>(null)
const byStrategy = ref<UsageGroup[]>([])
const models = ref<string[]>([])
const sessions = ref<SessionUsage[]>([])
const loading = ref(false)
const scanning = ref(false)

const days = computed(() => PERIODS.find((p) => p.id === period.value)?.days ?? 30)
const filter = computed<UsageFilter>(() => ({
  from: days.value > 0 ? Date.now() - days.value * 86_400_000 : null,
  workspaceId: workspaces.active?.id ?? null,
  projectId: projectId.value || null,
  profileId: profileId.value || null,
  strategy: strategy.value || null,
  model: model.value || null,
  taskId: taskId.value || null,
  workflowRunId: workflowRunId.value || null,
}))

let seq = 0
async function load() {
  const mine = ++seq
  loading.value = true
  try {
    const f = filter.value
    const [m, s, ms, list] = await Promise.all([
      api.usageReport(f, by.value),
      api.usageReport({ ...f, strategy: null }, 'strategy'),
      api.usageReport({ ...f, model: null }, 'model'),
      api.usageSessions(f, 100),
    ])
    if (mine !== seq) return
    main.value = m
    byStrategy.value = s.report.groups
    models.value = ms.report.groups.map((g) => g.key).filter(Boolean)
    sessions.value = list
  } catch (e) {
    app.toast('error', `Could not load usage: ${String((e as { message?: string }).message ?? e)}`)
  } finally {
    if (mine === seq) loading.value = false
  }
}

async function scan() {
  scanning.value = true
  await app.attempt(() => api.usageScan(), 'Could not read transcripts')
  scanning.value = false
  await load()
}

watch([period, projectId, profileId, strategy, model, taskId, workflowRunId, by], () => {
  const query: Record<string, string> = {}
  if (period.value !== '30d') query.period = period.value
  for (const [k, v] of [['project', projectId], ['profile', profileId], ['strategy', strategy], ['model', model], ['task', taskId], ['workflow', workflowRunId]] as const) {
    if (v.value) query[k] = v.value
  }
  if (by.value !== 'project') query.by = by.value
  void router.replace({ query })
  void load()
})
watch(() => workspaces.active?.id, () => void load())

let unlisten: (() => void) | null = null
let timer: ReturnType<typeof setTimeout> | null = null
onMounted(async () => {
  unlisten = await listen(USAGE_EVENT, () => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void load(), 400)
  })
  await scan()
})
onBeforeUnmount(() => {
  unlisten?.()
  if (timer) clearTimeout(timer)
})

const r = computed(() => main.value?.report ?? null)
const trend = computed(() => (r.value ? buckets(r.value.daily, days.value) : []))
const maxGroup = computed(() => Math.max(1, ...(r.value?.groups ?? []).map((g) => totalTokens(g.tokens))))
const unavailable = computed(() => (r.value ? r.value.coverage.missing + r.value.coverage.unreadable : 0))
const managed = computed(() => (main.value?.telemetry ?? []).filter((t) => t.managedTelemetry))
const strategyLabel = (s: string) => (s === 'unrecorded' ? 'Not recorded' : s.charAt(0).toUpperCase() + s.slice(1))
const chips = computed(() => [
  taskId.value ? { k: 'task', label: `Task ${sessions.value.find((s) => s.taskId === taskId.value)?.task ?? taskId.value.slice(-6)}`, clear: () => (taskId.value = '') } : null,
  workflowRunId.value ? { k: 'workflow', label: `Workflow run ${sessions.value.find((s) => s.workflowRunId === workflowRunId.value)?.workflow ?? workflowRunId.value.slice(-6)}`, clear: () => (workflowRunId.value = '') } : null,
].filter(Boolean) as { k: string; label: string; clear: () => void }[])
const PARSED = "Reported by the Claude API per response, parsed from Claude Code's local session transcripts (ADR-0010: parsed)"
const costNote = computed(() => {
  const c = r.value?.cost
  if (!c) return ''
  if (model.value) return 'not available per model (costs are per session)'
  const parts: string[] = []
  if (c.usd != null) parts.push("Claude Code's own estimate at API prices — not a bill")
  if (c.withheldSubscription) parts.push(`${c.withheldSubscription} subscription session${c.withheldSubscription === 1 ? '' : 's'} not converted into spend`)
  if (c.withheldUnknown) parts.push(`${c.withheldUnknown} session${c.withheldUnknown === 1 ? '' : 's'} with unknown billing — check the profile's sign-in under Claude profiles`)
  return parts.join(' · ') || 'no cost snapshot in the transcripts'
})
const maxStrategyContext = computed(() => Math.max(1, ...byStrategy.value.map((g) => g.efficiency.contextPerRequest ?? 0)))
</script>

<template>
  <div class="page usage" data-usage-view>
    <div class="page-header">
      <div>
        <h1>Usage</h1>
        <p>Token usage of your Claude sessions, read locally from the session transcripts Claude Code writes. Nothing leaves this machine, and NEXUS never changes telemetry settings.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="scanning" data-usage-scan @click="scan"><RefreshCw :class="{ spin: scanning }" /> Read transcripts</button>
      </div>
    </div>

    <!-- Filters: one row, scoping everything below. Date range first. -->
    <div class="filters" data-usage-filters>
      <select v-model="period" class="select" aria-label="Period" data-usage-period>
        <option v-for="p in PERIODS" :key="p.id" :value="p.id">{{ p.label }}</option>
      </select>
      <select v-model="projectId" class="select" aria-label="Project" data-usage-project>
        <option value="">All projects</option>
        <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <select v-model="profileId" class="select" aria-label="Profile" data-usage-profile>
        <option value="">All profiles</option>
        <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
      <select v-model="strategy" class="select" aria-label="Context strategy" data-usage-strategy>
        <option value="">All context strategies</option>
        <option v-for="s in ['minimal', 'balanced', 'deep', 'custom', 'unrecorded']" :key="s" :value="s">{{ strategyLabel(s) }}</option>
      </select>
      <select v-model="model" class="select" aria-label="Model" data-usage-model>
        <option value="">All models</option>
        <option v-for="m in models" :key="m" :value="m">{{ m }}</option>
      </select>
      <span v-for="c in chips" :key="c.k" class="filter-chip" :data-usage-chip="c.k">{{ c.label }} <button class="btn-icon btn-ghost btn-sm" :aria-label="`Clear ${c.k} filter`" @click="c.clear()"><X /></button></span>
    </div>

    <p class="kinds" data-usage-legend>
      <span class="badge badge-ok">Parsed</span> token counts measured by the Claude API per response, read from Claude Code's local transcripts — an undocumented format NEXUS checks per Claude Code version, so values may be inaccurate after a Claude update.
      <span class="badge badge-accent">Estimate</span> Claude Code's own cost estimate at API prices (API-billed profiles only — subscription usage is never shown as spend; not a bill) and NEXUS's estimate of the context it added (characters ÷ 4).
      <span class="badge badge-warn">Unavailable</span> no transcript, not readable or not attributable — shown with the reason, never as zero.
    </p>
    <p v-if="r && !r.formatVerified" class="notice notice-warn" data-usage-unverified>
      Transcripts of Claude Code {{ r.claudeVersions.join(', ') }} use a format NEXUS has not verified yet — the parsed values may be inaccurate.
    </p>

    <div v-if="!r && loading" class="faint">Reading usage…</div>
    <template v-else-if="r">
      <div class="tiles" :class="{ dimmed: loading }">
        <div class="tile" data-usage-tile="tokens">
          <span class="tile-label">Tokens <span class="badge badge-ok" :title="PARSED">Parsed</span></span>
          <strong class="tile-value">{{ compact(totalTokens(r.totals)) }}</strong>
          <span class="tile-sub">input {{ compact(r.totals.input) }} · output {{ compact(r.totals.output) }} · cache read {{ compact(r.totals.cacheRead) }} · cache write {{ compact(r.totals.cacheWrite) }}</span>
        </div>
        <div class="tile" data-usage-tile="requests">
          <span class="tile-label">API requests <span class="badge badge-ok" :title="PARSED">Parsed</span></span>
          <strong class="tile-value">{{ grouped(r.totals.requests) }}</strong>
          <span class="tile-sub">in {{ r.sessions }} session{{ r.sessions === 1 ? '' : 's' }}<template v-if="r.totals.subagentRequests"> · {{ grouped(r.totals.subagentRequests) }} by subagents</template></span>
        </div>
        <div class="tile" data-usage-tile="context">
          <span class="tile-label">Context / request <span class="badge badge-ok" :title="PARSED">Parsed</span></span>
          <strong class="tile-value">{{ compact(r.efficiency.contextPerRequest) }}</strong>
          <span class="tile-sub">{{ percent(r.efficiency.cacheHit) }} served from the prompt cache</span>
        </div>
        <div class="tile" data-usage-tile="cost">
          <span class="tile-label">Cost <span class="badge badge-accent">Estimate</span></span>
          <strong class="tile-value">{{ costText(r.cost) }}</strong>
          <span class="tile-sub">{{ costNote }}</span>
        </div>
        <div class="tile" data-usage-tile="nexus-context">
          <span class="tile-label">Added by NEXUS <span class="badge badge-accent">Estimate</span></span>
          <strong class="tile-value">{{ r.contextEstimate.tokensPerSession == null ? '–' : `≈ ${compact(r.contextEstimate.tokensPerSession)}` }}</strong>
          <span class="tile-sub">tokens per session{{ r.contextEstimate.shareOfContext == null ? '' : ` · ≈ ${percent(r.contextEstimate.shareOfContext)} of each request's context` }}</span>
        </div>
        <div class="tile" data-usage-tile="coverage">
          <span class="tile-label">Coverage <span v-if="unavailable" class="badge badge-warn">{{ unavailable }} unavailable</span></span>
          <strong class="tile-value">{{ r.coverage.measured }} / {{ r.coverage.sessions }}</strong>
          <span class="tile-sub">sessions measured · {{ r.coverage.noRequests }} without API requests · {{ r.coverage.pending }} pending<template v-if="r.coverage.outsideNexusRequests"> · {{ r.coverage.outsideNexusRequests }} responses from outside NEXUS not attributed</template></span>
        </div>
      </div>

      <section class="card">
        <div class="card-title"><BarChart3 /> Tokens per {{ trend[0]?.days === 7 ? 'week' : 'day' }} <span class="badge badge-ok" :title="PARSED">Parsed</span></div>
        <UsageTrendChart :buckets="trend" :dimmed="loading" />
      </section>

      <section class="card" data-usage-breakdown>
        <div class="card-title">Breakdown and efficiency</div>
        <div class="tabs" role="tablist">
          <button v-for="d in DIMENSIONS" :key="d.id" role="tab" class="tab" :class="{ active: by === d.id }" :aria-selected="by === d.id" :data-usage-by="d.id" @click="by = d.id">{{ d.label }}</button>
        </div>
        <table v-if="r.groups.length" class="table">
          <thead>
            <tr>
              <th>{{ DIMENSIONS.find((d) => d.id === by)?.label }}</th>
              <th class="bar-col">Tokens</th>
              <th class="num">Sessions</th>
              <th class="num">Requests</th>
              <th class="num">Per session</th>
              <th class="num">Context / request</th>
              <th class="num">Cache hit</th>
              <th class="num">Cost <span class="faint">(est.)</span></th>
              <th class="num">NEXUS context <span class="faint">(est.)</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="g in r.groups" :key="g.key" :data-usage-group="g.key || 'none'">
              <td class="label" :title="g.label">{{ g.label }}</td>
              <td class="bar-col">
                <span class="bar"><span class="fill" :style="{ width: `${Math.max(1, (totalTokens(g.tokens) / maxGroup) * 100)}%` }" /></span>
                <span class="bar-value">{{ compact(totalTokens(g.tokens)) }}</span>
              </td>
              <td class="num">{{ by === 'model' ? '–' : g.sessions }}</td>
              <td class="num">{{ grouped(g.tokens.requests) }}</td>
              <td class="num">{{ by === 'model' ? '–' : compact(g.efficiency.tokensPerSession) }}</td>
              <td class="num">{{ compact(g.efficiency.contextPerRequest) }}</td>
              <td class="num">{{ percent(g.efficiency.cacheHit) }}</td>
              <td class="num">{{ by === 'model' ? 'per session only' : costText(g.cost) }}</td>
              <td class="num">{{ g.contextEstimate.tokensPerSession == null ? '–' : `≈ ${compact(g.contextEstimate.tokensPerSession)}` }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="faint">No measured usage for this selection.</p>
      </section>

      <section class="card" data-usage-strategies>
        <div class="card-title">Context strategies compared</div>
        <p class="faint small">Parsed per request, with NEXUS's own share as an estimate. Projects and tasks differ — compare like with like by choosing a project above.</p>
        <div v-if="byStrategy.length" class="compare">
          <div v-for="g in byStrategy" :key="g.key" class="compare-row" :data-usage-strategy-row="g.key">
            <span class="compare-name">{{ g.label }}</span>
            <span class="bar wide"><span class="fill" :style="{ width: `${Math.max(1, ((g.efficiency.contextPerRequest ?? 0) / maxStrategyContext) * 100)}%` }" /></span>
            <span class="compare-fig"><strong>{{ compact(g.efficiency.contextPerRequest) }}</strong> context / request</span>
            <span class="compare-fig"><strong>{{ percent(g.efficiency.cacheHit) }}</strong> cache hit</span>
            <span class="compare-fig"><strong>{{ compact(g.efficiency.tokensPerSession) }}</strong> per session ({{ g.sessions }})</span>
            <span class="compare-fig"><strong>{{ g.contextEstimate.shareOfContext == null ? '–' : `≈ ${percent(g.contextEstimate.shareOfContext)}` }}</strong> added by NEXUS <span class="faint">(est.)</span></span>
          </div>
        </div>
        <p v-else class="faint">No measured sessions yet.</p>
      </section>

      <section class="card" data-usage-sessions>
        <div class="card-title">Sessions</div>
        <table v-if="sessions.length" class="table">
          <thead>
            <tr><th>Session</th><th>Usage</th><th>Project</th><th>Profile</th><th>Strategy</th><th class="num">Requests</th><th class="num">Tokens</th><th class="num">Cost <span class="faint">(est.)</span></th><th class="num">NEXUS context <span class="faint">(est.)</span></th></tr>
          </thead>
          <tbody>
            <tr v-for="s in sessions" :key="s.sessionId" :data-usage-session="s.sessionId" :data-usage-status="s.status">
              <td class="label" :title="s.title"><span>{{ s.title }}</span><span class="faint small"> · {{ relativeTime(s.startedAt) }}</span><span v-if="s.task" class="faint small"> · {{ s.task }}</span></td>
              <td>
                <span class="badge" :class="`badge-${STATUS[s.status].tone}`" :title="s.detail ?? ''">{{ STATUS[s.status].label }}{{ s.live && s.status === 'measured' ? ' (live)' : '' }}</span>
                <div v-if="s.detail" class="faint small detail">{{ s.detail }}</div>
              </td>
              <td>{{ s.project ?? '–' }}</td>
              <td>{{ s.profile ?? '–' }}</td>
              <td>{{ strategyLabel(s.strategy) }}</td>
              <td class="num">{{ s.status === 'measured' ? grouped(s.tokens.requests) : '–' }}</td>
              <td class="num">{{ s.status === 'measured' ? compact(totalTokens(s.tokens)) : '–' }}</td>
              <td class="num">{{ costText(s.cost) }}</td>
              <td class="num">{{ s.contextEstimate == null ? '–' : `≈ ${compact(s.contextEstimate)}` }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="faint">No Claude sessions in this period.</p>
      </section>

      <p class="notice" data-usage-privacy>
        <ShieldCheck /> NEXUS reads Claude Code's transcripts in each profile's folder — read-only, numbers only (no prompt or response text is stored). It never sets or forwards telemetry variables.
        <template v-for="t in managed" :key="t.profileId"><br /><Info /> Telemetry for profile <strong>{{ t.profile }}</strong> is configured by your organisation; NEXUS leaves it unchanged.</template>
        <template v-if="r.claudeVersions.length"><br />Transcripts written by Claude Code {{ r.claudeVersions.join(', ') }}.</template>
      </p>
    </template>
  </div>
</template>

<style scoped>
.usage {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.filters {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.filters .select {
  width: auto;
  flex: 0 1 190px;
  min-width: 140px;
}
.filter-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 4px 2px 10px;
  border-radius: 999px;
  background: var(--accent-soft);
  font-size: 12px;
}
.kinds {
  margin: 0;
  font-size: 12px;
  color: var(--text-muted);
  line-height: 1.9;
}
.kinds .badge {
  margin-left: 8px;
}
.kinds .badge:first-child {
  margin-left: 0;
}
.badge-muted {
  color: var(--text-muted);
  background: var(--bg-hover);
}
.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 12px;
}
.tab {
  height: 28px;
  padding: 0 11px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}
.tab.active {
  background: var(--bg-active);
  border-color: var(--border-strong);
  color: var(--text);
  font-weight: 600;
}
.tiles {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
  gap: 10px;
  transition: opacity 0.15s;
}
.tiles.dimmed {
  opacity: 0.55;
}
.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 12px 14px;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  background: var(--bg-panel);
}
.tile-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-muted);
}
.tile-value {
  font-size: 24px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--text);
}
.tile-sub {
  font-size: 12px;
  color: var(--text-faint);
}
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.table th {
  text-align: left;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-faint);
  padding: 6px;
}
.table td {
  padding: 6px;
  border-top: 1px solid var(--border);
  vertical-align: top;
}
.num {
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
.label {
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.bar-col {
  min-width: 180px;
}
.bar {
  display: inline-block;
  width: calc(100% - 52px);
  max-width: 220px;
  height: 8px;
  vertical-align: middle;
}
.bar.wide {
  width: 100%;
  max-width: none;
}
.fill {
  display: block;
  height: 8px;
  border-radius: 0 4px 4px 0;
  background: var(--series-1);
}
.bar-value {
  display: inline-block;
  width: 48px;
  margin-left: 4px;
  font-variant-numeric: tabular-nums;
  color: var(--text-muted);
  font-size: 12px;
}
.compare {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.compare-row {
  display: grid;
  grid-template-columns: 110px minmax(120px, 1fr) 150px 100px 150px 170px;
  align-items: center;
  gap: 14px;
  font-size: 12px;
  color: var(--text-muted);
}
.compare-name {
  color: var(--text);
  font-weight: 500;
}
.compare-fig {
  text-align: right;
  white-space: nowrap;
}
.compare-fig strong {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
.small {
  font-size: 12px;
}
.detail {
  max-width: 280px;
  white-space: normal;
}
.notice svg {
  width: 14px;
  height: 14px;
  vertical-align: -2px;
}
</style>
