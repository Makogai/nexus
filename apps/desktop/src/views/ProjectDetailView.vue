<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import {
  ArrowLeft,
  FileText,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  Link,
  RefreshCw,
  Sparkles,
  SquareTerminal,
  Trash2,
  TriangleAlert,
} from '@lucide/vue'
import { relativeTime } from '../lib/format'
import { api } from '../lib/ipc'
import type { Checkpoint } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import GitSummary from '../components/GitSummary.vue'
import SessionRow from '../components/SessionRow.vue'

const props = defineProps<{ id: string }>()
const app = useAppStore()
const projects = useProjectStore()
const sessions = useSessionStore()
const router = useRouter()
const refreshing = ref(false)

const project = computed(() => projects.byId[props.id] ?? null)
const git = computed(() => projects.git[props.id])
const projectSessions = computed(() => sessions.list.filter((s) => s.projectId === props.id && !s.archivedAt))
const checkpoints = ref<Checkpoint[]>([])

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
  },
  { immediate: true },
)
watch(() => app.externalChanges, loadCheckpoints)
onActivated(() => {
  loadGit()
  loadCheckpoints()
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
        <button class="btn btn-primary" :disabled="!project.available" @click="app.openLaunch(project.id)"><Sparkles /> Launch Claude</button>
      </div>
    </div>

    <div v-if="!project.available" class="notice notice-warn"><TriangleAlert /> This project is registered in the workspace but has no folder on this machine. Use “Locate…” on the Projects page.</div>

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
    </div>

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
