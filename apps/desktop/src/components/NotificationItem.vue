<script setup lang="ts">
import { computed, ref } from 'vue'
import { AlarmClock, AlertTriangle, Bell, Check, CircleCheck, Info, ShieldQuestion, X } from '@lucide/vue'
import type { AppNotification } from '../lib/types'
import { categoryLabels, relativeTime } from '../lib/format'
import { useAppStore } from '../stores/app'
import { useNotificationStore } from '../stores/notifications'
import { useProfileStore } from '../stores/profiles'
import { useProjectStore } from '../stores/projects'

const props = withDefaults(defineProps<{ n: AppNotification; compact?: boolean }>(), { compact: false })
const app = useAppStore()
const store = useNotificationStore()
const profiles = useProfileStore()
const projects = useProjectStore()
const snoozing = ref(false)

const icon = computed(() => {
  if (props.n.category === 'permission-requested') return ShieldQuestion
  switch (props.n.priority) {
    case 'critical':
    case 'warning':
      return AlertTriangle
    case 'action':
      return Bell
    case 'success':
      return CircleCheck
    default:
      return Info
  }
})
const profile = computed(() => (props.n.profileId ? profiles.byId[props.n.profileId] : null))
const project = computed(() => (props.n.projectId ? projects.byId[props.n.projectId] : null))
const unread = computed(() => !props.n.readAt)
const canOpen = computed(() => !!props.n.targetKind)

async function snooze(minutes: number) {
  snoozing.value = false
  await app.attempt(() => store.snooze(props.n.id, minutes), 'Could not snooze')
}
</script>

<template>
  <div
    class="notif"
    :class="[`p-${n.priority}`, { unread, compact, resolved: n.resolvedAt }]"
    :data-notification="n.id"
    :data-category="n.category"
  >
    <component :is="icon" class="icon" />
    <button class="body" :disabled="!canOpen" :title="canOpen ? 'Open' : undefined" @click="store.open(n)">
      <span class="title">
        {{ n.title }}<span v-if="n.occurrences > 1" class="faint"> ×{{ n.occurrences }}</span>
      </span>
      <span v-if="n.body && !compact" class="text">{{ n.body }}</span>
      <span class="meta">
        <span>{{ categoryLabels[n.category] ?? n.category }}</span>
        <template v-if="project"><span>·</span><span>{{ project.name }}</span></template>
        <template v-if="profile">
          <span>·</span><span class="row"><span class="pdot" :style="{ background: profile.color }" />{{ profile.name }}</span>
        </template>
        <span>·</span><span>{{ relativeTime(n.updatedAt) }}</span>
        <template v-if="n.resolvedAt"><span>·</span><span class="ok">handled</span></template>
      </span>
    </button>
    <div class="actions">
      <button v-if="unread" class="btn btn-icon btn-sm btn-ghost" title="Mark as read" aria-label="Mark as read" @click="store.markRead([n.id])"><Check /></button>
      <div class="snooze">
        <button class="btn btn-icon btn-sm btn-ghost" title="Snooze" aria-label="Snooze" @click="snoozing = !snoozing"><AlarmClock /></button>
        <div v-if="snoozing" class="snooze-menu" role="menu">
          <button role="menuitem" @click="snooze(15)">15 minutes</button>
          <button role="menuitem" @click="snooze(60)">1 hour</button>
          <button role="menuitem" @click="snooze(240)">4 hours</button>
          <button role="menuitem" @click="snooze(24 * 60)">Tomorrow</button>
        </div>
      </div>
      <button class="btn btn-icon btn-sm btn-ghost" title="Dismiss" aria-label="Dismiss" @click="store.dismiss([n.id])"><X /></button>
    </div>
  </div>
</template>

<style scoped>
.notif {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px 10px 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  background: var(--bg-panel);
  position: relative;
}
.notif.compact {
  padding: 8px 8px 8px 10px;
}
.notif.unread::before {
  content: '';
  position: absolute;
  left: 0;
  top: 10px;
  bottom: 10px;
  width: 3px;
  border-radius: 2px;
  background: var(--accent);
}
.notif.resolved {
  opacity: 0.7;
}
.icon {
  width: 16px;
  height: 16px;
  flex: none;
  margin-top: 2px;
  color: var(--text-faint);
}
.p-action .icon,
.p-critical .icon {
  color: var(--amber);
}
.p-critical .icon {
  color: var(--red);
}
.p-warning .icon {
  color: var(--amber);
}
.p-success .icon {
  color: var(--green);
}
.body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
  text-align: left;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: inherit;
}
.body:disabled {
  cursor: default;
}
.title {
  font-weight: 600;
  overflow-wrap: anywhere;
}
.unread .title {
  color: var(--text);
}
.text {
  color: var(--text-muted);
  font-size: 12.5px;
  overflow-wrap: anywhere;
}
.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  font-size: 11.5px;
  color: var(--text-faint);
}
.meta .ok {
  color: var(--green);
}
.pdot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  display: inline-block;
  margin-right: 4px;
}
.actions {
  display: flex;
  gap: 2px;
  flex: none;
}
.snooze {
  position: relative;
}
.snooze-menu {
  position: absolute;
  right: 0;
  top: 30px;
  z-index: 60;
  display: flex;
  flex-direction: column;
  min-width: 130px;
  padding: 4px;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 10px;
  box-shadow: var(--shadow);
}
.snooze-menu button {
  text-align: left;
  padding: 6px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
}
.snooze-menu button:hover {
  background: var(--bg-hover);
}
</style>
