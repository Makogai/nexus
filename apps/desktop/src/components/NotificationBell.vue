<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Bell, BellRing, CheckCheck } from '@lucide/vue'
import { attentionLabels, needsUser } from '../lib/format'
import { useNotificationStore } from '../stores/notifications'
import { useSessionStore } from '../stores/sessions'
import NotificationItem from './NotificationItem.vue'

const store = useNotificationStore()
const sessions = useSessionStore()
const router = useRouter()
const root = ref<HTMLElement>()

const waiting = computed(() => store.attention.filter((s) => s.status.status !== 'running'))
const latest = computed(() => store.recent.slice(0, 12))

function onDocClick(e: MouseEvent) {
  if (store.centerOpen && !root.value?.contains(e.target as Node)) store.centerOpen = false
}
onMounted(() => document.addEventListener('mousedown', onDocClick))
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))

function openSession(id: string) {
  store.centerOpen = false
  sessions.openTab(id)
}
function openCenter() {
  store.centerOpen = false
  router.push({ name: 'notifications' })
}
</script>

<template>
  <div ref="root" class="bell-root">
    <button
      class="bell"
      :class="{ action: store.counts.actionRequired > 0 }"
      :aria-expanded="store.centerOpen"
      :aria-label="`Notifications: ${store.counts.unread} unread, ${store.counts.actionRequired} need action`"
      data-testid="bell"
      @click="store.centerOpen = !store.centerOpen"
    >
      <BellRing v-if="store.counts.actionRequired > 0" />
      <Bell v-else />
      <span v-if="store.counts.actionRequired > 0" class="count count-action" data-testid="bell-action">{{ store.counts.actionRequired }}</span>
      <span v-else-if="store.counts.unread > 0" class="count" data-testid="bell-unread">{{ store.counts.unread }}</span>
    </button>

    <div v-if="store.centerOpen" class="panel" role="dialog" aria-label="Notification center">
      <div class="head">
        <strong>Notifications</strong>
        <span class="faint small">{{ store.counts.unread }} unread · {{ store.counts.actionRequired }} need action</span>
        <span class="spacer" />
        <button class="btn btn-sm btn-ghost" :disabled="store.counts.unread === 0" @click="store.markAllRead()"><CheckCheck /> Mark all read</button>
      </div>

      <div v-if="waiting.length" class="section">
        <div class="section-label">Sessions</div>
        <button v-for="s in waiting" :key="s.id" class="session-line" :data-attention-session="s.id" @click="openSession(s.id)">
          <span class="badge" :class="{ 'badge-warn': needsUser(s.status), 'badge-accent': s.status.status === 'idle' }">
            {{ attentionLabels[s.status.status] }}
          </span>
          <span class="truncate">{{ s.title }}</span>
          <span v-if="s.status.detail" class="faint small truncate">{{ s.status.detail }}</span>
        </button>
      </div>

      <div class="list-wrap">
        <NotificationItem v-for="n in latest" :key="n.id" :n="n" compact />
        <p v-if="latest.length === 0" class="faint empty-line">Nothing needs you right now.</p>
      </div>
      <button class="footer" @click="openCenter">Open notification center</button>
    </div>
  </div>
</template>

<style scoped>
.bell-root {
  position: relative;
}
.bell {
  position: relative;
  width: 36px;
  height: 34px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  border: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: pointer;
  color: var(--text-muted);
}
.bell:hover {
  border-color: var(--border-strong);
}
.bell.action {
  color: var(--amber);
}
.bell svg {
  width: 17px;
  height: 17px;
}
.count {
  position: absolute;
  top: -6px;
  right: -6px;
  min-width: 18px;
  height: 18px;
  padding: 0 5px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 700;
  line-height: 18px;
  background: var(--accent);
  color: white;
}
.count-action {
  background: var(--amber);
  color: #1b1300;
}
.panel {
  position: absolute;
  right: 0;
  top: 42px;
  z-index: 50;
  width: min(440px, 92vw);
  max-height: min(640px, 80vh);
  display: flex;
  flex-direction: column;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 14px;
  box-shadow: var(--shadow);
  overflow: hidden;
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 12px 8px 14px;
}
.small {
  font-size: 12px;
}
.section {
  padding: 4px 10px 8px;
  border-bottom: 1px solid var(--border);
}
.section-label {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint);
  padding: 4px 4px 6px;
}
.session-line {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 4px;
  border: none;
  border-radius: 8px;
  background: transparent;
  cursor: pointer;
  text-align: left;
}
.session-line:hover {
  background: var(--bg-hover);
}
.list-wrap {
  flex: 1;
  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px;
}
.empty-line {
  text-align: center;
  padding: 18px 0;
}
.footer {
  border: none;
  border-top: 1px solid var(--border);
  background: transparent;
  padding: 10px;
  cursor: pointer;
  color: var(--accent-strong);
  font-weight: 600;
}
.footer:hover {
  background: var(--bg-hover);
}
</style>
