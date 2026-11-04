<script setup lang="ts">
import { computed, ref } from 'vue'
import { Archive, GitBranch, Pencil, RotateCcw, Sparkles, Square, SquareTerminal, SquareArrowOutUpRight } from '@lucide/vue'
import type { SessionView } from '../lib/types'
import { duration, relativeTime, sessionStatus, sessionTone } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import { api } from '../lib/ipc'

const props = withDefaults(defineProps<{ session: SessionView; compact?: boolean; showProject?: boolean }>(), {
  compact: false,
  showProject: true,
})

const app = useAppStore()
const profiles = useProfileStore()
const projects = useProjectStore()
const sessions = useSessionStore()

const editing = ref(false)
const draft = ref('')

const s = computed(() => props.session)
const profile = computed(() => (s.value.profileId ? profiles.byId[s.value.profileId] : null))
const project = computed(() => (s.value.projectId ? projects.byId[s.value.projectId] : null))
const tone = computed(() => sessionTone(s.value))
const workName = computed(
  () => profiles.launchOptions?.workProfiles.find((w) => w.id === s.value.workProfile)?.name ?? null,
)

function focus() {
  sessions.openTab(s.value.id)
}
async function resume() {
  await app.attempt(() => sessions.resume(s.value.id), 'Could not resume the conversation')
}
async function interrupt() {
  await app.attempt(() => api.sessionInterrupt(s.value.id), 'Could not interrupt')
}
async function terminate() {
  const ok = await app.confirm({
    title: 'Terminate this process?',
    message: `"${s.value.title}" will be force-stopped.${s.value.kind === 'claude' ? ' The Claude conversation stays resumable.' : ''}`,
    confirmLabel: 'Terminate',
    danger: true,
  })
  if (ok) await app.attempt(() => api.sessionTerminate(s.value.id), 'Could not terminate')
}
function startRename() {
  draft.value = s.value.title
  editing.value = true
}
async function saveRename() {
  editing.value = false
  if (draft.value.trim() && draft.value !== s.value.title) {
    await app.attempt(() => sessions.rename(s.value.id, draft.value), 'Could not rename')
  }
}
async function archive() {
  await app.attempt(() => sessions.archive(s.value.id), 'Could not archive')
}
</script>

<template>
  <div class="session" :class="{ compact }">
    <span class="dot" :class="{ 'dot-live': tone === 'live', 'dot-warn': tone === 'warn', 'dot-error': tone === 'error' }" />
    <Sparkles v-if="s.kind === 'claude'" class="kind" />
    <SquareTerminal v-else class="kind" />
    <div class="main">
      <div class="row">
        <input
          v-if="editing"
          v-model="draft"
          class="input rename"
          autofocus
          @keydown.enter="saveRename"
          @keydown.esc="editing = false"
          @blur="saveRename"
        />
        <span v-else class="title truncate">{{ s.title }}</span>
        <span v-if="profile" class="badge" :title="profile.configDir ?? 'default location'">
          <span class="pdot" :style="{ background: profile.color }" />{{ profile.name }}
        </span>
        <span v-if="workName" class="badge">{{ workName }}</span>
      </div>
      <div class="meta">
        <span :class="{ live: tone === 'live' }">{{ sessionStatus(s) }}</span>
        <span>·</span>
        <span>{{ relativeTime(s.startedAt) }}</span>
        <template v-if="s.running || s.endedAt">
          <span>·</span>
          <span>{{ duration(s.startedAt, s.endedAt) }}</span>
        </template>
        <template v-if="s.resumedFrom">
          <span>·</span>
          <span>resumed</span>
        </template>
        <template v-if="showProject && project">
          <span>·</span>
          <RouterLink :to="`/projects/${project.id}`" class="plink">{{ project.name }}</RouterLink>
        </template>
        <template v-if="s.branch">
          <span>·</span>
          <span class="row branch"><GitBranch />{{ s.branch }}</span>
        </template>
        <template v-if="s.kind === 'claude' && s.integrations?.mcp && s.integrations.mcp.state !== 'enabled'">
          <span>·</span>
          <span class="faint" :title="s.integrations.mcp.reason">NEXUS tools off</span>
        </template>
        <template v-if="s.kind === 'claude' && !s.running && s.conversationRecorded === false">
          <span>·</span>
          <span class="faint">no conversation recorded</span>
        </template>
      </div>
    </div>
    <div class="actions">
      <template v-if="s.running">
        <button class="btn btn-sm" title="Show in terminal" @click="focus"><SquareArrowOutUpRight /> Open</button>
        <button class="btn btn-sm btn-ghost" title="Send Ctrl+C" @click="interrupt">Ctrl+C</button>
        <button class="btn btn-sm btn-ghost btn-icon" title="Terminate…" @click="terminate"><Square /></button>
      </template>
      <span v-else-if="s.liveElsewhere" class="faint small">open in its own terminal</span>
      <template v-else>
        <button v-if="s.resumable" class="btn btn-sm btn-primary" title="claude --resume" @click="resume"><RotateCcw /> Resume</button>
        <button v-else-if="s.attached" class="btn btn-sm" @click="focus">Output</button>
        <button v-if="!compact" class="btn btn-sm btn-ghost btn-icon" title="Rename" @click="startRename"><Pencil /></button>
        <button v-if="!compact" class="btn btn-sm btn-ghost btn-icon" title="Archive" @click="archive"><Archive /></button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.session {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
}
.session.compact {
  padding: 10px 14px;
}
.kind {
  width: 16px;
  height: 16px;
  color: var(--text-faint);
  flex: none;
}
.main {
  flex: 1;
  min-width: 0;
}
.title {
  font-weight: 600;
  min-width: 0;
}
.rename {
  height: 28px;
  max-width: 320px;
}
.meta {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin-top: 3px;
  font-size: 12px;
  color: var(--text-faint);
}
.meta .live {
  color: var(--cyan);
}
.branch svg {
  width: 12px;
  height: 12px;
}
.branch {
  gap: 3px;
}
.plink {
  color: var(--text-muted);
  text-decoration: none;
}
.plink:hover {
  color: var(--accent-strong);
}
.pdot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
}
.small {
  font-size: 12px;
}
.actions {
  display: flex;
  gap: 6px;
  flex: none;
}
</style>
