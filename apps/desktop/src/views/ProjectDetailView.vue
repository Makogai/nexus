<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { ArrowLeft, BookMarked, FileText, FolderOpen, Gauge, GitBranch, GitCommitHorizontal, History, Link, ListTodo, NotebookPen, RefreshCw, Sparkles, SquareTerminal, Trash2, TriangleAlert, Workflow } from '@lucide/vue'
import { relativeTime } from '../lib/format'
import { api, toIpcError } from '../lib/ipc'
import { useWorkspaceStore } from '../stores/workspaces'
import { STATUS_LABELS } from '../lib/knowledge'
import type { Checkpoint, ContextInitialized, KnowledgeItem, ProjectKnowledge, TaskStatus } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import GitSummary from '../components/GitSummary.vue'
import UsageSummary from '../components/UsageSummary.vue'
import SessionRow from '../components/SessionRow.vue'
import InstructionsPanel from '../components/InstructionsPanel.vue'

const props = defineProps<{ id: string }>()
const app = useAppStore()
const projects = useProjectStore()
const sessions = useSessionStore()
const router = useRouter()
const refreshing = ref(false)

const project = computed(() => projects.byId[props.id] ?? null)
const workspaces = useWorkspaceStore()
const contextResult = ref<ContextInitialized | null>(null)
const contextBusy = ref(false)
/** Create or refresh the project's portable context in the workspace (Portable Workspace P4). */
async function initContext() {
  const ws = workspaces.active
  if (!ws) return
  contextBusy.value = true
  try {
    contextResult.value = await api.projectContextInit(ws.id, props.id)
  } catch (e) {
    app.toast('error', `Could not write the project context: ${toIpcError(e).message}`)
  } finally {
    contextBusy.value = false
  }
}
const git = computed(() => projects.git[props.id])
const projectSessions = computed(() => sessions.list.filter((s) => s.projectId === props.id && !s.archivedAt))
const checkpoints = ref<Checkpoint[]>([])
const knowledge = ref<ProjectKnowledge | null>(null)

async function loadKnowledge() {
  if (!project.value?.available) return
  knowledge.value = await api.projectKnowledge(props.id).catch(() => knowledge.value)
}

function launchTask(t: KnowledgeItem) {
  if (t.id) app.openLaunch(props.id, null, { id: t.id, title: t.title, projectId: t.projectId })
}

async function loadCheckpoints() {
  checkpoints.value = await api.projectCheckpoints(props.id, 20).catch(() => checkpoints.value)
}

function loadGit(force = false) {
  if (project.value?.available) projects.gitStatus(props.id, force)
}
watch(
  () => props.id,
  () => {
    loadGit()
    loadCheckpoints()
    loadKnowledge()
  },
  { immediate: true },
)
watch(
  () => app.externalChanges,
  () => {
    loadCheckpoints()
    loadKnowledge()
  },
)
onActivated(() => {
  loadGit()
  loadCheckpoints()
  loadKnowledge()
})

async function refresh() {
  refreshing.value = true
  await app.attempt(() => projects.refresh(props.id), 'Could not refresh')
  refreshing.value = false
}

async function openShell() {
  await app.attempt(() => sessions.openShell({ projectId: props.id, shellId: app.prefs.defaultShell }), 'Could not open a shell')
}

async function reveal() {
  if (project.value?.localPath) await app.attempt(() => revealItemInDir(project.value!.localPath!), 'Could not open the folder')
}

async function unregister() {
  if (!project.value) return
  const ok = await app.confirm({
    title: `Remove “${project.value.name}” from the workspace?`,
    message: 'The project is unregistered from NEXUS and the workspace manifest. Its folder and Git repository are not touched.',
    confirmLabel: 'Remove from workspace',
    danger: true,
  })
  if (!ok) return
  await app.attempt(async () => {
    await projects.unregister(props.id)
    router.push('/projects')
  }, 'Could not remove the project')
}
</script>

<template>
  <div v-if="project" class="page">
    <button class="btn btn-ghost btn-sm back" @click="router.push('/projects')"><ArrowLeft /> Projects</button>
    <div class="page-header">
      <div class="title-block">
        <div class="avatar">{{ project.name.slice(0, 1).toUpperCase() }}</div>
        <div>
          <h1>{{ project.name }}</h1>
          <p class="mono selectable path">{{ project.localPath ?? 'Not available on this machine' }}</p>
        </div>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost btn-icon" title="Re-detect stack and refresh Git" :disabled="refreshing" @click="refresh">
          <RefreshCw :class="{ spin: refreshing }" />
        </button>
        <button class="btn" :disabled="!project.available" @click="reveal"><FolderOpen /> Open folder</button>
        <button class="btn" :disabled="!project.available" @click="openShell"><SquareTerminal /> Shell</button>
        <button class="btn" data-resume-project @click="router.push(`/projects/${project.id}/resume`)"><History /> Resume project</button>
        <button class="btn" data-project-context @click="router.push({ name: 'context', query: { project: project.id } })"><Gauge /> Context</button>
        <button class="btn" data-project-workflow @click="router.push({ name: 'workflows', query: { start: '1', project: project.id } })"><Workflow /> Run workflow</button>
        <button class="btn btn-primary" :disabled="!project.available" @click="app.openLaunch(project.id)"><Sparkles /> Launch Claude</button>
      </div>
    </div>

    <div v-if="!project.available" class="notice notice-warn"><TriangleAlert /> This project is registered in the workspace but has no folder on this machine. Use “Locate…” on the Projects page.</div>

    <UsageSummary :project-id="project.id" />

    <div class="grid grid-2 top">
      <section class="card">
        <h2 class="card-title"><GitBranch /> Repository</h2>
        <GitSummary :status="git?.status ?? null" :loading="git?.loading" :error="git?.error" />
        <dl v-if="git?.status?.isRepo" class="facts">
          <dt>Upstream</dt>
          <dd class="mono">{{ git.status.upstream ?? 'none' }}</dd>
          <dt>Changes</dt>
          <dd>{{ git.status.staged }} staged · {{ git.status.unstaged }} modified · {{ git.status.untracked }} untracked</dd>
          <template v-if="git.status.lastCommit">
            <dt>Last commit</dt>
            <dd class="row commit">
              <GitCommitHorizontal />
              <span class="truncate selectable">{{ git.status.lastCommit.summary }}</span>
              <span class="faint">{{ relativeTime(git.status.lastCommit.timestampMs) }}</span>
            </dd>
          </template>
          <template v-if="project.remoteUrl">
            <dt>Remote</dt>
            <dd class="row mono selectable"><Link class="ic" /> {{ project.remoteUrl }}</dd>
          </template>
        </dl>
      </section>

      <section class="card">
        <h2 class="card-title"><FileText /> Stack and conventions</h2>
        <div class="chips">
          <span v-for="s in project.stack.stacks" :key="s" class="chip">{{ s }}</span>
          <span v-for="pm in project.stack.packageManagers" :key="pm" class="chip">{{ pm }}</span>
          <span v-if="!project.stack.stacks.length" class="faint">No stack detected</span>
        </div>
        <div class="docs">
          <span class="badge" :class="project.stack.hasClaudeMd ? 'badge-ok' : ''">CLAUDE.md</span>
          <span class="badge" :class="project.stack.hasAgentsMd ? 'badge-ok' : ''">AGENTS.md</span>
          <span class="badge" :class="project.stack.hasClaudeDir ? 'badge-ok' : ''">.claude/</span>
          <span class="badge" :class="project.stack.hasReadme ? 'badge-ok' : ''">README</span>
        </div>
        <template v-if="project.stack.scripts.length">
          <div class="label scripts-label">Suggested commands <span class="faint">(detected, never run automatically)</span></div>
          <ul class="scripts">
            <li v-for="s in project.stack.scripts" :key="s.kind + s.command">
              <span class="kind">{{ s.kind }}</span><code class="selectable">{{ s.command }}</code>
            </li>
          </ul>
        </template>
      </section>

      <section class="card" data-project-context-card>
        <h2 class="card-title"><BookMarked /> Portable context</h2>
        <p class="faint">An overview, architecture notes, the current state and this project's tasks, decisions and memory live in the workspace repository (<span class="mono">project-context/</span>), so they travel with the workspace. Facts are read from the project's files and Git; nothing is invented, and documents you edit are never overwritten.</p>
        <button class="btn btn-sm" :disabled="contextBusy || !workspaces.active?.compat?.writable" data-project-context-init @click="initContext">Create or refresh</button>
        <p v-if="contextResult" class="faint" data-project-context-result>
          <span class="mono">{{ contextResult.dir }}/</span> — {{ contextResult.created.length }} created, {{ contextResult.updated.length }} updated, {{ contextResult.kept.length }} kept
        </p>
      </section>
    </div>

    <h2 class="section-title"><ListTodo class="title-ic" /> Open tasks <span class="count">{{ knowledge?.tasks.length ?? 0 }}</span></h2>
    <div v-if="knowledge?.tasks.length" class="list" data-project-tasks>
      <div v-for="t in knowledge.tasks" :key="t.path" class="list-row task-row" :data-task="t.id">
        <span class="badge">{{ STATUS_LABELS[t.status as TaskStatus] ?? t.status }}</span>
        <span class="task-title">{{ t.title }}</span>
        <span v-if="t.priority && t.priority !== 'medium'" class="badge" :class="{ 'badge-warn': t.priority === 'high', 'badge-error': t.priority === 'urgent' }">{{ t.priority }}</span>
        <span v-if="t.branch" class="chip mono">{{ t.branch }}</span>
        <button class="btn btn-sm btn-primary" :disabled="!project.available" @click="launchTask(t)"><Sparkles /> Launch</button>
      </div>
    </div>
    <div v-else class="empty">
      <p>No open tasks for this project. Add tasks in <RouterLink to="/knowledge">Knowledge</RouterLink>, or let Claude create them with the NEXUS tools.</p>
    </div>

    <h2 class="section-title"><NotebookPen class="title-ic" /> Notes for {{ knowledge?.git.branch ?? 'this project' }} <span class="count">{{ knowledge?.notes.length ?? 0 }}</span></h2>
    <div v-if="knowledge?.notes.length" class="list" data-project-notes>
      <div v-for="n in knowledge.notes" :key="n.path" class="list-row">
        <span class="badge">{{ n.kind }}</span>
        <span class="task-title">{{ n.title }}</span>
        <span v-if="n.relevance === 'this-branch'" class="chip mono">this branch</span>
        <span v-else-if="n.relevance === 'inherited'" class="chip mono" :title="`Recorded on ${n.branch}; its commit is part of this branch`">from {{ n.branch }}</span>
      </div>
    </div>
    <p v-if="knowledge?.hiddenOtherBranch" class="faint">{{ knowledge.hiddenOtherBranch }} note(s) of other branches are hidden — Git decides: their commits are not in the checked-out branch.</p>
    <div v-if="!knowledge?.notes.length && !knowledge?.hiddenOtherBranch" class="empty">
      <p>No notes yet. Claude can record branch-specific memory with <code>record_memory</code> (scope <code>branch</code>).</p>
    </div>

    <InstructionsPanel v-if="project.available" :project-id="project.id" class="instructions-card" />

    <h2 class="section-title">Checkpoints <span class="count">{{ checkpoints.length }}</span></h2>
    <div v-if="checkpoints.length" class="list">
      <div v-for="c in checkpoints" :key="c.id" class="list-row checkpoint">
        <span class="badge" :class="{ 'badge-ok': c.kind === 'done', 'badge-warn': c.kind === 'blocker', 'badge-accent': c.kind === 'decision' }">{{ c.kind }}</span>
        <span class="cp-summary selectable">{{ c.summary }}</span>
        <span class="faint cp-meta">{{ c.sessionTitle }} · {{ relativeTime(c.createdAt) }}</span>
      </div>
    </div>
    <div v-else class="empty">
      <p>No checkpoints yet. In a NEXUS-launched session Claude can record decisions, blockers and progress with the NEXUS <code>checkpoint</code> tool. They appear here.</p>
    </div>

    <h2 class="section-title">Sessions <span class="count">{{ projectSessions.length }}</span></h2>
    <div v-if="projectSessions.length" class="list">
      <SessionRow v-for="s in projectSessions" :key="s.id" :session="s" :show-project="false" />
    </div>
    <div v-else class="empty">
      <p>No sessions yet. Launch Claude here to start one — NEXUS records it so you can resume later.</p>
    </div>

    <h2 class="section-title danger-title">Danger zone</h2>
    <div class="card danger">
      <div>
        <strong>Remove from workspace</strong>
        <p class="muted">Unregisters the project. Files and Git history stay exactly as they are.</p>
      </div>
      <button class="btn btn-danger" @click="unregister"><Trash2 /> Remove</button>
    </div>
  </div>
  <div v-else class="page">
    <div class="empty"><p>Project not found in this workspace.</p></div>
  </div>
</template>

<style scoped>
.back {
  margin: -8px 0 10px -8px;
}
.task-row .btn {
  margin-left: auto;
}
.task-title {
  font-weight: 550;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.title-ic {
  width: 16px;
  height: 16px;
  vertical-align: -2px;
}
.instructions-card {
  margin-top: 22px;
}
.title-block {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
}
.avatar {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 800;
  font-size: 18px;
  color: #fff;
  background: linear-gradient(135deg, var(--accent), var(--cyan));
  flex: none;
}
.path {
  font-size: 12.5px;
  word-break: break-all;
}
.top {
  align-items: start;
}
.facts {
  display: grid;
  grid-template-columns: 100px 1fr;
  gap: 8px 14px;
  margin: 16px 0 0;
  font-size: 13px;
}
.facts dt {
  color: var(--text-faint);
}
.facts dd {
  margin: 0;
  min-width: 0;
  word-break: break-all;
}
.commit svg,
.ic {
  width: 14px;
  height: 14px;
  flex: none;
}
.docs {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 14px;
}
.scripts-label {
  margin-top: 16px;
}
.scripts {
  list-style: none;
  padding: 0;
  margin: 8px 0 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.scripts li {
  display: flex;
  gap: 10px;
  align-items: center;
}
.scripts .kind {
  width: 72px;
  color: var(--text-faint);
  font-size: 12px;
}
.checkpoint {
  align-items: flex-start;
}
.cp-summary {
  flex: 1;
  white-space: pre-wrap;
}
.cp-meta {
  font-size: 12px;
  flex: none;
}
.danger-title {
  color: var(--red);
}
.danger {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  border-color: color-mix(in srgb, var(--red) 30%, var(--border));
}
.danger p {
  margin: 4px 0 0;
}
</style>
