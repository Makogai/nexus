<script setup lang="ts">
// Learn a project (W4): build its knowledge base in reviewed steps. NEXUS maps what it can read itself (no model,
// nothing executed); a guided Claude session drafts the overview and the architecture summary and records memories;
// the person decides what enters the knowledge base. Graphify is optional: the person runs its offline build in a
// terminal NEXUS prepares, and the tab watches for the report. Extra context (ADR-0045): notes for Claude go into the
// learning task; folders it may also read (e.g. the repository the project lives in) stay on this computer.
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { BookOpenCheck, Check, FileSearch, Folder, FolderPlus, GraduationCap, Lightbulb, Network, RefreshCw, Sparkles, X } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { relativeTime } from '../lib/format'
import { typeIntoTerminal } from '../lib/terminal'
import type { LearnDepth, LearnDraft, ProjectMap } from '../lib/types'
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
/** How much the learning session may spend (ADR-0041); follows the open learning task's depth. */
const depth = ref<LearnDepth>('standard')
const showRules = ref(false)
/** Notes for Claude, saved into the learning task when the session starts. */
const notes = ref('')
const notesTouched = ref(false)
const NOTES_MAX = 2000
/** After "Build", the tab watches for graphify-out/GRAPH_REPORT.md (cheap check) until this time. */
const graphWatchUntil = ref(0)
const checkingGraph = ref(false)
let graphTimer: number | null = null
let lastLoad = 0
const depthInfo = computed(() => map.value?.depths.find((d) => d.id === depth.value) ?? null)
const depthNames: Record<LearnDepth, string> = { quick: 'Quick', standard: 'Standard', deep: 'Deep' }

async function load() {
  const ws = workspaces.active
  if (!ws) return
  busy.value = true
  error.value = null
  lastLoad = Date.now()
  try {
    ;[map.value, drafts.value] = await Promise.all([api.projectLearnMap(ws.id, props.projectId), api.projectLearnDrafts(ws.id, props.projectId)])
    if (map.value.task) depth.value = map.value.task.depth
    if (!notesTouched.value) notes.value = map.value.task?.notes ?? ''
    open.value = drafts.value[0]?.id ?? null
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

/** The drafts are reviewed but the learning task is still open (e.g. reviewed before NEXUS closed it on its own). */
async function finishLearning() {
  const ws = workspaces.active
  const t = map.value?.task
  if (!ws || !t) return
  try {
    await api.knowledgeUpdateTask(ws.id, t.id, { status: 'done', log: 'Learning reviewed: marked done' })
    app.toast('success', 'Learning finished — the task is done')
    await load()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

async function startSession() {
  const ws = workspaces.active
  if (!ws) return
  try {
    const task = await api.projectLearnStart(ws.id, props.projectId, depth.value, notes.value)
    notesTouched.value = false
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
    graphWatchUntil.value = Date.now() + 30 * 60_000
    startGraphTimer()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

/** The tab is watching for a graph build the person runs in a terminal. */
const watchingGraph = ref(false)

function startGraphTimer() {
  stopGraphTimer()
  graphTimer = window.setInterval(() => void checkGraph(false), 4000)
  watchingGraph.value = true
}

function stopGraphTimer() {
  if (graphTimer !== null) window.clearInterval(graphTimer)
  graphTimer = null
  watchingGraph.value = false
}

/** Cheap check for a (re)built graph; a new report reloads the map. `manual`: the person asked, so say the result. */
async function checkGraph(manual: boolean) {
  const m = map.value
  if (!m || checkingGraph.value) return
  checkingGraph.value = true
  try {
    const g = await api.projectLearnGraph(props.projectId)
    if (g.built && g.builtMs !== m.graph.builtMs) {
      graphWatchUntil.value = 0
      stopGraphTimer()
      await load()
      app.toast('success', 'The graph is ready — the learning session reads it first.')
    } else {
      m.graph = g
      if (manual) app.toast('info', g.built ? 'The graph is up to date.' : 'No graph yet — Graphify writes graphify-out/GRAPH_REPORT.md when the build finishes.')
    }
  } catch (e) {
    if (manual) app.toast('error', toIpcError(e).message)
    stopGraphTimer()
  } finally {
    checkingGraph.value = false
    if (graphTimer !== null && Date.now() > graphWatchUntil.value) stopGraphTimer()
  }
}

/** Coming back to NEXUS (e.g. from another window): look for a graph built meanwhile. */
function onWindowFocus() {
  if (map.value && graphTimer === null) void checkGraph(false)
}

async function addFolder(path?: string) {
  let dir = path
  if (!dir) {
    const picked = await openDialog({ directory: true, title: 'A folder Claude may read while learning this project' })
    if (typeof picked !== 'string') return
    dir = picked
  }
  try {
    await api.projectLearnAddFolder(props.projectId, dir)
    await load()
  } catch (e) {
    app.toast('error', `Not added: ${toIpcError(e).message}`)
  }
}

async function removeFolder(path: string) {
  try {
    await api.projectLearnRemoveFolder(props.projectId, path)
    await load()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

const offerSurrounding = computed(() => {
  const m = map.value
  return m?.surrounding && !m.extraFolders.some((f) => f.path === m.surrounding!.path) ? m.surrounding : null
})

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

onMounted(() => {
  void load()
  window.addEventListener('focus', onWindowFocus)
})
// The project page is kept alive: coming back to it (e.g. from the terminal) reads the project again.
onActivated(() => {
  window.addEventListener('focus', onWindowFocus)
  if (Date.now() - lastLoad > 1500) void load()
  if (graphWatchUntil.value > Date.now()) startGraphTimer()
})
onDeactivated(() => {
  window.removeEventListener('focus', onWindowFocus)
  stopGraphTimer()
})
onBeforeUnmount(() => {
  window.removeEventListener('focus', onWindowFocus)
  stopGraphTimer()
})
watch(
  () => props.projectId,
  () => {
    notesTouched.value = false
    graphWatchUntil.value = 0
    stopGraphTimer()
    void load()
  },
)
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
            <p class="small" data-learn-graph-ready>Graph ready: <code>{{ map.graph.report }}</code><template v-if="map.graph.builtMs"> · built {{ relativeTime(map.graph.builtMs) }}</template>. The learning session reads it first.</p>
            <div class="row">
              <button class="btn btn-sm" data-learn-graph-build @click="buildGraph"><RefreshCw /> Rebuild in a terminal</button>
              <span v-if="watchingGraph" class="faint small" data-learn-graph-watching>Watching for the new report…</span>
            </div>
          </template>
          <template v-else-if="map.graph.installed">
            <p class="small">Graphify can turn the code into a queryable graph for Claude. NEXUS types <code>{{ map.graph.command }}</code> into a terminal in the project — code-only, it runs fully offline — and you press Enter. <code>graphify-out/</code> is kept out of Git in this clone.</p>
            <div class="row">
              <button class="btn btn-sm btn-primary" data-learn-graph-build @click="buildGraph"><Network /> Build the graph in a terminal</button>
              <button class="btn btn-sm btn-ghost" :disabled="checkingGraph" data-learn-graph-check @click="checkGraph(true)"><RefreshCw :class="{ spin: checkingGraph }" /> Check again</button>
            </div>
            <p v-if="watchingGraph" class="faint small" data-learn-graph-watching>Watching for <code>graphify-out/GRAPH_REPORT.md</code> — this ticks by itself when the build finishes.</p>
          </template>
          <template v-else>
            <p class="small faint" data-learn-graph-missing>Graphify is not installed on this computer — it is optional. To use it, install it yourself (<code>uv tool install graphifyy</code>, the PyPI package with a double y; see the Graphify manifest in the workspace's <code>integrations/</code>), then read again.</p>
          </template>
        </section>
      </div>

      <section class="card session" data-learn-session>
        <h2 class="card-title"><GraduationCap /> Learn with Claude</h2>
        <p class="small">
          Starts one Claude session on a task that carries this map. Claude follows NEXUS's <code>{{ map.skill }}</code> skill: it reads a few chosen files <strong>without
          changing them</strong>, drafts the overview and the architecture summary, and records durable memories (notes are marked unverified, decisions start as proposals).
          No subagents. You decide below what enters the knowledge base. It runs on your Claude plan.
        </p>
        <div class="field depth">
          <label>Depth</label>
          <div class="segments" role="radiogroup" aria-label="Depth" data-learn-depth>
            <button
              v-for="d in map.depths"
              :key="d.id"
              class="segment"
              role="radio"
              :data-depth="d.id"
              :aria-checked="depth === d.id"
              :class="{ selected: depth === d.id }"
              @click="depth = d.id"
            >
              {{ depthNames[d.id] }}
            </button>
          </div>
          <span v-if="depthInfo" class="hint" data-learn-budget>
            {{ depthInfo.files ? `Reads at most ${depthInfo.files} files` : 'No file limit (still selective)' }} · overview ≤ {{ depthInfo.overviewWords }} words ·
            architecture ≤ {{ depthInfo.architectureWords }} words · {{ depthInfo.memories[0] }}–{{ depthInfo.memories[1] }} memories.
            <button class="link" data-learn-rules-toggle @click="showRules = !showRules">{{ showRules ? 'Hide' : 'Show' }} the rules Claude gets</button>
          </span>
          <pre v-if="showRules && depthInfo" class="rules" data-learn-rules>{{ depthInfo.rules }}</pre>
        </div>
        <div class="field">
          <label for="learn-notes">Notes for Claude <span class="faint">optional</span></label>
          <textarea
            id="learn-notes"
            v-model="notes"
            class="input"
            rows="3"
            :maxlength="NOTES_MAX"
            data-learn-notes
            placeholder="What the code does not say — e.g. “drivebay is one app of my maiw monorepo; shared code lives in maiw/packages; billing is being rewritten.”"
            @input="notesTouched = true"
          />
          <span class="hint">Saved in the learning task when the session starts — Claude reads them as part of the task.</span>
        </div>
        <div class="field" data-learn-folders>
          <label>Also let Claude read <span class="faint">optional · on this computer only</span></label>
          <div v-if="offerSurrounding" class="suggest" data-learn-surrounding>
            <span class="small"><strong>{{ map.name }}</strong> lives inside the Git repository <strong>{{ offerSurrounding.name }}</strong> <code>{{ offerSurrounding.path }}</code> — its README, docs and shared code may explain the project.</span>
            <button class="btn btn-sm" data-learn-add-surrounding @click="addFolder(offerSurrounding.path)"><FolderPlus /> Let Claude read {{ offerSurrounding.name }} too</button>
          </div>
          <ul v-if="map.extraFolders.length" class="extra-folders">
            <li v-for="f in map.extraFolders" :key="f.path" :data-learn-folder="f.name">
              <Folder class="ic" />
              <strong>{{ f.name }}</strong>
              <code class="faint path">{{ f.path }}</code>
              <span v-if="!f.available" class="badge badge-warn">missing — skipped</span>
              <button class="btn btn-sm btn-ghost btn-icon" :aria-label="`Stop letting Claude read ${f.name}`" :title="`Remove ${f.name}`" @click="removeFolder(f.path)"><X /></button>
            </li>
          </ul>
          <div>
            <button class="btn btn-sm btn-ghost" data-learn-add-folder @click="addFolder()"><FolderPlus /> Add a folder…</button>
          </div>
          <span class="hint">Read-only context for the learning session only (rule LEARN-8, within the same budget). Credential folders, Claude's configuration, your home folder and whole drives are refused.</span>
        </div>
        <div class="row">
          <button class="btn btn-primary" data-learn-start @click="startSession"><Sparkles /> {{ map.task ? 'Continue the learning session' : 'Start a learning session' }}</button>
          <button v-if="map.task && !map.drafts && !map.context.overviewGenerated && map.context.architectureSummary" class="btn" data-learn-finish @click="finishLearning"><Check /> Finish learning</button>
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
.depth {
  margin: 4px 0 10px;
}
.link {
  border: none;
  background: none;
  color: var(--accent-strong);
  cursor: pointer;
  padding: 0;
  margin-left: 4px;
  font: inherit;
  text-decoration: underline;
}
.suggest {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--accent-soft);
}
.suggest .small {
  flex: 1;
  min-width: 240px;
  overflow-wrap: anywhere;
}
.extra-folders {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.extra-folders li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  min-width: 0;
}
.extra-folders .ic {
  width: 14px;
  height: 14px;
  color: var(--text-faint);
  flex-shrink: 0;
}
.extra-folders .path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.rules {
  margin: 6px 0 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
  white-space: pre-wrap;
}
@media (max-width: 1100px) {
  .grid {
    grid-template-columns: 1fr;
  }
}
</style>
