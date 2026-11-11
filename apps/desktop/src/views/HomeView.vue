<script setup lang="ts">
import { computed, onActivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { FolderGit2, History, Inbox, Plus, Sparkles, SquareTerminal, UserCog } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { NeedsYouItem } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import { useWorkspaceStore } from '../stores/workspaces'
import NeedsYouList from '../components/NeedsYouList.vue'
import ProjectCard from '../components/ProjectCard.vue'
import SessionRow from '../components/SessionRow.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const sessions = useSessionStore()
const router = useRouter()

const wsId = computed(() => workspaces.active?.id)
const wsSessions = computed(() => sessions.list.filter((s) => s.workspaceId === wsId.value && !s.archivedAt))
const running = computed(() => wsSessions.value.filter((s) => s.running))
const continueList = computed(() =>
  wsSessions.value.filter((s) => !s.running && s.kind === 'claude' && s.resumable).slice(0, 4),
)
const recentProjects = computed(() => projects.list.slice(0, 6))

const greeting = computed(() => {
  const h = new Date().getHours()
  return h < 5 ? 'Working late' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening'
})

/** What waits for the person across the workspace's projects (ADR-0048) — built from what NEXUS already records. */
const needsYou = ref<NeedsYouItem[]>([])
async function loadNeedsYou() {
  if (wsId.value) needsYou.value = await api.needsYou(wsId.value).catch(() => needsYou.value)
}
watch(
  () => [wsId.value, app.externalChanges, app.pushRequestsChanged, sessions.list.map((s) => `${s.id}:${s.status.status}`).join()],
  () => loadNeedsYou(),
  { immediate: true },
)

onMounted(() => {
  if (!profiles.installations) profiles.loadInstallations().catch(() => undefined)
  profiles.detect().catch(() => undefined)
})
onActivated(() => {
  sessions.load().catch(() => undefined)
  loadNeedsYou()
})
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>{{ greeting }}</h1>
        <p>{{ workspaces.active?.name }} · {{ projects.list.length }} projects · {{ running.length }} running</p>
      </div>
      <div class="page-actions">
        <button class="btn" @click="router.push('/projects')"><Plus /> Add project</button>
        <button class="btn btn-primary" @click="app.openLaunch()"><Sparkles /> Launch</button>
      </div>
    </div>

    <div v-if="profiles.list.length === 0" class="card setup">
      <UserCog class="setup-icon" />
      <div>
        <h3>Register your Claude configuration</h3>
        <p class="muted">
          <template v-if="profiles.unregistered.length">
            NEXUS found {{ profiles.unregistered.map((d) => d.suggestedName).join(', ') }}. Register them to launch sessions with the right account.
          </template>
          <template v-else>Point NEXUS at a Claude Code configuration folder, or create a fresh one.</template>
        </p>
      </div>
      <button class="btn btn-primary" @click="router.push('/profiles')">Set up profiles</button>
    </div>

    <template v-if="needsYou.length">
      <h2 class="section-title" data-home-needs-you><Inbox class="st-icon" /> Needs you <span class="count">{{ needsYou.length }}</span></h2>
      <NeedsYouList :items="needsYou" />
    </template>

    <template v-if="running.length">
      <h2 class="section-title"><SquareTerminal class="st-icon live" /> Running now <span class="count">{{ running.length }}</span></h2>
      <div class="list">
        <SessionRow v-for="s in running" :key="s.id" :session="s" compact />
      </div>
    </template>

    <template v-if="continueList.length">
      <h2 class="section-title"><History class="st-icon" /> Continue where you left off</h2>
      <div class="list">
        <SessionRow v-for="s in continueList" :key="s.id" :session="s" compact />
      </div>
    </template>

    <h2 class="section-title">
      <FolderGit2 class="st-icon" /> Projects <span class="count">{{ projects.list.length }}</span>
      <span class="spacer" />
      <RouterLink v-if="projects.list.length > 6" to="/projects" class="see-all">See all</RouterLink>
    </h2>
    <div v-if="recentProjects.length" class="grid grid-3">
      <ProjectCard v-for="p in recentProjects" :key="p.id" :project="p" />
    </div>
    <div v-else class="empty">
      <FolderGit2 class="empty-icon" />
      <h3>No projects yet</h3>
      <p>Register an existing Git repository. NEXUS detects its stack and scripts without running anything.</p>
      <button class="btn btn-primary" @click="router.push('/projects')"><Plus /> Add a project</button>
    </div>

    <h2 class="section-title"><UserCog class="st-icon" /> Environment</h2>
    <div class="grid grid-3">
      <div class="card env">
        <div class="card-title">Claude Code</div>
        <template v-if="profiles.installations">
          <div v-for="i in profiles.installations" :key="i.path" class="env-row">
            <span class="mono">{{ i.version ?? 'unknown' }}</span>
            <span class="faint truncate" :title="i.path">{{ i.source }}</span>
            <span v-if="!i.launchable" class="badge badge-warn">not launchable</span>
          </div>
          <p v-if="profiles.installations.length === 0" class="muted">Not found. Install Claude Code, then restart NEXUS.</p>
        </template>
        <span v-else class="skeleton" style="width: 60%" />
      </div>
      <div class="card env">
        <div class="card-title">Claude profiles</div>
        <div v-for="p in profiles.list" :key="p.id" class="env-row">
          <span class="pdot" :style="{ background: p.color }" />
          <span class="truncate">{{ p.name }}</span>
          <span class="faint truncate">{{ p.configDir ? 'custom dir' : 'default location' }}</span>
        </div>
        <p v-if="profiles.list.length === 0" class="muted">None registered.</p>
      </div>
      <div class="card env">
        <div class="card-title">Git</div>
        <div class="env-row"><span class="mono">{{ app.info?.gitVersion ?? 'not found' }}</span></div>
        <p class="faint small">NEXUS uses your git and its credential helper. It never stores Git credentials.</p>
      </div>
    </div>
  </div>
</template>

<style scoped>
.setup {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 8px;
  border-color: var(--accent-ring);
  background: linear-gradient(135deg, var(--accent-soft), transparent 60%), var(--bg-panel);
}
.setup h3 {
  margin: 0 0 4px;
}
.setup p {
  margin: 0;
}
.setup > div {
  flex: 1;
}
.setup-icon {
  width: 28px;
  height: 28px;
  color: var(--accent-strong);
}
.st-icon {
  width: 16px;
  height: 16px;
  color: var(--text-faint);
}
.st-icon.live {
  color: var(--cyan);
}
.see-all {
  font-size: 13px;
  font-weight: 500;
  text-decoration: none;
}
.env-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 0;
  min-width: 0;
}
.pdot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex: none;
}
.small {
  font-size: 12px;
  margin: 8px 0 0;
}
</style>
