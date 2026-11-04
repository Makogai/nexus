<script setup lang="ts">
import { computed, onActivated, ref, watch } from 'vue'
import { BellOff, Info, Send } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { AppNotification, NotificationCategory, NotificationFilter, NotificationPrefs, NotificationPriority } from '../lib/types'
import { attentionLabels, categoryLabels, needsUser, priorityLabels, relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useNotificationStore } from '../stores/notifications'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import { useWorkspaceStore } from '../stores/workspaces'
import NotificationItem from '../components/NotificationItem.vue'

const app = useAppStore()
const store = useNotificationStore()
const sessions = useSessionStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const profiles = useProfileStore()

type Tab = 'inbox' | 'history' | 'preferences'
const tab = ref<Tab>('inbox')
const items = ref<AppNotification[]>([])
const filter = ref<{ scope: 'workspace' | 'all'; projectId: string; profileId: string; sessionId: string; priority: '' | NotificationPriority; unreadOnly: boolean }>({
  scope: 'workspace',
  projectId: '',
  profileId: '',
  sessionId: '',
  priority: '',
  unreadOnly: false,
})
const draft = ref<NotificationPrefs | null>(null)
const quiet = ref({ on: false, from: '22:00', to: '07:00' })

const sessionOptions = computed(() =>
  sessions.list.filter((s) => s.kind === 'claude' && (filter.value.scope === 'all' || s.workspaceId === workspaces.active?.id)).slice(0, 50),
)

async function load() {
  const f: NotificationFilter = {
    workspaceId: filter.value.scope === 'workspace' ? (workspaces.active?.id ?? null) : null,
    projectId: filter.value.projectId || null,
    profileId: filter.value.profileId || null,
    sessionId: filter.value.sessionId || null,
    priority: filter.value.priority || null,
    unreadOnly: filter.value.unreadOnly,
    includeDismissed: tab.value === 'history',
    limit: 300,
  }
  items.value = await app.attempt(() => api.notificationsList(f), 'Could not load notifications') ?? items.value
}

watch([filter, tab], () => tab.value !== 'preferences' && load(), { deep: true })
watch(() => store.revision, () => tab.value !== 'preferences' && load())
onActivated(() => {
  load()
  store.loadPrefs().then(resetDraft).catch(() => undefined)
})

function resetDraft() {
  if (!store.prefs) return
  draft.value = JSON.parse(JSON.stringify(store.prefs)) as NotificationPrefs
  quiet.value = store.prefs.quietHours ? { on: true, from: store.prefs.quietHours[0], to: store.prefs.quietHours[1] } : { on: false, from: '22:00', to: '07:00' }
}
watch(() => store.prefs, (p) => p && !draft.value && resetDraft(), { immediate: true })

async function savePrefs() {
  if (!draft.value) return
  const p = { ...draft.value, quietHours: quiet.value.on ? ([quiet.value.from, quiet.value.to] as [string, string]) : null }
  const ok = await app.attempt(() => store.savePrefs(p), 'Could not save preferences')
  if (ok !== undefined) {
    resetDraft()
    app.toast('success', 'Notification preferences saved')
  }
}
function toggleIn(list: string[], id: string) {
  const i = list.indexOf(id)
  if (i === -1) list.push(id)
  else list.splice(i, 1)
}
async function testNotification() {
  const ok = await app.attempt(() => api.notificationsTest(), 'Could not show a desktop notification')
  if (ok !== undefined) app.toast('info', 'Sent a test notification. If nothing appeared, check the system notification settings and Focus / Do Not Disturb.')
}
const categories = computed(() => Object.keys(categoryLabels) as NotificationCategory[])
const attentionBoard = computed(() => store.attention)
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>Notifications</h1>
        <p>What your Claude sessions, tasks and review queues need from you — across workspaces and profiles. NEXUS reports only what Claude Code signals; it never answers or approves for you.</p>
      </div>
      <div class="page-actions">
        <button class="btn" :disabled="store.counts.unread === 0" @click="store.markAllRead()">Mark all read</button>
      </div>
    </div>

    <section v-if="attentionBoard.length" class="board" aria-label="Live sessions">
      <button
        v-for="s in attentionBoard"
        :key="s.id"
        class="tile"
        :class="{ urgent: needsUser(s.status) }"
        :data-board-session="s.id"
        :title="s.status.certainty === 'assumed' ? 'No signal from Claude — NEXUS does not guess prompts' : (s.status.detail ?? '')"
        @click="sessions.openTab(s.id)"
      >
        <span class="tile-status" :data-status="s.status.status">{{ attentionLabels[s.status.status] }}{{ s.status.certainty === 'assumed' ? '?' : '' }}</span>
        <span class="tile-title truncate">{{ s.title }}</span>
        <span class="faint small truncate">{{ s.status.detail ?? (s.status.since ? relativeTime(s.status.since) : '') }}</span>
      </button>
    </section>

    <div class="tabs" role="tablist">
      <button v-for="t in (['inbox', 'history', 'preferences'] as Tab[])" :key="t" role="tab" class="tab" :class="{ active: tab === t }" :aria-selected="tab === t" @click="tab = t">
        {{ t === 'inbox' ? 'Inbox' : t === 'history' ? 'History' : 'Preferences' }}
      </button>
    </div>

    <template v-if="tab !== 'preferences'">
      <div class="filters">
        <select v-model="filter.scope" class="select" aria-label="Workspace scope">
          <option value="workspace">This workspace</option>
          <option value="all">All workspaces</option>
        </select>
        <select v-model="filter.projectId" class="select" aria-label="Project">
          <option value="">All projects</option>
          <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <select v-model="filter.profileId" class="select" aria-label="Profile">
          <option value="">All profiles</option>
          <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <select v-model="filter.sessionId" class="select" aria-label="Session">
          <option value="">All sessions</option>
          <option v-for="s in sessionOptions" :key="s.id" :value="s.id">{{ s.title }}</option>
        </select>
        <select v-model="filter.priority" class="select" aria-label="Priority">
          <option value="">All priorities</option>
          <option v-for="(label, p) in priorityLabels" :key="p" :value="p">{{ label }}</option>
        </select>
        <label class="checkbox"><input v-model="filter.unreadOnly" type="checkbox" /><span>Unread only</span></label>
      </div>
      <div v-if="items.length" class="list">
        <NotificationItem v-for="n in items" :key="n.id" :n="n" />
      </div>
      <div v-else class="empty">
        <BellOff class="empty-icon" />
        <h3>{{ tab === 'inbox' ? 'Nothing needs you' : 'No notifications yet' }}</h3>
        <p>Permission requests, questions, finished turns, task updates and review queues of NEXUS sessions appear here.</p>
      </div>
    </template>

    <div v-else-if="draft" class="prefs">
      <div class="card">
        <div class="card-title">Desktop notifications</div>
        <p v-if="store.availability && !store.availability.available" class="notice notice-warn"><Info /> {{ store.availability.reason }}</p>
        <p v-else-if="store.availability" class="hint">
          Shown by {{ store.availability.backend === 'windows' ? 'Windows' : store.availability.backend }}{{ store.availability.clickToOpen ? '; clicking one opens the session in NEXUS' : '' }}.
          Focus Assist / Do Not Disturb can still hide them.
        </p>
        <label class="checkbox"><input v-model="draft.desktopEnabled" type="checkbox" data-pref="desktop" /><span>Show desktop notifications</span></label>
        <label class="checkbox"><input v-model="draft.sound" type="checkbox" /><span>Play a sound</span></label>
        <label class="checkbox"><input v-model="draft.notifyFocusedSession" type="checkbox" /><span>Also for the session I am looking at</span></label>
        <div class="quiet">
          <label class="checkbox"><input v-model="quiet.on" type="checkbox" /><span>Quiet hours</span></label>
          <input v-model="quiet.from" class="input time" type="time" :disabled="!quiet.on" aria-label="Quiet from" />
          <span class="faint">to</span>
          <input v-model="quiet.to" class="input time" type="time" :disabled="!quiet.on" aria-label="Quiet until" />
          <span class="hint">Critical notifications still show; everything stays in the inbox.</span>
        </div>
        <div><button class="btn btn-sm" @click="testNotification"><Send /> Send a test notification</button></div>
      </div>

      <div class="card">
        <div class="card-title">Categories</div>
        <table class="cats">
          <thead><tr><th>Category</th><th>Inbox</th><th>Desktop</th></tr></thead>
          <tbody>
            <tr v-for="c in categories" :key="c" :data-category-pref="c">
              <td>{{ categoryLabels[c] }}<span v-if="c === 'workflow-failed'" class="faint"> (workflows — reserved)</span></td>
              <td><input v-model="draft.categories[c].inApp" type="checkbox" :aria-label="`${categoryLabels[c]} in the inbox`" /></td>
              <td><input v-model="draft.categories[c].desktop" type="checkbox" :disabled="!draft.categories[c].inApp" :aria-label="`${categoryLabels[c]} on the desktop`" /></td>
            </tr>
          </tbody>
        </table>
      </div>

      <div class="card">
        <div class="card-title">Mute desktop notifications for</div>
        <div class="mutes">
          <div>
            <div class="section-title">Workspaces</div>
            <label v-for="w in workspaces.list" :key="w.id" class="checkbox">
              <input type="checkbox" :checked="draft.mutedWorkspaces.includes(w.id)" @change="toggleIn(draft.mutedWorkspaces, w.id)" /><span>{{ w.name }}</span>
            </label>
          </div>
          <div>
            <div class="section-title">Claude profiles</div>
            <label v-for="p in profiles.list" :key="p.id" class="checkbox">
              <input type="checkbox" :checked="draft.mutedProfiles.includes(p.id)" @change="toggleIn(draft.mutedProfiles, p.id)" /><span>{{ p.name }}</span>
            </label>
          </div>
        </div>
      </div>
      <div class="row gap">
        <button class="btn btn-primary" data-testid="save-prefs" @click="savePrefs">Save preferences</button>
        <button class="btn" @click="resetDraft">Reset</button>
      </div>
      <p class="notice"><Info /> Repeats of the same situation are merged (×n) and each session is rate-limited, so a busy session cannot flood you. Notifications older than 30 days are deleted.</p>
    </div>
  </div>
</template>

<style scoped>
.board {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
  gap: 10px;
  margin-bottom: 16px;
}
.tile {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  text-align: left;
  cursor: pointer;
  min-width: 0;
}
.tile:hover {
  border-color: var(--border-strong);
}
.tile.urgent {
  border-color: color-mix(in srgb, var(--amber) 55%, var(--border));
}
.tile-status {
  font-size: 11.5px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--cyan);
}
.urgent .tile-status {
  color: var(--amber);
}
.tile-status[data-status='idle'] {
  color: var(--accent-strong);
}
.tile-title {
  font-weight: 600;
}
.small {
  font-size: 12px;
}
.tabs {
  display: flex;
  gap: 6px;
  margin-bottom: 12px;
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
.filters {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
.filters .select {
  width: auto;
  min-width: 140px;
}
.list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.prefs {
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-width: 760px;
}
.prefs .card {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.quiet {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
}
.time {
  width: 110px;
}
.cats {
  width: 100%;
  border-collapse: collapse;
}
.cats th {
  text-align: left;
  font-size: 12px;
  color: var(--text-faint);
  font-weight: 600;
  padding: 4px 6px;
}
.cats td {
  padding: 6px;
  border-top: 1px solid var(--border);
}
.mutes {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
}
.mutes > div {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.gap {
  gap: 8px;
}
</style>
