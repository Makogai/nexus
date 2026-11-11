<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { ArrowLeft, ArrowUp, BookMarked, Boxes, Ellipsis, FileText, FolderOpen, Gauge, GitBranch, GitCommitHorizontal, GraduationCap, History, Inbox, Link, ListTodo, NotebookPen, RefreshCw, Settings2, Sparkles, SquareTerminal, Trash2, TriangleAlert, Workflow } from '@lucide/vue'
import { needsUser, relativeTime } from '../lib/format'
import { api, toIpcError } from '../lib/ipc'
import { useWorkspaceStore } from '../stores/workspaces'
import { typeIntoTerminal } from '../lib/terminal'
import { failureGuidance, failureTitle } from '../lib/push'
import type { Checkpoint, ContextInitialized, EnvCheckItem, EnvironmentReport, NeedsYouItem, ProjectKnowledge, PushRequest } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import GitSummary from '../components/GitSummary.vue'
import UsageSummary from '../components/UsageSummary.vue'
import InstructionsPanel from '../components/InstructionsPanel.vue'
import NeedsYouList from '../components/NeedsYouList.vue'
import ProjectCommits from '../components/ProjectCommits.vue'
import ProjectFiles from '../components/ProjectFiles.vue'
import ProjectLearn from '../components/ProjectLearn.vue'
import ProjectSessions from '../components/ProjectSessions.vue'
import ProjectTasks from '../components/ProjectTasks.vue'

const props = defineProps<{ id: string }>()
const app = useAppStore()
const projects = useProjectStore()
const sessions = useSessionStore()
const router = useRouter()
const route = useRoute()
const refreshing = ref(false)

/**
 * Tabs (ADR-0048), kept in the URL (`?tab=`): Overview (what needs you, repository, stack, usage), Sessions, Tasks,
 * Knowledge (notes, decisions, checkpoints, instructions), Learn (W4), Files and Commits (W3), Settings (environment,
 * portable context, danger zone). Ctrl+1…8 switch between them.
 */
const TABS = ['overview', 'sessions', 'tasks', 'knowledge', 'learn', 'files', 'commits', 'settings'] as const
type Tab = (typeof TABS)[number]
const NEEDS_FOLDER: Tab[] = ['learn', 'files', 'commits']
const tab = computed<Tab>(() => {
  const q = route.query.tab
  return typeof q === 'string' && (TABS as readonly string[]).includes(q) ? (q as Tab) : 'overview'
})
function setTab(t: Tab) {
  router.replace({ query: { ...route.query, tab: t === 'overview' ? undefined : t } })
}

const project = computed(() => projects.byId[props.id] ?? null)
const workspaces = useWorkspaceStore()
const contextResult = ref<ContextInitialized | null>(null)
const env = ref<EnvironmentReport | null>(null)
const envBusy = ref(false)
/** Environment requirements from the project's files (Portable Workspace P5); `check` probes this computer. */
async function loadEnv(check: boolean) {
  const ws = workspaces.active
  if (!ws) return
  envBusy.value = true
  try {
    env.value = await api.projectEnvironment(ws.id, props.id, check)
  } catch (e) {
    app.toast('error', `Could not read the environment: ${toIpcError(e).message}`)
  } finally {
    envBusy.value = false
  }
}
const ENV_STATUS: Record<string, string> = { ok: 'badge-ok', missing: 'badge-error', mismatch: 'badge-warn', unknown: '' }
/** One row per requirement; checked rows carry what this computer has. */
const envRows = computed(() => {
  const e = env.value
  if (!e) return []
  if (e.checks.length) return e.checks
  return e.environment.requirements.map((r) => ({ ...r, name: r.tool, installed: null, foundAt: null, status: null as EnvCheckItem['status'] | null, installUrl: null, installCommand: null as string[] | null }))
})
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
/** What waits for the person in this project (ADR-0048). */
const needsYou = ref<NeedsYouItem[]>([])

/** Tab labels carry counts: running/total sessions (live and attention dots), open tasks and those in review. */
const counts = computed(() => {
  const live = projectSessions.value.filter((s) => s.running || s.liveElsewhere)
  return {
    running: live.length,
    sessions: projectSessions.value.length,
    waiting: live.some((s) => s.kind === 'claude' && needsUser(s.status)),
    tasks: knowledge.value?.tasks.length ?? 0,
    review: knowledge.value?.tasks.filter((t) => t.status === 'review').length ?? 0,
    notes: knowledge.value?.notes.length ?? 0,
  }
})

async function loadKnowledge() {
  pushRequests.value = await api.pushRequestOpen(props.id).catch(() => pushRequests.value)
  const ws = workspaces.active
  if (ws) needsYou.value = await api.needsYou(ws.id, props.id).catch(() => needsYou.value)
  if (!project.value?.available) return
  knowledge.value = await api.projectKnowledge(props.id).catch(() => knowledge.value)
}
/** Push requests waiting for the person (ADR-0043), and failed ones until dismissed or superseded (ADR-0047). */
const pushRequests = ref<PushRequest[]>([])
const dismissing = ref<string | null>(null)
async function dismissPush(r: PushRequest) {
  dismissing.value = r.id
  try {
    await api.pushRequestDismiss(r.id)
    pushRequests.value = pushRequests.value.filter((x) => x.id !== r.id)
    needsYou.value = needsYou.value.filter((x) => x.id !== r.id)
  } catch (e) {
    app.toast('error', `Could not dismiss: ${toIpcError(e).message}`)
  } finally {
    dismissing.value = null
  }
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
  () => [app.externalChanges, app.pushRequestsChanged],
  () => {
    loadCheckpoints()
    loadKnowledge()
  },
)
// A push changes ahead/behind on the Repository card.
watch(
  () => app.pushRequestsChanged,
  () => loadGit(true),
)
// A session of this project starts waiting for the person (or stops): the "Needs you" strip follows.
watch(
  () => projectSessions.value.map((s) => `${s.id}:${s.status.status}`).join(),
  () => loadKnowledge(),
)

/** Ctrl+1…8 switch tabs (not while typing in a field or a terminal). */
function onKey(e: KeyboardEvent) {
  if (!(e.ctrlKey || e.metaKey) || e.shiftKey || e.altKey) return
  const n = Number(e.key)
  if (!Number.isInteger(n) || n < 1 || n > TABS.length) return
  const target = e.target as HTMLElement | null
  if (target?.closest?.('.xterm, input, textarea, select, [contenteditable="true"], .cm-editor')) return
  const t = TABS[n - 1]!
  if (NEEDS_FOLDER.includes(t) && !project.value?.available) return
  e.preventDefault()
  setTab(t)
}
let listening = false
function listen(on: boolean) {
  if (on === listening) return
  listening = on
  if (on) window.addEventListener('keydown', onKey)
  else window.removeEventListener('keydown', onKey)
}
onMounted(() => listen(true))
onUnmounted(() => listen(false))
onDeactivated(() => listen(false))
onActivated(() => {
  listen(true)
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

/** The header's "⋯" menu (kept in the DOM, so its items keep their selectors). */
const menuOpen = ref(false)
const menuRoot = ref<HTMLElement>()
function onDocClick(e: MouseEvent) {
  if (menuOpen.value && !menuRoot.value?.contains(e.target as Node)) menuOpen.value = false
}
onMounted(() => document.addEventListener('mousedown', onDocClick))
onUnmounted(() => document.removeEventListener('mousedown', onDocClick))
function menu(action: () => unknown) {
  menuOpen.value = false
  action()
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
          <p v-if="git?.status?.conflicted || git?.status?.operation" class="git-alert" data-project-git-alert>
            <span v-if="git.status.conflicted" class="badge badge-error">{{ git.status.conflicted }} conflicted</span>
            <span v-if="git.status.operation" class="badge badge-warn">{{ git.status.operation }} in progress</span>
          </p>
        </div>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost btn-icon" title="Re-detect stack and refresh Git" :disabled="refreshing" :aria-busy="refreshing" @click="refresh">
          <RefreshCw :class="{ spin: refreshing }" />
        </button>
        <div ref="menuRoot" class="more">
          <button class="btn" :aria-expanded="menuOpen" aria-haspopup="menu" title="More project actions" data-project-more @click="menuOpen = !menuOpen"><Ellipsis /> More</button>
          <div v-show="menuOpen" class="menu" role="menu">
            <button class="menu-item" role="menuitem" :disabled="!project.available" @click="menu(reveal)"><FolderOpen /> Open folder</button>
            <button class="menu-item" role="menuitem" data-resume-project @click="menu(() => router.push(`/projects/${project!.id}/resume`))"><History /> Resume project</button>
            <button class="menu-item" role="menuitem" data-open-history @click="menu(() => router.push(`/projects/${project!.id}/history`))"><NotebookPen /> Development history</button>
            <button class="menu-item" role="menuitem" data-project-context @click="menu(() => router.push({ name: 'context', query: { project: project!.id } }))"><Gauge /> Context</button>
            <button class="menu-item" role="menuitem" data-project-workflow @click="menu(() => router.push({ name: 'workflows', query: { start: '1', project: project!.id } }))"><Workflow /> Run workflow</button>
          </div>
        </div>
        <button class="btn" :disabled="!project.available" @click="openShell"><SquareTerminal /> Shell</button>
        <button class="btn btn-primary" :disabled="!project.available" data-project-launch @click="app.openLaunch(project.id)"><Sparkles /> Launch</button>
      </div>
    </div>

    <div v-if="!project.available" class="notice notice-warn"><TriangleAlert /> This project is registered in the workspace but has no folder on this machine. Use “Locate…” on the Projects page.</div>

    <template v-for="r in pushRequests" :key="r.id">
      <!-- A failed push stays until dismissed or superseded (ADR-0047): why, what next, review again. -->
      <div v-if="r.status === 'failed'" class="notice notice-error push-banner" :data-push-request="r.id" :data-push-failed="r.failureKind ?? 'other'">
        <TriangleAlert />
        <span><strong>Push of <span class="mono">{{ r.branch }}</span> failed: {{ failureTitle(r.failureKind).toLowerCase() }}.</strong> {{ failureGuidance(r.failureKind) }}</span>
        <button class="btn btn-sm" data-push-open @click="app.pushRequestId = r.id">Review again</button>
        <button class="btn btn-sm btn-ghost" :disabled="dismissing === r.id" :aria-busy="dismissing === r.id" data-push-banner-dismiss @click="dismissPush(r)">Dismiss</button>
      </div>
      <div v-else class="notice notice-accent push-banner" :data-push-request="r.id">
        <ArrowUp />
        <span>Claude asks to push <strong class="mono">{{ r.branch }}</strong><template v-if="r.reason"> — “{{ r.reason }}”</template></span>
        <button class="btn btn-sm btn-primary" data-push-open @click="app.pushRequestId = r.id">Review &amp; push</button>
      </div>
    </template>

    <div class="tabs" role="tablist" aria-label="Project views">
      <button role="tab" class="tab" :class="{ active: tab === 'overview' }" :aria-selected="tab === 'overview'" data-project-tab="overview" title="Ctrl+1" @click="setTab('overview')">
        Overview<span v-if="needsYou.length" class="tab-badge" :title="`${needsYou.length} waiting for you`">{{ needsYou.length }}</span>
      </button>
      <button role="tab" class="tab" :class="{ active: tab === 'sessions' }" :aria-selected="tab === 'sessions'" data-project-tab="sessions" title="Ctrl+2" @click="setTab('sessions')">
        <SquareTerminal class="tab-ic" /> Sessions
        <span class="tab-count" data-tab-count="sessions">{{ counts.running }}/{{ counts.sessions }}</span>
        <span v-if="counts.running" class="tab-dot live" title="Running" />
        <span v-if="counts.waiting" class="tab-dot attention" title="A session waits for you" data-tab-attention />
      </button>
      <button role="tab" class="tab" :class="{ active: tab === 'tasks' }" :aria-selected="tab === 'tasks'" data-project-tab="tasks" title="Ctrl+3" @click="setTab('tasks')">
        <ListTodo class="tab-ic" /> Tasks
        <span class="tab-count" data-tab-count="tasks">{{ counts.tasks }}</span>
        <span v-if="counts.review" class="tab-badge" :title="`${counts.review} ready for review`" data-tab-review>{{ counts.review }} review</span>
      </button>
      <button role="tab" class="tab" :class="{ active: tab === 'knowledge' }" :aria-selected="tab === 'knowledge'" data-project-tab="knowledge" title="Ctrl+4" @click="setTab('knowledge')">
        <NotebookPen class="tab-ic" /> Knowledge<span class="tab-count">{{ counts.notes }}</span>
      </button>
      <button role="tab" class="tab" :class="{ active: tab === 'learn' }" :aria-selected="tab === 'learn'" :disabled="!project.available" data-project-tab="learn" title="Ctrl+5" @click="setTab('learn')"><GraduationCap class="tab-ic" /> Learn</button>
      <button role="tab" class="tab" :class="{ active: tab === 'files' }" :aria-selected="tab === 'files'" :disabled="!project.available" data-project-tab="files" title="Ctrl+6" @click="setTab('files')"><FileText class="tab-ic" /> Files</button>
      <button role="tab" class="tab" :class="{ active: tab === 'commits' }" :aria-selected="tab === 'commits'" :disabled="!project.available" data-project-tab="commits" title="Ctrl+7" @click="setTab('commits')"><GitCommitHorizontal class="tab-ic" /> Commits</button>
      <button role="tab" class="tab" :class="{ active: tab === 'settings' }" :aria-selected="tab === 'settings'" data-project-tab="settings" title="Ctrl+8" @click="setTab('settings')"><Settings2 class="tab-ic" /> Settings</button>
    </div>

    <ProjectSessions v-if="tab === 'sessions'" :project-id="project.id" :available="project.available" />
    <ProjectTasks v-else-if="tab === 'tasks'" :project-id="project.id" :available="project.available" :knowledge="knowledge" @changed="loadKnowledge" />
    <ProjectLearn v-else-if="tab === 'learn' && project.available" :project-id="project.id" />
    <ProjectFiles v-else-if="tab === 'files' && project.available && project.localPath" :project-id="project.id" :root="project.localPath" />
    <ProjectCommits v-else-if="tab === 'commits' && project.available" :project-id="project.id" />

    <template v-else-if="tab === 'knowledge'">
      <h2 class="section-title first"><NotebookPen class="title-ic" /> Notes and decisions for {{ knowledge?.git.branch ?? 'this project' }} <span class="count">{{ knowledge?.notes.length ?? 0 }}</span></h2>
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

      <h2 class="section-title">Checkpoints <span class="count">{{ checkpoints.length }}</span></h2>
      <div v-if="checkpoints.length" class="list" data-project-checkpoints>
        <div v-for="c in checkpoints" :key="c.id" class="list-row checkpoint">
          <span class="badge" :class="{ 'badge-ok': c.kind === 'done', 'badge-warn': c.kind === 'blocker', 'badge-accent': c.kind === 'decision' }">{{ c.kind }}</span>
          <span class="cp-summary selectable">{{ c.summary }}</span>
          <span class="faint cp-meta">{{ c.sessionTitle }} · {{ relativeTime(c.createdAt) }}</span>
        </div>
      </div>
      <div v-else class="empty">
        <p>No checkpoints yet. In a NEXUS-launched session Claude can record decisions, blockers and progress with the NEXUS <code>checkpoint</code> tool. They appear here.</p>
      </div>

      <InstructionsPanel v-if="project.available" :project-id="project.id" class="instructions-card" />
    </template>

    <template v-else-if="tab === 'settings'">
      <div class="grid grid-2 top">
        <section class="card" data-project-environment>
          <h2 class="card-title"><Boxes /> Environment</h2>
          <p class="faint">What this project needs, read from its own files (they stay authoritative). Checking asks each tool for its version only; installing is always your step in a terminal.</p>
          <div class="row-actions">
            <button class="btn btn-sm" :disabled="envBusy" :aria-busy="envBusy" data-env-load @click="loadEnv(false)">Show requirements</button>
            <button class="btn btn-sm" :disabled="envBusy" :aria-busy="envBusy" data-env-check @click="loadEnv(true)"><RefreshCw v-if="envBusy" class="spin" /> Check this computer</button>
          </div>
          <template v-if="env">
            <p class="faint">From the {{ env.from }}.</p>
            <table class="env-table" data-env-table>
              <tbody>
                <tr v-for="(r, i) in envRows" :key="r.tool + r.source + i" :data-env-row="r.tool">
                  <td><strong>{{ r.name }}</strong></td>
                  <td class="mono">{{ r.constraint || 'any' }}</td>
                  <td class="faint">{{ r.source }}</td>
                  <td v-if="r.status">
                    <span class="badge" :class="ENV_STATUS[r.status]">{{ r.status }}</span>
                    <span v-if="r.installed" class="faint"> {{ r.installed }}</span>
                    <button v-if="r.installCommand" class="btn btn-sm btn-ghost" :data-env-install="r.tool" @click="typeIntoTerminal(r.installCommand.join(' '))">Type install command</button>
                  </td>
                </tr>
              </tbody>
            </table>
            <p v-if="env.environment.containers.composeServices.length" class="faint">Local services ({{ env.environment.containers.composeFile }}): {{ env.environment.containers.composeServices.join(', ') }}</p>
            <p v-if="env.environment.containers.devcontainer" class="faint">Dev container: {{ env.environment.containers.devcontainer }} <span v-if="env.environment.containers.devcontainerImage" class="mono">{{ env.environment.containers.devcontainerImage }}</span></p>
            <p v-if="env.environment.envVars.length" class="faint">Environment variables (names only): <span class="mono">{{ env.environment.envVars.map((v) => v.name).join(', ') }}</span></p>
            <p v-if="env.environment.setupSection" class="faint">Setup instructions: README.md — “{{ env.environment.setupSection }}”</p>
          </template>
        </section>

        <section class="card" data-project-context-card>
          <h2 class="card-title"><BookMarked /> Portable context</h2>
          <p class="faint">An overview, architecture notes, the current state and this project's tasks, decisions and memory live in the workspace repository (<span class="mono">project-context/</span>), so they travel with the workspace. Facts are read from the project's files and Git; nothing is invented, and documents you edit are never overwritten.</p>
          <button class="btn btn-sm" :disabled="contextBusy || !workspaces.active?.compat?.writable" :aria-busy="contextBusy" data-project-context-init @click="initContext"><RefreshCw v-if="contextBusy" class="spin" /> {{ contextBusy ? 'Writing…' : 'Create or refresh' }}</button>
          <p v-if="contextResult" class="faint" data-project-context-result>
            <span class="mono">{{ contextResult.dir }}/</span> — {{ contextResult.created.length }} created, {{ contextResult.updated.length }} updated, {{ contextResult.kept.length }} kept
          </p>
        </section>
      </div>

      <h2 class="section-title danger-title">Danger zone</h2>
      <div class="card danger">
        <div>
          <strong>Remove from workspace</strong>
          <p class="muted">Unregisters the project. Files and Git history stay exactly as they are.</p>
        </div>
        <button class="btn btn-danger" @click="unregister"><Trash2 /> Remove</button>
      </div>
    </template>

    <template v-else>
      <h2 class="section-title first"><Inbox class="title-ic" /> Needs you <span class="count">{{ needsYou.length }}</span></h2>
      <NeedsYouList v-if="needsYou.length" :items="needsYou" :show-project="false" />
      <p v-else class="faint nothing" data-needs-you-empty>Nothing waits for you in this project.</p>

      <div class="grid grid-2 top">
        <section class="card" data-project-repository>
          <h2 class="card-title"><GitBranch /> Repository</h2>
          <GitSummary :status="git?.status ?? null" :loading="git?.loading" :error="git?.error" />
          <dl v-if="git?.status?.isRepo" class="facts">
            <dt>Upstream</dt>
            <dd class="mono">{{ git.status.upstream ?? 'none' }}</dd>
            <dt>Changes</dt>
            <dd data-project-changes>
              {{ git.status.staged }} staged · {{ git.status.unstaged }} modified · {{ git.status.untracked }} untracked<template v-if="git.status.conflicted"> · <strong class="conflicted">{{ git.status.conflicted }} conflicted</strong></template>
              <span v-if="git.status.operation" class="badge badge-warn">{{ git.status.operation }} in progress</span>
            </dd>
            <template v-if="git.status.upstream">
              <dt>Remote</dt>
              <dd data-project-ahead-behind :title="`Against ${git.status.upstream}, as of the last fetch`">
                {{ git.status.ahead }} ahead · {{ git.status.behind }} behind<span v-if="git.status.ahead && git.status.behind" class="badge badge-warn">diverged</span>
              </dd>
            </template>
            <template v-if="git.status.lastCommit">
              <dt>Last commit</dt>
              <dd class="row commit">
                <GitCommitHorizontal />
                <span class="truncate selectable">{{ git.status.lastCommit.summary }}</span>
                <span class="faint">{{ relativeTime(git.status.lastCommit.timestampMs) }}</span>
              </dd>
            </template>
            <template v-if="project.remoteUrl">
              <dt>Remote URL</dt>
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
      </div>

      <UsageSummary :project-id="project.id" class="usage" />
    </template>
  </div>
  <div v-else class="page">
    <div class="empty"><p>Project not found in this workspace.</p></div>
  </div>
</template>

<style scoped>
.tabs {
  display: flex;
  gap: 6px;
  margin: 4px 0 16px;
}
.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 30px;
  padding: 0 12px;
  border-radius: 8px;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}
.tab:disabled {
  opacity: 0.5;
  cursor: default;
}
.tab.active {
  background: var(--bg-active);
  border-color: var(--border-strong);
  font-weight: 600;
}
.tab-ic {
  width: 14px;
  height: 14px;
}
.tabs {
  flex-wrap: wrap;
}
.tab-count {
  color: var(--text-faint);
  font-weight: 500;
  font-size: 12px;
}
.tab-badge {
  font-size: 11px;
  font-weight: 600;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--accent-soft);
  color: var(--accent-strong);
}
.tab-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex: none;
}
.tab-dot.live {
  background: var(--green);
}
.tab-dot.attention {
  background: var(--amber);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--amber) 30%, transparent);
}
.more {
  position: relative;
}
.more .menu {
  position: absolute;
  right: 0;
  top: calc(100% + 4px);
  z-index: 20;
  min-width: 210px;
  padding: 4px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--bg-raised);
  box-shadow: var(--shadow, 0 8px 24px rgb(0 0 0 / 0.25));
  display: grid;
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}
.menu-item:hover:not(:disabled) {
  background: var(--bg-hover);
}
.menu-item:disabled {
  opacity: 0.5;
  cursor: default;
}
.menu-item svg {
  width: 15px;
  height: 15px;
}
.section-title.first {
  margin-top: 4px;
}
.nothing {
  margin: 0 0 6px;
}
.usage {
  margin-top: 16px;
}
.row-actions {
  display: flex;
  gap: 8px;
  margin: 6px 0;
}
.env-table {
  width: 100%;
  font-size: 12.5px;
  border-collapse: collapse;
}
.env-table td {
  padding: 4px 6px;
  border-top: 1px solid var(--border);
  vertical-align: middle;
}
.back {
  margin: -8px 0 10px -8px;
}
.push-banner {
  align-items: center;
}
.push-banner .btn:first-of-type {
  margin-left: auto;
}
.push-banner .btn {
  flex: none;
}
.git-alert {
  display: flex;
  gap: 6px;
  margin: 4px 0 0;
}
.conflicted {
  color: var(--red);
}
.facts dd .badge {
  margin-left: 6px;
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
/* Many actions: they wrap under the title instead of squeezing the path into a narrow column. */
.page-header {
  flex-wrap: wrap;
  align-items: center;
}
.page-header .page-actions {
  flex-wrap: wrap;
}
.title-block {
  display: flex;
  align-items: center;
  gap: 14px;
  min-width: 0;
  flex: 1 1 380px;
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
