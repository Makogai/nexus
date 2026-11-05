<script setup lang="ts">
// Learn a project (W4): build its knowledge base in reviewed steps. NEXUS maps what it can read itself (no model,
// nothing executed); a guided Claude session drafts the overview and the architecture summary and records memories;
// the person decides what enters the knowledge base. Graphify is optional: the person runs its offline build in a
// terminal NEXUS prepares.
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { BookOpenCheck, Check, FileSearch, GraduationCap, Lightbulb, Network, RefreshCw, Sparkles, X } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { relativeTime } from '../lib/format'
import { typeIntoTerminal } from '../lib/terminal'
import type { LearnDraft, ProjectMap } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useWorkspaceStore } from '../stores/workspaces'
import MarkdownView from './MarkdownView.vue'

const props = defineProps<{ projectId: string }>()
const app = useAppStore()
const workspaces = useWorkspaceStore()
const router = useRouter()

const map = ref<ProjectMap | null>(null)
const drafts = ref<LearnDraft[]>([])
const open = ref<string | null>(null)
const busy = ref(false)
const error = ref<string | null>(null)

async function load() {
  const ws = workspaces.active
  if (!ws) return
  busy.value = true
  error.value = null
  try {
    ;[map.value, drafts.value] = await Promise.all([api.projectLearnMap(ws.id, props.projectId), api.projectLearnDrafts(ws.id, props.projectId)])
    open.value = drafts.value[0]?.id ?? null
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

async function startSession() {
  const ws = workspaces.active
  if (!ws) return
  try {
    const task = await api.projectLearnStart(ws.id, props.projectId)
    app.openLaunch(props.projectId, null, { id: task.id, title: task.title, projectId: props.projectId }, { kickoff: true })
    await load()
  } catch (e) {
    app.toast('error', `Could not prepare the learning session: ${toIpcError(e).message}`)
  }
}

async function buildGraph() {
  const m = map.value
  if (!m) return
  try {
    const added = await api.projectLearnExcludeGraph(props.projectId)
    if (added) app.toast('info', 'graphify-out/ is now ignored in this clone (.git/info/exclude — not committed).')
    await typeIntoTerminal(m.graph.command, { projectId: props.projectId })
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

async function decide(d: LearnDraft, apply: boolean) {
  const ws = workspaces.active
  if (!ws || !map.value) return
  if (apply && d.topic === 'overview' && !map.value.context.overviewGenerated) {
    const ok = await app.confirm({ title: 'Replace the overview?', message: 'The project overview already has text you wrote or accepted. This draft replaces it (the draft stays as a note).', confirmLabel: 'Replace' })
    if (!ok) return
  }
  try {
    await api.projectLearnDecide(ws.id, props.projectId, d.id, apply)
    app.toast('success', apply ? (d.topic === 'overview' ? 'The draft is now the project overview' : 'The draft is now the architecture summary') : 'Draft set aside')
    await load()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

const languages = computed(() => {
  const m = map.value
  if (!m?.languages.length) return []
  const total = m.languages.reduce((n, l) => n + l.files, 0) || 1
  return m.languages.map((l) => ({ ...l, share: Math.max(2, Math.round((l.files / total) * 100)) }))
})
const steps = computed(() => {
  const m = map.value
  if (!m) return []
  return [
    { done: true, text: 'NEXUS read the repository' },
    { done: m.graph.built, text: 'Knowledge graph (optional)', optional: true },
    { done: !!m.task || m.drafts > 0 || m.context.architectureSummary, text: 'Learning session with Claude' },
    { done: !m.context.overviewGenerated && m.context.architectureSummary, text: 'You reviewed the overview and architecture' },
  ]
})

onMounted(load)
watch(() => props.projectId, load)
</script>

<template>
  <div class="learn" data-project-learn>
    <div v-if="error" class="notice notice-error selectable">{{ error }}</div>
    <template v-if="map">
      <ol class="steps" aria-label="Progress">
        <li v-for="(s, i) in steps" :key="i" :class="{ done: s.done }"><span class="dot"><Check v-if="s.done" /></span>{{ s.text }}<span v-if="s.optional" class="faint"> · optional</span></li>
      </ol>

      <div class="grid">
        <section class="card" data-learn-map>
          <h2 class="card-title"><FileSearch /> What NEXUS read <span class="faint small">no model, nothing run</span></h2>
          <div class="facts">
            <div><strong>{{ map.files.toLocaleString() }}{{ map.filesTruncated ? '+' : '' }}</strong><span>tracked files</span></div>
            <div v-if="map.history"><strong>{{ map.history.commits.toLocaleString() }}</strong><span>commits · {{ map.history.contributors }} author{{ map.history.contributors === 1 ? '' : 's' }}</span></div>
            <div v-if="map.history?.lastMs"><strong>{{ relativeTime(map.history.lastMs) }}</strong><span>last commit</span></div>
          </div>
          <div v-if="languages.length" class="langs" aria-label="Languages">
            <div class="bar"><span v-for="l in languages" :key="l.name" :style="{ width: `${l.share}%` }" :title="`${l.name}: ${l.files} files`" /></div>
            <span v-for="l in languages" :key="l.name" class="lang">{{ l.name }} <span class="faint">{{ l.files }}</span></span>
          </div>
          <dl class="kv">
            <template v-if="map.folders.length">
              <dt>Folders</dt>
              <dd class="chips"><span v-for="f in map.folders" :key="f.name" class="chip mono">{{ f.name }}/ <span class="faint">{{ f.files }}</span></span></dd>
            </template>
            <template v-if="map.stacks.length || map.packageManagers.length">
              <dt>Stack</dt>
              <dd class="chips"><span v-for="s in [...map.stacks, ...map.packageManagers]" :key="s" class="chip">{{ s }}</span></dd>
            </template>
            <template v-if="map.scripts.length">
              <dt>Commands</dt>
              <dd class="mono small">{{ map.scripts.map(([k, c]) => `${k}: ${c}`).join(' · ') }}</dd>
            </template>
            <dt>Docs</dt>
            <dd class="mono small">{{ map.docs.length ? map.docs.slice(0, 12).join(' · ') + (map.docs.length > 12 ? ` · +${map.docs.length - 12}` : '') : 'none found' }}</dd>
            <dt>Knowledge</dt>
            <dd class="small">
              overview {{ map.context.overviewGenerated ? 'generated from the README' : 'written' }} · architecture summary {{ map.context.architectureSummary ? 'written' : 'missing' }} ·
              {{ map.context.notes }} notes · {{ map.context.decisions }} decisions · {{ map.context.openTasks }} open tasks
            </dd>
          </dl>
          <button class="btn btn-sm btn-ghost" :disabled="busy" @click="load"><RefreshCw :class="{ spin: busy }" /> Read again</button>
        </section>

        <section class="card" data-learn-graph>
          <h2 class="card-title"><Network /> Knowledge graph <span class="faint small">Graphify · optional</span></h2>
          <template v-if="map.graph.built">
            <p class="small">Graph ready: <code>{{ map.graph.report }}</code><template v-if="map.graph.builtMs"> · built {{ relativeTime(map.graph.builtMs) }}</template>. The learning session reads it first.</p>
            <button class="btn btn-sm" data-learn-graph-build @click="buildGraph"><RefreshCw /> Rebuild in a terminal</button>
          </template>
          <template v-else-if="map.graph.installed">
            <p class="small">Graphify can turn the code into a queryable graph for Claude. NEXUS types <code>{{ map.graph.command }}</code> into a terminal in the project — code-only, it runs fully offline — and you press Enter. <code>graphify-out/</code> is kept out of Git in this clone.</p>
            <button class="btn btn-sm btn-primary" data-learn-graph-build @click="buildGraph"><Network /> Build the graph in a terminal</button>
          </template>
          <template v-else>
            <p class="small faint" data-learn-graph-missing>Graphify is not installed on this computer — it is optional. To use it, install it yourself (<code>uv tool install graphifyy</code>, the PyPI package with a double y; see the Graphify manifest in the workspace's <code>integrations/</code>), then read again.</p>
          </template>
        </section>
      </div>

      <section class="card session" data-learn-session>
        <h2 class="card-title"><GraduationCap /> Learn with Claude</h2>
        <p class="small">
          Starts a Claude session on a task that carries this map. Claude reads the code and documents <strong>without changing them</strong>, drafts the overview and the
          architecture summary, and records durable memories (notes are marked unverified, decisions start as proposals). You decide below what enters the knowledge base. It
          runs on your Claude plan.
        </p>
        <div class="row">
          <button class="btn btn-primary" data-learn-start @click="startSession"><Sparkles /> {{ map.task ? 'Continue the learning session' : 'Start a learning session' }}</button>
          <span v-if="map.task" class="faint small">Task: {{ map.task.title }} · {{ map.task.status }}</span>
          <span class="spacer" />
          <button class="btn btn-sm btn-ghost" @click="router.push({ name: 'knowledge', query: { tab: 'proposals' } })"><Lightbulb /> Memory proposals</button>
        </div>
      </section>

      <section class="drafts" data-learn-drafts>
        <h2 class="section-title"><BookOpenCheck class="title-ic" /> Drafts to review <span class="count">{{ drafts.length }}</span></h2>
        <p v-if="!drafts.length" class="faint small">No drafts waiting. Drafts appear here when Claude proposes an overview or an architecture summary.</p>
        <article v-for="d in drafts" :key="d.id" class="draft card" :data-draft="d.topic">
          <header class="draft-head" @click="open = open === d.id ? null : d.id">
            <span class="chip">{{ d.topic === 'overview' ? 'Overview' : 'Architecture' }}</span>
            <strong>{{ d.title }}</strong>
            <span class="faint small">{{ d.updated?.slice(0, 16).replace('T', ' ') }} · by Claude, unverified</span>
          </header>
          <MarkdownView v-if="open === d.id" class="draft-body" :source="d.body" />
          <div class="row">
            <button class="btn btn-sm btn-primary" data-draft-apply @click="decide(d, true)"><Check /> {{ d.topic === 'overview' ? 'Use as overview' : 'Use as architecture summary' }}</button>
            <button class="btn btn-sm btn-ghost" data-draft-discard @click="decide(d, false)"><X /> Set aside</button>
          </div>
        </article>
      </section>
    </template>
    <p v-else-if="busy" class="faint">Reading the project…</p>
  </div>
</template>

<style scoped>
.learn {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.steps {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 18px;
  margin: 0;
  padding: 0;
  list-style: none;
  font-size: 13px;
  color: var(--text-muted);
}
.steps li {
  display: flex;
  align-items: center;
  gap: 7px;
}
.steps li.done {
  color: var(--text);
}
.dot {
  width: 18px;
  height: 18px;
  border-radius: 50%;
  border: 1px solid var(--border-strong);
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.done .dot {
  background: var(--accent-soft);
  border-color: var(--accent);
  color: var(--accent-strong);
}
.dot svg {
  width: 11px;
  height: 11px;
}
.grid {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr);
  gap: 16px;
}
.card-title .faint {
  font-weight: 400;
  margin-left: 6px;
}
.facts {
  display: flex;
  gap: 26px;
  margin: 4px 0 12px;
}
.facts div {
  display: flex;
  flex-direction: column;
}
.facts strong {
  font-size: 18px;
}
.facts span {
  font-size: 12px;
  color: var(--text-faint);
}
.langs {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 12px;
  margin-bottom: 12px;
  font-size: 12px;
}
.bar {
  display: flex;
  width: 100%;
  height: 6px;
  border-radius: 3px;
  overflow: hidden;
  background: var(--bg-input);
  gap: 2px;
}
.bar span {
  background: var(--accent);
  opacity: 0.75;
}
.bar span:nth-child(2n) {
  background: var(--cyan);
}
.bar span:nth-child(3n) {
  background: var(--amber);
}
.kv {
  display: grid;
  grid-template-columns: 90px 1fr;
  gap: 8px 12px;
  margin: 0 0 10px;
  font-size: 13px;
}
.kv dt {
  color: var(--text-faint);
}
.kv dd {
  margin: 0;
  min-width: 0;
  overflow-wrap: anywhere;
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}
.chip .faint {
  margin-left: 4px;
}
.small {
  font-size: 12.5px;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.spacer {
  flex: 1;
}
.drafts {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.draft {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.draft-head {
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
}
.draft-body {
  border-top: 1px solid var(--border);
  padding-top: 10px;
  max-height: 46vh;
  overflow: auto;
}
.title-ic {
  width: 16px;
  height: 16px;
}
@media (max-width: 1100px) {
  .grid {
    grid-template-columns: 1fr;
  }
}
</style>
