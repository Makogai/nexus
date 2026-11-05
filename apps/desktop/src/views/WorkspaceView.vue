<script setup lang="ts">
// Workspace (Portable Workspace P3): the Workspace Doctor — health of the portable workspace with recommendations and
// reviewable repairs — and its starter template (install, update; per-file plan first, ADR-0037).
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { CircleAlert, CircleCheck, Info, LayoutTemplate, RefreshCw, Stethoscope, TriangleAlert, Wrench } from '@lucide/vue'
import BaseModal from '../components/BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import type { DoctorFinding, DoctorReport, RepairPlan, TemplatePlan, TemplateSummary } from '../lib/types'
import { relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const router = useRouter()
const ws = computed(() => workspaces.active)
const report = ref<DoctorReport | null>(null)
const templates = ref<TemplateSummary[]>([])
const busy = ref(false)
const repairPlan = ref<RepairPlan | null>(null)
const templatePlan = ref<TemplatePlan | null>(null)

const ICON = { ok: CircleCheck, info: Info, warn: TriangleAlert, error: CircleAlert } as const
const VIEWS: Record<string, string> = { constitutions: 'constitutions', workflows: 'workflows', kits: 'kits', projects: 'projects', sync: 'sync', knowledge: 'knowledge', workspace: 'workspace', 'claude-setup': 'profiles' }
const problems = computed(() => (report.value?.findings ?? []).filter((f) => f.status === 'warn' || f.status === 'error'))
const notes = computed(() => (report.value?.findings ?? []).filter((f) => f.status === 'info'))
const healthy = computed(() => (report.value?.findings ?? []).filter((f) => f.status === 'ok'))
const current = computed(() => ws.value?.template ?? null)

async function load() {
  if (!ws.value) return
  busy.value = true
  try {
    ;[report.value, templates.value] = await Promise.all([api.workspaceDoctor(ws.value.id), api.templateList()])
  } catch (e) {
    app.toast('error', `Workspace Doctor failed: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
onMounted(load)
watch(() => ws.value?.id, load)

function go(f: DoctorFinding) {
  const name = f.navigate ? VIEWS[f.navigate] : null
  if (name && name !== 'workspace') router.push({ name })
}

async function reviewRepair(id: string) {
  if (!ws.value) return
  try {
    repairPlan.value = await api.workspaceRepairPlan(ws.value.id, id)
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

async function applyRepair() {
  if (!ws.value || !repairPlan.value) return
  busy.value = true
  try {
    const done = await api.workspaceRepair(ws.value.id, repairPlan.value.id)
    repairPlan.value = null
    app.toast('success', done.message, 6000)
    await workspaces.load()
    await load()
  } catch (e) {
    app.toast('error', `Repair failed: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}

async function reviewTemplate(id: string) {
  if (!ws.value) return
  try {
    templatePlan.value = await api.templatePlan(ws.value.id, id)
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

const planCounts = computed(() => {
  const c: Record<string, number> = {}
  for (const i of templatePlan.value?.items ?? []) c[i.action] = (c[i.action] ?? 0) + 1
  return c
})
const ACTION_LABEL: Record<string, string> = {
  add: 'new files',
  update: 'updated files',
  'keep-customized': 'files you changed (kept; new version saved to compare)',
  'keep-removed': 'files you removed (not added again)',
  'skip-existing': 'files of your own at a template path (never overwritten)',
  retire: 'files no longer in the template (kept)',
  unchanged: 'unchanged files',
}

async function applyTemplate() {
  if (!ws.value || !templatePlan.value) return
  busy.value = true
  try {
    const done = await api.templateApply(ws.value.id, templatePlan.value.template.id)
    templatePlan.value = null
    app.toast('success', `${done.plan.template.id} ${done.plan.template.version}: ${done.written.length} file(s) written${done.savedForReview.length ? `, ${done.savedForReview.length} saved to compare` : ''}`, 6000)
    await workspaces.load()
    await load()
  } catch (e) {
    app.toast('error', `Template failed — nothing was changed: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="page workspace-page" data-workspace-view>
    <div class="page-header">
      <div>
        <h1>Workspace</h1>
        <p>The health of this portable workspace — what travels with it, what this computer still needs — and its starter template. Repairs show what they change before anything is written.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="busy" data-doctor-refresh @click="load"><RefreshCw :class="{ spin: busy }" /> Check again</button>
      </div>
    </div>

    <div v-if="!ws" class="empty"><Stethoscope class="empty-icon" /><h3>No workspace yet</h3></div>
    <template v-else>
      <section class="card summary" data-doctor-summary>
        <div class="summary-row">
          <strong>{{ ws.name }}</strong>
          <span class="mono faint">{{ ws.rootPath }}</span>
          <span v-if="ws.compat" class="badge" :class="ws.compat.writable ? 'badge-ok' : 'badge-warn'">schema {{ ws.compat.schemaVersion }}{{ ws.compat.writable ? '' : ' · read-only' }}</span>
          <span v-if="report" class="faint">checked {{ relativeTime(report.checkedAt) }}</span>
        </div>
        <div v-if="report" class="counts" data-doctor-counts>
          <span class="badge badge-error" v-if="report.counts.error">{{ report.counts.error }} error{{ report.counts.error === 1 ? '' : 's' }}</span>
          <span class="badge badge-warn" v-if="report.counts.warn">{{ report.counts.warn }} warning{{ report.counts.warn === 1 ? '' : 's' }}</span>
          <span class="badge" v-if="report.counts.info">{{ report.counts.info }} note{{ report.counts.info === 1 ? '' : 's' }}</span>
          <span class="badge badge-ok">{{ report.counts.ok }} ok</span>
        </div>
      </section>

      <section v-if="report" class="card" data-doctor-findings>
        <h2 class="card-title"><Stethoscope /> Workspace Doctor</h2>
        <p v-if="!problems.length" class="faint" data-doctor-clean>Nothing needs attention.</p>
        <ul class="findings">
          <li v-for="f in [...problems, ...notes]" :key="f.check + f.summary" :class="`finding ${f.status}`" :data-finding="f.check">
            <component :is="ICON[f.status]" class="ic" />
            <div class="body">
              <div class="summary-line">{{ f.summary }}</div>
              <ul v-if="f.detail.length" class="detail"><li v-for="d in f.detail" :key="d" class="mono">{{ d }}</li></ul>
              <div v-if="f.recommendation" class="rec">{{ f.recommendation }}</div>
            </div>
            <div class="actions">
              <button v-if="f.repair" class="btn btn-sm" :data-repair="f.repair.id" @click="reviewRepair(f.repair.id)"><Wrench /> {{ f.repair.title }}</button>
              <button v-if="f.navigate && f.navigate !== 'workspace' && VIEWS[f.navigate]" class="btn btn-sm btn-ghost" @click="go(f)">Open</button>
            </div>
          </li>
        </ul>
        <details v-if="healthy.length" class="healthy">
          <summary>{{ healthy.length }} check{{ healthy.length === 1 ? '' : 's' }} passed</summary>
          <ul><li v-for="f in healthy" :key="f.check + f.summary" :data-finding-ok="f.check"><CircleCheck class="ic ok" /> {{ f.summary }}</li></ul>
        </details>
      </section>

      <section class="card" data-template-card>
        <h2 class="card-title"><LayoutTemplate /> Starter template</h2>
        <p class="faint">
          <template v-if="current">This workspace uses <strong>{{ current.id }} {{ current.version }}</strong>.</template>
          <template v-else>This workspace has no starter template.</template>
          Templates add constitutions, skills and workflows. Only the small core agreement is always in Claude's context; everything else is read when relevant, and stack rules apply only to projects that use the stack.
        </p>
        <div class="templates">
          <div v-for="t in templates" :key="t.id" class="template" :class="{ current: current?.id === t.id }" :data-template-row="t.id">
            <div>
              <strong>{{ t.title }}</strong> <span class="faint">{{ t.version }}</span>
              <p class="faint">{{ t.summary }}</p>
              <p class="counts">{{ t.constitutions }} constitutions · {{ t.stackRuleSets }} stack rule sets · {{ t.skills }} skills · {{ t.workflows }} workflows · {{ t.integrations }} integrations · always on: {{ t.alwaysOnChars }} characters</p>
            </div>
            <button class="btn btn-sm" :disabled="busy || !ws.compat?.writable" :data-template-review="t.id" @click="reviewTemplate(t.id)">{{ current?.id === t.id ? 'Check for updates' : current ? 'Switch…' : 'Install…' }}</button>
          </div>
        </div>
      </section>
    </template>

    <BaseModal :open="!!repairPlan" :title="repairPlan?.title ?? ''" subtitle="What this repair changes" @close="repairPlan = null">
      <template v-if="repairPlan">
        <ul class="plan" data-repair-changes><li v-for="c in repairPlan.changes" :key="c">{{ c }}</li></ul>
        <ul v-if="repairPlan.notes.length" class="plan notes"><li v-for="n in repairPlan.notes" :key="n">{{ n }}</li></ul>
      </template>
      <template #footer>
        <button class="btn btn-ghost" @click="repairPlan = null">Cancel</button>
        <button class="btn btn-primary" :disabled="busy" data-repair-apply @click="applyRepair">Apply</button>
      </template>
    </BaseModal>

    <BaseModal :open="!!templatePlan" :title="templatePlan ? `${templatePlan.template.id} ${templatePlan.template.version}` : ''" subtitle="What applying the template changes" :width="620" @close="templatePlan = null">
      <template v-if="templatePlan">
        <ul class="plan" data-template-plan>
          <li v-for="(n, action) in planCounts" :key="action"><strong>{{ n }}</strong> {{ ACTION_LABEL[action] ?? action }}</li>
        </ul>
        <ul class="plan notes"><li v-for="n in templatePlan.notes" :key="n">{{ n }}</li></ul>
      </template>
      <template #footer>
        <button class="btn btn-ghost" @click="templatePlan = null">Cancel</button>
        <button class="btn btn-primary" :disabled="busy || !(planCounts.add || planCounts.update || planCounts['keep-customized'] || templatePlan?.installed?.version !== templatePlan?.template.version)" data-template-apply @click="applyTemplate">Apply</button>
      </template>
    </BaseModal>
  </div>
</template>

<style scoped>
.summary-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.counts {
  margin-top: 8px;
  display: flex;
  gap: 6px;
  font-size: 12px;
}
.findings {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.finding {
  display: grid;
  grid-template-columns: 18px 1fr auto;
  gap: 10px;
  align-items: start;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-input);
}
.finding.error .ic {
  color: var(--red);
}
.finding.warn .ic {
  color: var(--amber);
}
.finding.info .ic {
  color: var(--blue);
}
.ic {
  width: 16px;
  height: 16px;
  margin-top: 2px;
}
.ic.ok {
  color: var(--green);
}
.summary-line {
  font-weight: 500;
}
.detail {
  margin: 4px 0 0;
  padding-left: 16px;
  font-size: 12px;
  color: var(--text-muted);
}
.rec {
  margin-top: 4px;
  font-size: 12.5px;
  color: var(--text-muted);
}
.actions {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  justify-content: flex-end;
}
.healthy {
  margin-top: 10px;
  font-size: 13px;
  color: var(--text-muted);
}
.healthy ul {
  list-style: none;
  padding: 0;
  margin: 6px 0 0;
}
.healthy li {
  display: flex;
  gap: 6px;
  align-items: center;
  padding: 2px 0;
}
.templates {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 8px;
}
.template {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.template.current {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.template p {
  margin: 4px 0 0;
  font-size: 12.5px;
}
.template .counts {
  font-size: 11.5px;
  color: var(--text-faint);
}
.plan {
  margin: 6px 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.6;
}
.plan.notes {
  color: var(--text-muted);
}
</style>
