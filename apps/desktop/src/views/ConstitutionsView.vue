<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { History, Info, Landmark, Plus, RefreshCw, Save, Search, ShieldCheck, TriangleAlert, Undo2 } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { ConstitutionHistory, ConstitutionSummary, ConstitutionView, EffectiveConstitution, KnowledgeItem, RuleHit } from '../lib/types'
import { ENFORCEMENT_LABELS, SCOPE_LABELS, SCOPE_ORDER, clone, counts } from '../lib/constitution'
import { relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import ConstitutionRules from '../components/ConstitutionRules.vue'
import ConstitutionTree from '../components/ConstitutionTree.vue'
import NewConstitutionDialog from '../components/NewConstitutionDialog.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const route = useRoute()

type Tab = 'documents' | 'effective'
/** `read`: the formatted document (default); `structured` / `markdown`: the two editors (W2). */
type Mode = 'read' | 'structured' | 'markdown' | 'history'
const tab = ref<Tab>('documents')
const mode = ref<Mode>('read')
const list = ref<ConstitutionSummary[]>([])
const selectedId = ref<string | null>(null)
const view = ref<ConstitutionView | null>(null)
const draft = ref<ConstitutionView['structure'] | null>(null)
const raw = ref('')
const dirty = ref(false)
const busy = ref(false)
const error = ref<string | null>(null)
const creating = ref(false)
const filter = ref('')
const hits = ref<RuleHit[] | null>(null)
const history = ref<ConstitutionHistory | null>(null)
const oldVersion = ref<{ oid: string; body: string } | null>(null)
// Effective preview
const effProject = ref('')
const effTask = ref('')
const effPending = ref(false)
const effective = ref<EffectiveConstitution | null>(null)
const tasks = ref<KnowledgeItem[]>([])

const wsId = computed(() => workspaces.active?.id ?? null)
const verifying = ref(false)
const complianceOf = (id: string) => effective.value?.compliance?.find((c) => c.ruleId === id) ?? null
const linkedGates = computed(() => [...new Set((effective.value?.compliance ?? []).map((c) => c.check).filter((c): c is string => !!c))])
function complianceLabel(id: string): string {
  const c = complianceOf(id)
  switch (c?.status ?? 'instructed') {
    case 'passed':
      return `Verified: ${c!.check} passed ${c!.at ? relativeTime(c!.at) : ''}`
    case 'failed':
      return `Failing: ${c!.check}`
    case 'not-run':
      return `Gate ${c!.check} not run yet`
    case 'gate-missing':
      return `Gate ${c!.check} not defined`
    case 'not-approved':
      return `Gate ${c!.check} not approved`
    case 'no-project':
      return `Gate ${c!.check} (choose a project)`
    default:
      return 'Instructed only'
  }
}
function complianceTone(id: string) {
  const s = complianceOf(id)?.status
  return { 'badge-ok': s === 'passed', 'badge-error': s === 'failed', 'badge-warn': s === 'gate-missing' || s === 'not-run' || s === 'not-approved' }
}
async function verifyRules() {
  if (!effProject.value) return
  verifying.value = true
  for (const g of linkedGates.value) await app.attempt(() => api.gateRun(effProject.value, g), `Gate ${g} could not run`)
  verifying.value = false
  await loadEffective()
}
const grouped = computed(() => SCOPE_ORDER.map((s) => ({ scope: s, docs: list.value.filter((d) => d.scope === s) })).filter((g) => g.docs.length))
const pendingCount = computed(() => list.value.filter((d) => !d.approved).length)
const projectName = (id: string | null) => projects.list.find((p) => p.id === id)?.name ?? null
const draftCounts = computed(() => (draft.value ? counts(draft.value) : null))

async function loadList() {
  if (!wsId.value) return
  try {
    list.value = await api.constitutionList(wsId.value)
    error.value = null
    if (!selectedId.value && list.value[0]) await select(list.value[0].id)
  } catch (e) {
    error.value = toIpcError(e).message
  }
}

async function select(id: string) {
  if (dirty.value && !(await app.confirm({ title: 'Discard changes?', message: 'This constitution has unsaved changes.', confirmLabel: 'Discard', danger: true }))) return
  selectedId.value = id
  mode.value = 'read'
  await loadView()
}

async function loadView() {
  if (!wsId.value || !selectedId.value) return
  const v = await app.attempt(() => api.constitutionGet(wsId.value!, selectedId.value!), 'Could not open the constitution')
  if (!v) return
  view.value = v
  draft.value = clone(v.structure)
  raw.value = v.body
  dirty.value = false
  history.value = null
  oldVersion.value = null
}

async function save() {
  if (!wsId.value || !view.value || !draft.value) return
  busy.value = true
  try {
    const v =
      mode.value === 'markdown'
        ? await api.constitutionSaveRaw(wsId.value, view.value.id, raw.value, view.value.hash)
        : await api.constitutionSave(wsId.value, view.value.id, draft.value, view.value.hash)
    view.value = v
    draft.value = clone(v.structure)
    raw.value = v.body
    dirty.value = false
    app.toast('success', `Saved “${v.title}” — ${v.rules} rules`)
    await loadList()
  } catch (e) {
    const err = toIpcError(e)
    app.toast('error', err.code === 'conflict' ? 'The file changed on disk since you opened it — reload and apply your change again.' : `Not saved: ${err.message}`)
  } finally {
    busy.value = false
  }
}

async function approve() {
  if (!wsId.value || !view.value) return
  const ok = await app.attempt(() => api.knowledgeApprove(wsId.value!, view.value!.id), 'Could not approve')
  if (ok) {
    app.toast('success', `Approved this version of “${view.value.title}” on this machine`)
    await Promise.all([loadList(), loadView()])
  }
}

function setMode(m: Mode) {
  if (m === mode.value) return
  if (dirty.value && m !== 'history') {
    app.toast('info', 'Save or discard your changes before switching the editor.')
    return
  }
  mode.value = m
  if (m === 'history') loadHistory()
}

async function loadHistory() {
  if (!wsId.value || !view.value) return
  history.value = (await app.attempt(() => api.constitutionHistory(wsId.value!, view.value!.id), 'Could not read the history')) ?? null
}
async function showVersion(oid: string) {
  if (!wsId.value || !view.value) return
  const body = await app.attempt(() => api.constitutionVersion(wsId.value!, view.value!.id, oid), 'Could not read that version')
  if (body !== undefined) oldVersion.value = { oid, body }
}

let searchTimer: ReturnType<typeof setTimeout> | undefined
watch(filter, (q) => {
  clearTimeout(searchTimer)
  if (!q.trim() || !wsId.value) {
    hits.value = null
    return
  }
  searchTimer = setTimeout(async () => {
    hits.value = await api.constitutionSearch(wsId.value!, q).catch(() => [])
  }, 200)
})

async function loadEffective() {
  if (!wsId.value) return
  effective.value = (await app.attempt(() => api.constitutionEffective(wsId.value!, effProject.value || null, effTask.value || null, effPending.value), 'Could not compute the effective rules')) ?? null
}
watch([tab, effProject, effTask, effPending], () => tab.value === 'effective' && loadEffective())
watch(tab, async (t) => {
  if (t === 'effective' && wsId.value && !tasks.value.length) {
    const o = await api.knowledgeOverview(wsId.value).catch(() => null)
    tasks.value = (o?.items ?? []).filter((i) => i.valid && (i.kind === 'task' || i.kind === 'workflow') && i.status !== 'done')
  }
})

async function onCreated(v: ConstitutionView) {
  creating.value = false
  dirty.value = false
  selectedId.value = v.id
  mode.value = 'structured' // a new constitution opens in the editor
  await loadList()
  await loadView()
}

watch(wsId, () => {
  selectedId.value = null
  view.value = null
  loadList()
})
watch(
  () => [route.name, route.query.pending, route.query.doc],
  async () => {
    if (route.name === 'constitutions' && typeof route.query.doc === 'string') {
      // Opened from a knowledge document ("Edit in Constitutions"): show that document.
      tab.value = 'documents'
      await loadList()
      await select(route.query.doc)
    } else if (route.name === 'constitutions' && route.query.pending) {
      // Opened from a review notification: reload (the kept-alive list may be stale) and show what needs review.
      tab.value = 'documents'
      await loadList()
      const first = list.value.find((d) => !d.approved)
      if (first) await select(first.id)
    }
  },
  { immediate: true },
)
onActivated(() => loadList())
loadList()
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Constitutions</h1>
        <p>How Claude must and should work here — articles, sections and rules with stable ids, from global to task scope. Mandatory policies cannot be overridden by narrower scopes; organisation-managed Claude settings always come first.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" @click="loadList"><RefreshCw /> Refresh</button>
        <button class="btn btn-primary" data-new-constitution @click="creating = true"><Plus /> New constitution</button>
      </div>
    </div>
    <div v-if="error" class="notice notice-error selectable">{{ error }}</div>
    <div v-if="pendingCount" class="notice notice-warn" data-pending-banner>
      <TriangleAlert /> {{ pendingCount }} constitution(s) changed outside NEXUS (for example through Git) — Claude does not get those versions until you review and approve them.
    </div>

    <div class="tabs" role="tablist">
      <button role="tab" class="tab" :class="{ active: tab === 'documents' }" :aria-selected="tab === 'documents'" data-tab="documents" @click="tab = 'documents'">Documents</button>
      <button role="tab" class="tab" :class="{ active: tab === 'effective' }" :aria-selected="tab === 'effective'" data-tab="effective" @click="tab = 'effective'">Effective rules</button>
    </div>

    <template v-if="tab === 'documents'">
      <div v-if="!list.length" class="empty">
        <Landmark class="empty-icon" />
        <h3>No constitutions yet</h3>
        <p>Start with a workspace charter. Each rule gets a stable id that Claude can cite, and you decide what is mandatory.</p>
        <button class="btn btn-primary" @click="creating = true"><Plus /> New constitution</button>
      </div>

      <div v-else class="layout">
        <nav class="doc-list" aria-label="Constitutions">
          <div class="search">
            <Search />
            <input v-model="filter" class="input" type="search" placeholder="Search rules (id, title, text)…" aria-label="Search rules" data-rule-search />
          </div>
          <div v-if="hits" class="hits">
            <button v-for="h in hits" :key="h.rule.docId + h.rule.id" class="hit" @click="select(h.rule.docId)">
              <span class="mono rid">{{ h.rule.id }}</span> {{ h.rule.title }}
              <span class="faint small">{{ h.rule.docTitle }} · {{ ENFORCEMENT_LABELS[h.rule.enforcement] }}</span>
            </button>
            <p v-if="!hits.length" class="faint small">No rule matches.</p>
          </div>
          <div v-for="g in grouped" :key="g.scope" class="group">
            <div class="group-label">{{ SCOPE_LABELS[g.scope] }}</div>
            <button v-for="d in g.docs" :key="d.id" class="doc" :class="{ active: d.id === selectedId }" :data-constitution="d.title" @click="select(d.id)">
              <span class="doc-title">{{ d.title }}</span>
              <span class="faint small">
                {{ d.rules }} rules · {{ d.mandatory }} mandatory<template v-if="d.projectId"> · {{ projectName(d.projectId) }}</template><template v-if="d.stacks.length"> · {{ d.stacks.join(', ') }}</template>
              </span>
              <span v-if="!d.approved" class="badge badge-warn">needs approval</span>
              <span v-else-if="d.errors" class="badge badge-error">errors</span>
              <span v-else-if="d.applies === 'on-demand'" class="badge">on demand</span>
            </button>
          </div>
        </nav>

        <section v-if="view && draft" class="detail" data-constitution-detail>
          <header class="detail-head">
            <div>
              <h2>{{ view.title }}</h2>
              <p class="faint small mono selectable">{{ view.relativePath }}</p>
              <p class="small">
                {{ SCOPE_LABELS[view.scope] }} · {{ view.applies === 'always' ? 'mandatory policies and rules reach every session start (bounded)' : 'read on demand' }}
                <template v-if="draftCounts"> · {{ draftCounts.mandatory }} mandatory, {{ draftCounts.rule }} rules, {{ draftCounts.guideline }} guidelines</template>
              </p>
              <div v-if="view.appliesTo.length" class="chips applies">
                <span class="faint small">Applies to</span>
                <span v-for="p in view.appliesTo" :key="p.id" class="chip">{{ p.name }}</span>
              </div>
            </div>
            <div class="segments" role="radiogroup" aria-label="Editor">
              <button class="segment" :class="{ selected: mode === 'read' }" role="radio" :aria-checked="mode === 'read'" data-mode="read" @click="setMode('read')">Read</button>
              <button class="segment" :class="{ selected: mode === 'structured' }" role="radio" :aria-checked="mode === 'structured'" data-mode="structured" @click="setMode('structured')">Edit</button>
              <button class="segment" :class="{ selected: mode === 'markdown' }" role="radio" :aria-checked="mode === 'markdown'" data-mode="markdown" @click="setMode('markdown')">Markdown</button>
              <button class="segment" :class="{ selected: mode === 'history' }" role="radio" :aria-checked="mode === 'history'" data-mode="history" @click="setMode('history')"><History /> History</button>
            </div>
          </header>

          <div v-if="!view.approved" class="notice notice-warn approve" data-review>
            <TriangleAlert />
            <span>This version was changed outside NEXUS. Review it below — Claude keeps using nothing from it until you approve this exact version.</span>
            <button class="btn btn-sm btn-primary" :disabled="dirty" data-approve @click="approve"><ShieldCheck /> Approve this version</button>
          </div>
          <div v-for="p in view.problems.filter((x) => x.severity === 'error')" :key="p.message" class="notice notice-error"><TriangleAlert /> {{ p.message }}</div>

          <ConstitutionRules v-if="mode === 'read'" class="reading" :structure="view.structure" />
          <ConstitutionTree
            v-else-if="mode === 'structured'"
            :structure="draft"
            :problems="view.problems"
            :other-rules="view.otherRules"
            :filter="filter"
            @change="dirty = true"
          />
          <div v-else-if="mode === 'markdown'" class="field">
            <label for="c-raw">Markdown <span class="faint">(## article · ### section · #### rule, with a <code>&lt;!-- rule: id=… enforcement=… --&gt;</code> line)</span></label>
            <textarea id="c-raw" v-model="raw" class="textarea mono raw" spellcheck="false" data-raw @input="dirty = true" />
          </div>
          <div v-else class="history">
            <p v-if="history && history.reason" class="notice"><Info /> {{ history.reason }}</p>
            <div v-if="history?.commits.length" class="commits">
              <button v-for="c in history.commits" :key="c.oid" class="commit" :class="{ active: oldVersion?.oid === c.oid }" @click="showVersion(c.oid)">
                <span class="mono">{{ c.oid.slice(0, 8) }}</span> {{ c.summary }} <span class="faint small">{{ relativeTime(c.timestampMs) }}</span>
              </button>
            </div>
            <pre v-if="oldVersion" class="old selectable">{{ oldVersion.body }}</pre>
          </div>

          <footer v-if="mode === 'structured' || mode === 'markdown'" class="save-bar">
            <span class="faint small">{{ dirty ? 'Unsaved changes' : view.updated ? `Saved ${view.updated}` : '' }}</span>
            <button class="btn" :disabled="!dirty || busy" @click="loadView"><Undo2 /> Discard</button>
            <button class="btn btn-primary" :disabled="!dirty || busy" data-save @click="save"><Save /> {{ busy ? 'Saving…' : 'Save' }}</button>
          </footer>
        </section>
      </div>
    </template>

    <section v-else class="effective" data-effective>
      <div class="filters">
        <select v-model="effProject" class="select" aria-label="Project" data-eff-project>
          <option value="">Workspace (no project)</option>
          <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <select v-model="effTask" class="select" aria-label="Task">
          <option value="">No task</option>
          <option v-for="t in tasks" :key="t.id ?? t.path" :value="t.id">{{ t.title }}</option>
        </select>
        <label class="checkbox"><input v-model="effPending" type="checkbox" /><span>Preview unapproved changes</span></label>
        <button v-if="effProject && linkedGates.length" class="btn btn-sm" :disabled="verifying" data-verify-rules @click="verifyRules">{{ verifying ? 'Verifying…' : `Verify ${linkedGates.length} linked gate(s) now` }}</button>
      </div>
      <p class="notice small" data-compliance-note>
        <Info /> Applying is not guaranteeing: NEXUS gives Claude these rules as instructions; <strong>mandatory</strong> means no narrower scope can override them — not that Claude cannot break them. Only rules with a <em>Verified by gate</em> are checked, by NEXUS running that gate for the project. For hard limits, use Claude's own permission rules (organisation-managed) or the same gate in CI.
      </p>
      <template v-if="effective">
        <div v-for="c in effective.conflicts" :key="c.message" class="notice notice-warn" data-conflict><TriangleAlert /> {{ c.message }}</div>
        <p v-if="effective.pending.length" class="notice">
          <Info /> Not used until approved: {{ effective.pending.map((d) => d.title).join(', ') }}{{ effPending ? ' (shown below as a preview)' : '' }}.
        </p>
        <div v-if="effective.rules.length" class="eff-list">
          <div v-for="r in effective.rules" :key="r.id" class="eff-rule" :class="`e-${r.enforcement}`" :data-effective-rule="r.id">
            <div class="eff-head">
              <span class="mono rid">{{ r.id }}</span>
              <strong>{{ r.title }}</strong>
              <span class="badge" :class="{ 'badge-error': r.enforcement === 'mandatory', 'badge-live': r.enforcement === 'guideline' }">{{ ENFORCEMENT_LABELS[r.enforcement] }}</span>
              <span class="faint small">{{ SCOPE_LABELS[r.scope] }} · {{ r.docTitle }}<template v-if="r.article"> › {{ r.article }}</template></span>
              <span class="badge" :class="complianceTone(r.id)" :data-compliance="complianceOf(r.id)?.status ?? 'instructed'">{{ complianceLabel(r.id) }}</span>
            </div>
            <p class="eff-text">{{ r.text }}</p>
            <p v-if="r.replaces" class="faint small">Replaces <span class="mono">{{ r.replaces.id }}</span> “{{ r.replaces.title }}” from {{ r.replaces.docTitle }}</p>
          </div>
        </div>
        <div v-else class="empty"><h3>No rules apply here</h3></div>
        <details v-if="effective.overridden.length" class="overridden">
          <summary>{{ effective.overridden.length }} rule(s) overridden or disabled by a narrower scope</summary>
          <p v-for="o in effective.overridden" :key="o.rule.docId + o.rule.id" class="small">
            <span class="mono">{{ o.rule.id }}</span> ({{ o.rule.docTitle }}) {{ o.disabled ? 'disabled' : 'replaced' }} by <span class="mono">{{ o.by.id }}</span> ({{ o.by.docTitle }})
          </p>
        </details>
      </template>
    </section>

    <NewConstitutionDialog v-if="wsId" :open="creating" :workspace-id="wsId" @close="creating = false" @created="onCreated" />
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
.layout {
  display: grid;
  grid-template-columns: 290px 1fr;
  gap: 16px;
  min-height: 0;
}
.doc-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.search {
  display: flex;
  align-items: center;
  gap: 6px;
}
.search svg {
  width: 15px;
  height: 15px;
  color: var(--text-faint);
}
.group-label {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint);
  padding: 4px 2px;
}
.doc,
.hit {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  width: 100%;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: pointer;
  text-align: left;
  margin-bottom: 6px;
}
.doc.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.doc-title {
  font-weight: 600;
}
.small {
  font-size: 12px;
}
.rid {
  color: var(--accent-strong);
  font-size: 12px;
}
.detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.detail-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: flex-start;
}
.detail-head h2 {
  margin: 0 0 2px;
}
.applies {
  margin-top: 6px;
  align-items: center;
}
.segments {
  display: flex;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
  flex: none;
}
.segment {
  height: 28px;
  padding: 0 10px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  gap: 5px;
}
.segment svg {
  width: 14px;
  height: 14px;
}
.segment.selected {
  background: var(--accent-soft);
  color: var(--accent-strong);
}
.approve {
  align-items: center;
}
.raw {
  min-height: 420px;
  font-size: 12.5px;
}
.save-bar {
  position: sticky;
  bottom: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 10px 0;
  background: var(--bg);
  border-top: 1px solid var(--border);
}
.save-bar span {
  margin-right: auto;
}
.commits {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.commit {
  text-align: left;
  padding: 6px 10px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: pointer;
}
.commit.active {
  border-color: var(--accent);
}
.old {
  max-height: 420px;
  overflow: auto;
  padding: 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  font-size: 12px;
  white-space: pre-wrap;
}
.filters {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
.filters .select {
  width: auto;
  min-width: 200px;
}
.eff-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.eff-rule {
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  border-left: 3px solid var(--accent);
  background: var(--bg-panel);
}
.eff-rule.e-mandatory {
  border-left-color: var(--red);
}
.eff-rule.e-guideline {
  border-left-color: var(--cyan);
}
.eff-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.eff-text {
  margin: 6px 0 0;
  color: var(--text-muted);
  white-space: pre-wrap;
}
.overridden {
  margin-top: 12px;
}
</style>
