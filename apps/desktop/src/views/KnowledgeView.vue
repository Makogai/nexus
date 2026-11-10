<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { BookOpen, FileWarning, Globe, Info, Lightbulb, Lock, Plus, RefreshCw, Search, ShieldAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { KIND_LABELS, STATUS_LABELS, fileName, problems, rulesInOrder } from '../lib/knowledge'
import type { KnowledgeHit, KnowledgeItem, KnowledgeKind, KnowledgeOverview, MemoryApproval, MemoryProposal, TaskStatus } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import TaskBoard from '../components/TaskBoard.vue'
import KnowledgeDialog from '../components/KnowledgeDialog.vue'
import KnowledgeDocDialog from '../components/KnowledgeDocDialog.vue'
import ProposalCard from '../components/ProposalCard.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()

type Tab = KnowledgeKind | 'health' | 'proposals'
const route = useRoute()
/** `?tab=` opens a kind (e.g. `rule` from the "Needs you" inbox) or the proposals. */
const tabOf = (q: unknown): Tab | null => (q === 'proposals' || (typeof q === 'string' && q in KIND_LABELS) ? (q as Tab) : null)
const tab = ref<Tab>(tabOf(route.query.tab) ?? 'task')
const proposals = ref<MemoryProposal[]>([])
const proposalBusy = ref(false)
const overview = ref<KnowledgeOverview | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const creating = ref(false)
const viewing = ref<KnowledgeItem | null>(null)
const query = ref('')
const hits = ref<KnowledgeHit[] | null>(null)

const wsId = computed(() => workspaces.active?.id ?? null)
const items = computed(() => overview.value?.items ?? [])
const projectNames = computed(() => Object.fromEntries(projects.list.map((p) => [p.id, p.name])))
const broken = computed(() => problems(items.value))
const pending = computed(() => items.value.filter((i) => i.needsApproval))
const counts = computed(() => {
  const c: Record<string, number> = { health: broken.value.length, proposals: proposals.value.length }
  for (const k of Object.keys(KIND_LABELS)) c[k] = items.value.filter((i) => i.kind === k && i.valid && !i.conflict).length
  return c
})
const ofKind = computed(() =>
  tab.value === 'rule'
    ? rulesInOrder(items.value)
    : items.value.filter((i) => i.kind === tab.value && i.valid && !i.conflict),
)
const tabs = computed<Tab[]>(() => [...(Object.keys(KIND_LABELS) as KnowledgeKind[]), 'proposals', ...(broken.value.length ? (['health'] as Tab[]) : [])])

async function load(rebuild = false) {
  if (!wsId.value) return
  loading.value = true
  try {
    overview.value = await api.knowledgeOverview(wsId.value, rebuild)
    error.value = null
    await loadProposals(tab.value === 'proposals')
    if (rebuild) {
      const r = overview.value.report
      app.toast('success', `Index rebuilt from ${r.scanned} files${r.invalid ? ` (${r.invalid} invalid)` : ''}`)
    }
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    loading.value = false
  }
}

/** Proposals are scanned from recent sessions when the tab is opened or on Rescan (cheap, deterministic). */
async function loadProposals(extract: boolean) {
  if (!wsId.value) return
  proposals.value = await api.knowledgeProposals(wsId.value, extract).catch(() => proposals.value)
}

async function approveProposal(p: MemoryProposal, approval: MemoryApproval) {
  if (!wsId.value) return
  proposalBusy.value = true
  const item = await app.attempt(() => api.knowledgeProposalApprove(wsId.value!, p.id, approval), 'Could not approve')
  proposalBusy.value = false
  if (item) app.toast('success', `Saved as ${item.kind}: “${item.title}”`)
  await Promise.all([load(), loadProposals(false)])
}

async function rejectProposal(p: MemoryProposal) {
  proposalBusy.value = true
  await app.attempt(() => api.knowledgeProposalReject(p.id), 'Could not reject')
  proposalBusy.value = false
  await loadProposals(false)
}

watch(tab, (t) => t === 'proposals' && loadProposals(true))

// Opened from a notification (kept alive: react to later navigations too).
watch(
  () => [route.name, route.query.tab, route.query.task],
  async () => {
    if (route.name !== 'knowledge') return
    const asked = tabOf(route.query.tab)
    if (asked) tab.value = asked
    const taskId = typeof route.query.task === 'string' ? route.query.task : null
    if (taskId) {
      tab.value = 'task'
      if (!overview.value) await load()
      viewing.value = items.value.find((i) => i.id === taskId) ?? null
      if (!viewing.value) app.toast('info', 'That task no longer exists in this workspace.')
    }
  },
)

async function move(task: KnowledgeItem, status: TaskStatus) {
  if (!wsId.value || !task.id) return
  try {
    await api.knowledgeUpdateTask(wsId.value, task.id, { status, expectedHash: task.hash })
  } catch (e) {
    const err = toIpcError(e)
    app.toast('error', err.code === 'conflict' ? `“${task.title}” changed on disk — reloaded, try again` : `Could not move the task: ${err.message}`)
  }
  await load()
}

let searchTimer: ReturnType<typeof setTimeout> | undefined
watch(query, (q) => {
  clearTimeout(searchTimer)
  if (!q.trim()) {
    hits.value = null
    return
  }
  searchTimer = setTimeout(async () => {
    if (!wsId.value) return
    hits.value = await api.knowledgeSearch(wsId.value, q).catch(() => [])
  }, 220)
})

function launchTask(t: KnowledgeItem) {
  if (t.id) app.openLaunch(t.projectId, null, { id: t.id, title: t.title, projectId: t.projectId })
}

function openHit(h: KnowledgeHit) {
  viewing.value = items.value.find((i) => i.path === h.path) ?? { ...h, scope: 'workspace', needsApproval: false }
}

// Files are edited outside NEXUS too (editor, git pull): re-scan when the window regains focus and when the
// database changes (e.g. Claude created a task through MCP).
let active = false
const onFocus = () => active && load()
onMounted(() => {
  active = true
  window.addEventListener('focus', onFocus)
  load()
})
onBeforeUnmount(() => window.removeEventListener('focus', onFocus))
onActivated(() => {
  if (!active) {
    active = true
    load()
  }
})
onDeactivated(() => (active = false))
watch(wsId, () => {
  overview.value = null
  load()
})
watch(
  () => app.externalChanges,
  () => active && load(),
)
watch(broken, (b) => {
  if (!b.length && tab.value === 'health') tab.value = 'task'
})
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Knowledge</h1>
        <p>Rules, tasks, decisions, notes, specs and workflows of {{ workspaces.active?.name ?? 'this workspace' }} — Markdown files Claude reads on demand.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="loading" title="Re-scan the files" @click="load()"><RefreshCw :class="{ spin: loading }" /> Rescan</button>
        <button class="btn btn-ghost" :disabled="loading" title="Drop the index and rebuild it from the files" @click="load(true)">Rebuild index</button>
        <button class="btn btn-primary" :disabled="!wsId" @click="creating = true"><Plus /> New</button>
      </div>
    </div>

    <div v-if="!wsId" class="empty">
      <BookOpen class="empty-icon" />
      <h3>No active workspace</h3>
      <p>Knowledge lives in a workspace folder. Create or open a workspace first.</p>
    </div>
    <template v-else>
      <div v-if="error" class="notice notice-error selectable">{{ error }}</div>
      <div v-if="pending.length" class="notice notice-warn approvals">
        <ShieldAlert />
        <span>{{ pending.length }} always-on rule{{ pending.length > 1 ? 's' : '' }} changed or arrived from outside and {{ pending.length > 1 ? 'are' : 'is' }} not given to Claude until approved:</span>
        <button v-for="r in pending" :key="r.path" class="btn btn-sm" @click="viewing = r">Review “{{ r.title }}”</button>
      </div>

      <div class="toolbar">
        <div class="search">
          <Search class="ic" />
          <input v-model="query" class="input" type="search" placeholder="Search knowledge…" aria-label="Search knowledge" />
        </div>
      </div>

      <div v-if="hits" class="list">
        <button v-for="h in hits" :key="h.path" class="list-row hit" @click="openHit(h)">
          <span class="chip">{{ h.kind }}</span>
          <span class="hit-title">{{ h.title }}</span>
          <span class="faint snippet">{{ h.snippet }}</span>
        </button>
        <p v-if="!hits.length" class="faint">No matches.</p>
      </div>

      <template v-else>
        <div class="filters" role="tablist">
          <button
            v-for="t in tabs"
            :key="t"
            class="filter"
            role="tab"
            :aria-selected="tab === t"
            :class="{ active: tab === t, warn: t === 'health' }"
            :data-tab="t"
            @click="tab = t"
          >
            <FileWarning v-if="t === 'health'" class="ic" />
            <Lightbulb v-else-if="t === 'proposals'" class="ic" />
            {{ t === 'health' ? 'Problems' : t === 'proposals' ? 'Proposals' : KIND_LABELS[t] }} <span class="faint">{{ counts[t] }}</span>
          </button>
        </div>

        <TaskBoard v-if="tab === 'task'" :tasks="ofKind" :project-names="projectNames" @move="move" @open="viewing = $event" @launch="launchTask" />

        <div v-else-if="tab === 'proposals'" class="proposals" data-proposals>
          <p class="notice notice-accent explain-proposals">
            <Lightbulb />
            <span>Proposed from what Claude recorded as decisions, blockers and notes in past sessions. They are <strong>not knowledge</strong> until you approve them — edit, choose a scope, or reject.</span>
          </p>
          <ProposalCard v-for="p in proposals" :key="p.id" :proposal="p" :busy="proposalBusy" @approve="(a) => approveProposal(p, a)" @reject="rejectProposal(p)" />
          <p v-if="!proposals.length" class="faint">No proposals waiting for review.</p>
        </div>

        <div v-else-if="tab === 'health'" class="list">
          <div v-for="p in broken" :key="p.path" class="list-row problem">
            <FileWarning class="ic warn" />
            <div>
              <div class="mono selectable">{{ fileName(p.path) }}</div>
              <div class="faint selectable">{{ p.error ?? p.conflict }}</div>
            </div>
          </div>
          <p class="faint">Fix these files in an editor; NEXUS never guesses or rewrites a broken file. They are invisible to Claude until fixed.</p>
        </div>

        <div v-else-if="ofKind.length" class="list">
          <button v-for="i in ofKind" :key="i.path" class="list-row doc" :data-doc="i.id" @click="viewing = i">
            <span class="doc-title">{{ i.title }}</span>
            <span v-if="i.kind === 'rule'" class="chip">{{ i.scope }}{{ i.stacks.length ? `: ${i.stacks.join(', ')}` : '' }}</span>
            <span v-if="i.kind === 'rule'" class="badge" :class="i.applies === 'always' ? 'badge-accent' : ''">{{ i.applies ?? 'on-demand' }}</span>
            <span v-if="i.needsApproval" class="badge badge-warn">needs approval</span>
            <span v-if="i.status && i.kind !== 'rule'" class="badge">{{ STATUS_LABELS[i.status as TaskStatus] ?? i.status }}</span>
            <span v-if="i.projectId && projectNames[i.projectId]" class="chip">{{ projectNames[i.projectId] }}</span>
            <Globe v-if="i.scope === 'global'" class="ic" aria-label="Global" />
            <Lock v-else-if="i.privacy === 'local-only'" class="ic" aria-label="Local only" />
          </button>
        </div>
        <div v-else class="empty">
          <BookOpen class="empty-icon" />
          <h3>No {{ KIND_LABELS[tab as KnowledgeKind]?.toLowerCase() }} yet</h3>
          <p>Create one here, write a Markdown file in the workspace folder, or let Claude record tasks, notes and decisions through the NEXUS tools.</p>
        </div>
      </template>

      <p class="notice explain">
        <Info />
        <span>Claude gets only approved <em>always-on</em> rules in its system prompt (≤ 1,500 characters). Everything else stays out of the context until Claude looks it up with the NEXUS tools — the files in the workspace folder are the source of truth.</span>
      </p>
    </template>

    <KnowledgeDialog v-if="wsId" :open="creating" :workspace-id="wsId" :projects="projects.list" :initial-kind="tab === 'health' || tab === 'proposals' ? 'task' : tab" @close="creating = false" @created="load()" />
    <KnowledgeDocDialog v-if="wsId" :item="viewing" :workspace-id="wsId" @close="viewing = null" @approved="load()" @changed="load()" @launch="(t) => { viewing = null; launchTask(t) }" />
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  gap: 10px;
  margin-bottom: 12px;
}
.search {
  position: relative;
  flex: 1;
  max-width: 420px;
}
.search .ic {
  position: absolute;
  left: 10px;
  top: 50%;
  transform: translateY(-50%);
}
.search .input {
  padding-left: 32px;
}
.filters {
  display: flex;
  gap: 6px;
  margin-bottom: 14px;
  flex-wrap: wrap;
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
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.filter.active {
  background: var(--accent-soft);
  border-color: var(--accent-ring);
  color: var(--text);
}
.filter.warn {
  color: var(--warn, #d9a441);
}
.ic {
  width: 15px;
  height: 15px;
  color: var(--text-muted);
  flex: none;
}
.ic.warn {
  color: var(--warn, #d9a441);
}
.doc,
.hit {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  text-align: left;
  cursor: pointer;
  color: var(--text);
  font: inherit;
}
.doc-title,
.hit-title {
  font-weight: 550;
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.snippet {
  flex: 2;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.problem {
  display: flex;
  gap: 10px;
  align-items: flex-start;
}
.approvals {
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 12px;
}
.explain {
  margin-top: 20px;
}
.proposals {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
