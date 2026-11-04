<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { CircleCheck, CircleMinus, CirclePlus, Gauge, Info, RefreshCw, Sparkles } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { ContextAnalysis, ContextConfig, ContextReport, ContextSettings, ContextStrategy, ProjectDocEntry } from '../lib/types'
import { relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'

const app = useAppStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const route = useRoute()

type Tab = 'strategy' | 'inspector' | 'metrics'
const tab = ref<Tab>('strategy')
const projectId = ref('')
const profileId = ref('')
const config = ref<ContextConfig | null>(null)
const analysis = ref<ContextAnalysis | null>(null)
const report = ref<ContextReport | null>(null)
const docs = ref<ProjectDocEntry[]>([])
const presets = ref<Record<string, ContextSettings>>({})
const custom = ref<ContextSettings | null>(null)
const busy = ref(false)

const STRATEGIES: { id: ContextStrategy; name: string; summary: string }[] = [
  { id: 'minimal', name: 'Minimal', summary: 'Mandatory policies and the task brief only. Everything else on demand.' },
  { id: 'balanced', name: 'Balanced', summary: 'Approved rules and constitution at start, a one-line pointer to knowledge, the rest on demand.' },
  { id: 'deep', name: 'Deep', summary: 'Larger rule budget, an index of open tasks and decisions, recent checkpoints at start.' },
  { id: 'custom', name: 'Custom', summary: 'Choose each source and budget yourself.' },
]

const project = computed(() => projects.list.find((p) => p.id === projectId.value) ?? null)
const estimate = (s: ContextStrategy) => analysis.value?.estimates.find((e) => e.strategy === s) ?? null
const predictedFiles = computed(() => (report.value?.predicted?.claude.layers ?? []).filter((l) => l.load === 'at-start' && l.path))
const kn = computed(() => report.value?.onDemand.knowledge)

async function load() {
  if (!projectId.value) return
  busy.value = true
  try {
    const [c, a, d] = await Promise.all([api.contextGet(projectId.value), api.contextAnalyze(projectId.value), api.contextDocs(projectId.value)])
    config.value = c
    analysis.value = a
    docs.value = d
    custom.value = JSON.parse(JSON.stringify(c.settings)) as ContextSettings
    if (tab.value !== 'strategy') await inspect()
  } catch (e) {
    app.toast('error', `Could not analyse the project: ${String((e as { message?: string }).message ?? e)}`)
  } finally {
    busy.value = false
  }
}

async function inspect() {
  if (!projectId.value) return
  report.value = (await app.attempt(() => api.contextInspect(projectId.value, profileId.value || null), 'Could not inspect the context')) ?? null
}

async function choose(s: ContextStrategy) {
  if (!projectId.value) return
  const c = await app.attempt(() => api.contextSet(projectId.value, s, s === 'custom' ? custom.value : null), 'Could not save the strategy')
  if (c) {
    config.value = c
    custom.value = JSON.parse(JSON.stringify(c.settings)) as ContextSettings
    app.toast('success', `${project.value?.name}: ${s} context for new sessions`)
    if (analysis.value) analysis.value.configured = s
  }
}

function startCustom() {
  const base = presets.value[config.value?.strategy === 'custom' ? 'balanced' : (config.value?.strategy ?? 'balanced')]
  custom.value = JSON.parse(JSON.stringify(config.value?.strategy === 'custom' ? config.value.settings : base)) as ContextSettings
}

watch(projectId, load)
watch(tab, (t) => t !== 'strategy' && inspect())
watch(profileId, () => tab.value !== 'strategy' && inspect())
watch(
  () => route.query.project,
  (q) => {
    if (typeof q === 'string' && q) projectId.value = q
  },
  { immediate: true },
)
onActivated(() => {
  if (!projectId.value && projects.list[0]) projectId.value = projects.list[0].id
  if (!profileId.value && profiles.list[0]) profileId.value = profiles.list[0].id
})
api.contextPresets()
  .then((list) => (presets.value = Object.fromEntries(list)))
  .catch(() => undefined)
if (!projectId.value && projects.list[0]) projectId.value = projects.list[0].id
if (!profileId.value && profiles.list[0]) profileId.value = profiles.list[0].id
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Context</h1>
        <p>How much NEXUS gives each Claude session for a project, from which sources — and what actually happened. Claude's own CLAUDE.md layers and organisation policy are shown as they are; NEXUS only adds, never replaces.</p>
      </div>
      <div class="page-actions">
        <select v-model="projectId" class="select" aria-label="Project" data-context-project>
          <option value="" disabled>Choose a project…</option>
          <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <button class="btn btn-ghost" :disabled="busy" @click="load"><RefreshCw :class="{ spin: busy }" /> Refresh</button>
      </div>
    </div>

    <div v-if="!projects.list.length" class="empty">
      <Gauge class="empty-icon" />
      <h3>No projects yet</h3>
      <p>Register a project first; NEXUS then analyses its context sources.</p>
    </div>

    <template v-else-if="project">
      <div class="tabs" role="tablist">
        <button v-for="t in (['strategy', 'inspector', 'metrics'] as Tab[])" :key="t" role="tab" class="tab" :class="{ active: tab === t }" :aria-selected="tab === t" :data-tab="t" @click="tab = t">
          {{ t === 'strategy' ? 'Strategy' : t === 'inspector' ? 'Inspector' : 'Retrieval metrics' }}
        </button>
      </div>

      <!-- Strategy (onboarding wizard) -->
      <section v-if="tab === 'strategy' && analysis" class="strategy" data-context-strategy>
        <div class="card facts">
          <div class="card-title">What NEXUS found in {{ analysis.project }}</div>
          <div class="fact-grid">
            <div><strong>{{ analysis.docs.files }}</strong><span>project docs ({{ Math.round(analysis.docs.bytes / 1024) }} KB)</span></div>
            <div><strong>{{ analysis.knowledge.decisions }}</strong><span>decisions</span></div>
            <div><strong>{{ analysis.knowledge.openTasks }}</strong><span>open tasks</span></div>
            <div><strong>{{ analysis.knowledge.notes }}</strong><span>notes</span></div>
            <div><strong>{{ analysis.constitutionRules }}</strong><span>constitution rules ({{ analysis.constitutionMandatory }} mandatory)</span></div>
            <div><strong>{{ analysis.instructions.claudeMd ? 'yes' : 'no' }}</strong><span>CLAUDE.md in the project</span></div>
          </div>
          <p v-if="analysis.docs.skipped.length" class="hint">Left out of the docs index: {{ analysis.docs.skipped.join(', ') }}</p>
          <p class="recommend" data-recommendation><Sparkles /> Recommended: <strong>{{ STRATEGIES.find((s) => s.id === analysis!.recommendation)?.name }}</strong> — {{ analysis.reasons.join('; ') }}.</p>
          <p v-if="config && !config.configured" class="notice notice-accent" data-not-configured><Info /> Not chosen yet — sessions use Balanced, the default.</p>
        </div>

        <div class="cards">
          <div v-for="s in STRATEGIES" :key="s.id" class="card choice" :class="{ current: config?.configured && config.strategy === s.id, recommended: analysis.recommendation === s.id }" :data-strategy="s.id">
            <div class="choice-head">
              <strong>{{ s.name }}</strong>
              <span v-if="config?.configured && config.strategy === s.id" class="badge badge-ok">in use</span>
              <span v-else-if="analysis.recommendation === s.id" class="badge badge-accent">recommended</span>
            </div>
            <p class="small">{{ s.summary }}</p>
            <p v-if="estimate(s.id)" class="small faint">
              ~{{ estimate(s.id)!.rulesChars.toLocaleString() }} chars of rules at launch ({{ estimate(s.id)!.rulesInPrompt }} in the prompt, {{ estimate(s.id)!.onDemand }} on demand) · SessionStart ≤ {{ estimate(s.id)!.startupCharsMax.toLocaleString() }}
            </p>
            <button v-if="s.id !== 'custom'" class="btn btn-sm" :class="{ 'btn-primary': analysis.recommendation === s.id }" :data-use="s.id" @click="choose(s.id)">Use {{ s.name }}</button>
            <button v-else class="btn btn-sm" data-custom-open @click="startCustom">Configure…</button>
          </div>
        </div>

        <div v-if="custom" class="card custom" data-custom>
          <div class="card-title">Sources and budgets</div>
          <div class="grid-2">
            <label class="checkbox"><input v-model="custom.alwaysRules" type="checkbox" /><span>Always-on rule documents at launch</span></label>
            <label class="field-inline">
              <span>Constitution at launch</span>
              <select v-model="custom.constitution" class="select">
                <option value="rules">Mandatory policies and rules</option>
                <option value="mandatory">Mandatory policies only</option>
                <option value="on-demand">Nothing (on demand)</option>
              </select>
            </label>
            <label class="field-inline"><span>Rule budget (chars)</span><input v-model.number="custom.rulesBudget" class="input" type="number" min="300" max="4000" step="100" /></label>
            <label class="field-inline"><span>SessionStart limit (chars)</span><input v-model.number="custom.startupChars" class="input" type="number" min="300" max="4000" step="100" /></label>
            <label class="checkbox"><input v-model="custom.startupSummary" type="checkbox" /><span>Knowledge summary line at start</span></label>
            <label class="checkbox"><input v-model="custom.startupIndex" type="checkbox" /><span>Open tasks and decisions index at start</span></label>
            <label class="checkbox"><input v-model="custom.checkpointsAtStart" type="checkbox" /><span>Latest checkpoints at start</span></label>
            <label class="checkbox"><input v-model="custom.projectDocs" type="checkbox" /><span>Project README/docs searchable on demand</span></label>
            <label class="checkbox"><input v-model="custom.observeInstructions" type="checkbox" /><span>Observe instruction files Claude loads (hook)</span></label>
            <label class="field-inline"><span>Search results (max)</span><input v-model.number="custom.retrievalLimit" class="input" type="number" min="1" max="20" /></label>
            <label class="field-inline"><span>Document slice (chars)</span><input v-model.number="custom.retrievalChars" class="input" type="number" min="1000" max="40000" step="1000" /></label>
          </div>
          <div class="row gap"><button class="btn btn-primary" data-use="custom" @click="choose('custom')">Use custom</button><button class="btn" @click="custom = null">Close</button></div>
        </div>

        <div v-if="docs.length" class="card">
          <div class="card-title">Project documents available on demand ({{ docs.length }})</div>
          <div class="doc-rows">
            <div v-for="d in docs" :key="d.path" class="doc-row"><code>{{ d.path }}</code><span class="faint small">{{ d.title }} · {{ relativeTime(d.modifiedMs) }}</span></div>
          </div>
        </div>
      </section>

      <!-- Inspector -->
      <section v-else-if="tab === 'inspector'" class="inspector" data-context-inspector>
        <div class="filters">
          <select v-model="profileId" class="select" aria-label="Profile for the prediction">
            <option value="">No prediction (choose a profile)</option>
            <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
        </div>
        <template v-if="report">
          <div class="cols">
            <div class="card" data-col="configured">
              <div class="card-title">Configured</div>
              <p><strong>{{ report.configured.strategy }}</strong><span v-if="!report.configured.configured" class="faint"> (default)</span></p>
              <ul class="small plain">
                <li>Rules at launch: {{ report.configured.settings.alwaysRules ? 'yes' : 'on demand' }}, constitution: {{ report.configured.settings.constitution }}, budget {{ report.configured.settings.rulesBudget }}</li>
                <li>SessionStart: {{ [report.configured.settings.startupSummary && 'summary', report.configured.settings.startupIndex && 'index', report.configured.settings.checkpointsAtStart && 'checkpoints'].filter(Boolean).join(', ') || 'task brief only' }} (≤ {{ report.configured.settings.startupChars }})</li>
                <li>Retrieval: {{ report.configured.settings.retrievalLimit }} results, {{ report.configured.settings.retrievalChars.toLocaleString() }} chars, project docs {{ report.configured.settings.projectDocs ? 'on' : 'off' }}</li>
              </ul>
            </div>
            <div class="card" data-col="predicted">
              <div class="card-title">Predicted for the next session</div>
              <template v-if="report.predicted">
                <p class="small">{{ predictedFiles.length }} instruction file(s) Claude loads itself at start · NEXUS appends {{ report.predicted.nexus.appendedChars.toLocaleString() }} chars</p>
                <ul class="small plain mono">
                  <li v-for="l in predictedFiles" :key="l.path ?? ''">{{ l.path }}</li>
                </ul>
                <p v-if="report.predicted.nexus.rules?.constitution" class="small">Constitution: {{ report.predicted.nexus.rules.constitution.included.length }} in the prompt, {{ report.predicted.nexus.rules.constitution.onDemand.length }} by id</p>
              </template>
              <p v-else class="faint small">Choose a profile above.</p>
            </div>
            <div class="card" data-col="observed">
              <div class="card-title">Observed in the latest session</div>
              <template v-if="report.observed.session">
                <p class="small">“{{ report.observed.session.title }}” · {{ relativeTime(report.observed.session.startedAt) }}</p>
                <p v-if="!report.observed.instructionsObserved" class="notice small"><Info /> Instruction loading was not observed in that session (its strategy or hooks did not include it).</p>
                <ul class="small plain mono">
                  <li v-for="f in report.observed.instructions" :key="f.path" :data-observed="f.predicted ? 'predicted' : 'unexpected'">
                    <CircleCheck v-if="f.predicted" class="ok" /><CirclePlus v-else class="warn" /> {{ f.path }} <span class="faint">({{ f.memoryType }}, {{ f.loadReason }})</span>
                  </li>
                  <li v-for="m in report.observed.missing" :key="m" data-observed="missing"><CircleMinus class="warn" /> {{ m }} <span class="faint">predicted, not reported</span></li>
                </ul>
                <p v-if="report.observed.nexus?.rules" class="small">NEXUS appended: {{ report.observed.nexus.rules.included.join(', ') || 'no rules' }}</p>
              </template>
              <p v-else class="faint small">No Claude session of this project yet.</p>
            </div>
            <div class="card" data-col="on-demand">
              <div class="card-title">Available on demand</div>
              <ul v-if="kn" class="small plain">
                <li>{{ kn.decisions }} decisions · {{ kn.notes }} notes · {{ kn.specs }} specs · {{ kn.openTasks }} open tasks · {{ kn.workflows }} workflows</li>
                <li>{{ report.onDemand.projectDocs }} project documents</li>
                <li v-if="report.onDemand.constitutionRules.length">Constitution by id: {{ report.onDemand.constitutionRules.join(', ') }}</li>
                <li class="faint">Through NEXUS tools: {{ report.onDemand.tools.join(', ') }}</li>
              </ul>
            </div>
            <div class="card wide" data-col="retrieved">
              <div class="card-title">Actually retrieved in the latest session</div>
              <table v-if="report.retrieved.length" class="small table">
                <thead><tr><th>Tool</th><th>Results</th><th>Chars</th><th>Documents</th></tr></thead>
                <tbody>
                  <tr v-for="(r, i) in report.retrieved" :key="i"><td class="mono">{{ r.tool }}</td><td>{{ r.results }}</td><td>{{ r.chars.toLocaleString() }}</td><td class="mono">{{ r.ids.join(', ') }}</td></tr>
                </tbody>
              </table>
              <p v-else class="faint small">Nothing retrieved through NEXUS in that session.</p>
            </div>
          </div>
        </template>
      </section>

      <!-- Metrics -->
      <section v-else-if="tab === 'metrics' && report" class="metrics" data-context-metrics>
        <div class="fact-grid">
          <div><strong>{{ report.metrics.sessions }}</strong><span>recent sessions</span></div>
          <div><strong>{{ report.metrics.sessionsWithRetrieval }}</strong><span>with retrieval</span></div>
          <div><strong>{{ report.metrics.retrievals }}</strong><span>retrievals</span></div>
          <div><strong>{{ report.metrics.empty }}</strong><span>returned nothing</span></div>
          <div><strong>{{ report.metrics.chars.toLocaleString() }}</strong><span>chars retrieved</span></div>
        </div>
        <div class="grid-2">
          <div class="card">
            <div class="card-title">By tool</div>
            <p v-for="[t, n] in report.metrics.byTool" :key="t" class="small"><code>{{ t }}</code> × {{ n }}</p>
          </div>
          <div class="card">
            <div class="card-title">Most retrieved</div>
            <p v-for="[d, n] in report.metrics.topDocuments" :key="d" class="small mono">{{ d }} × {{ n }}</p>
          </div>
        </div>
        <p class="notice small"><Info /> Counts and sizes only — NEXUS does not store the queries Claude sends.</p>
      </section>
    </template>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 6px;
  margin-bottom: 14px;
}
.tab {
  height: 30px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: transparent;
  cursor: pointer;
}
.tab.active {
  background: var(--bg-active);
  border-color: var(--border-strong);
  font-weight: 600;
}
.page-actions .select {
  min-width: 200px;
}
.strategy,
.inspector,
.metrics {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.fact-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
  margin: 8px 0;
}
.fact-grid div {
  display: flex;
  flex-direction: column;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
}
.fact-grid strong {
  font-size: 18px;
}
.fact-grid span {
  font-size: 12px;
  color: var(--text-muted);
}
.recommend {
  display: flex;
  align-items: center;
  gap: 6px;
}
.recommend svg {
  width: 15px;
  height: 15px;
  color: var(--accent-strong);
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
}
.choice {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.choice.recommended {
  border-color: var(--accent);
}
.choice.current {
  border-color: var(--green);
}
.choice-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.choice .btn {
  align-self: flex-start;
  margin-top: auto;
}
.small {
  font-size: 12.5px;
}
.custom .grid-2 {
  gap: 10px 18px;
  margin-bottom: 12px;
}
.field-inline {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.field-inline .input,
.field-inline .select {
  width: 200px;
}
.gap {
  gap: 8px;
}
.doc-rows {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.doc-row {
  display: flex;
  gap: 10px;
  align-items: baseline;
}
.cols {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 12px;
}
.cols .wide {
  grid-column: 1 / -1;
}
.plain {
  list-style: none;
  padding: 0;
  margin: 6px 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-wrap: anywhere;
}
.plain svg {
  width: 13px;
  height: 13px;
  vertical-align: -2px;
}
.ok {
  color: var(--green);
}
.warn {
  color: var(--amber);
}
.table {
  width: 100%;
  border-collapse: collapse;
}
.table th {
  text-align: left;
  color: var(--text-faint);
  font-weight: 600;
  padding: 4px 6px;
}
.table td {
  padding: 4px 6px;
  border-top: 1px solid var(--border);
}
.filters .select {
  min-width: 240px;
}
</style>
