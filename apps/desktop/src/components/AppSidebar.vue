<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { Bell, BookOpen, FolderGit2, Gauge, House, Landmark, Package, Workflow, PanelLeftClose, PanelLeftOpen, Settings, SquareTerminal, UserCog } from '@lucide/vue'
import { useAppStore } from '../stores/app'
import { useNotificationStore } from '../stores/notifications'
import { useSessionStore } from '../stores/sessions'

const app = useAppStore()
const sessions = useSessionStore()
const notifications = useNotificationStore()
const route = useRoute()

const items = computed(() => [
  { to: '/', label: 'Home', icon: House, match: (p: string) => p === '/' },
  { to: '/projects', label: 'Projects', icon: FolderGit2, match: (p: string) => p.startsWith('/projects') },
  { to: '/knowledge', label: 'Knowledge', icon: BookOpen, match: (p: string) => p.startsWith('/knowledge') },
  { to: '/constitutions', label: 'Constitutions', icon: Landmark, match: (p: string) => p.startsWith('/constitutions') },
  { to: '/context', label: 'Context', icon: Gauge, match: (p: string) => p.startsWith('/context') },
  { to: '/workflows', label: 'Workflows', icon: Workflow, match: (p: string) => p.startsWith('/workflows') },
  {
    to: '/sessions',
    label: 'Sessions',
    icon: SquareTerminal,
    match: (p: string) => p.startsWith('/sessions'),
    badge: sessions.running.length || undefined,
  },
  {
    to: '/notifications',
    label: 'Notifications',
    icon: Bell,
    match: (p: string) => p.startsWith('/notifications'),
    badge: notifications.waitingCount || undefined,
  },
  { to: '/profiles', label: 'Claude profiles', icon: UserCog, match: (p: string) => p.startsWith('/profiles') },
  { to: '/kits', label: 'AI Kits', icon: Package, match: (p: string) => p.startsWith('/kits') },
])
</script>

<template>
  <nav class="sidebar" aria-label="Main">
    <div class="brand">
      <svg viewBox="0 0 1024 1024" class="logo" aria-hidden="true">
        <defs>
          <linearGradient id="sb-g" x1="0" y1="1" x2="1" y2="0">
            <stop offset="0" stop-color="#7c6cf2" />
            <stop offset="1" stop-color="#3ad1c5" />
          </linearGradient>
        </defs>
        <path d="M332 712 V312 L692 712 V312" fill="none" stroke="url(#sb-g)" stroke-width="88" stroke-linecap="round" stroke-linejoin="round" />
        <circle cx="692" cy="312" r="80" fill="#3ad1c5" />
        <circle cx="332" cy="712" r="80" fill="#7c6cf2" />
      </svg>
      <span v-if="!app.prefs.sidebarCollapsed" class="name">NEXUS</span>
    </div>

    <ul>
      <li v-for="item in items" :key="item.to">
        <RouterLink :to="item.to" class="nav-item" :class="{ active: item.match(route.path) }" :title="item.label">
          <component :is="item.icon" />
          <span v-if="!app.prefs.sidebarCollapsed" class="label">{{ item.label }}</span>
          <span v-if="item.badge" class="nav-badge">{{ item.badge }}</span>
        </RouterLink>
      </li>
    </ul>

    <div class="spacer" />
    <ul>
      <li>
        <RouterLink to="/settings" class="nav-item" :class="{ active: route.path.startsWith('/settings') }" title="Settings">
          <Settings />
          <span v-if="!app.prefs.sidebarCollapsed" class="label">Settings</span>
        </RouterLink>
      </li>
      <li>
        <button
          class="nav-item"
          :title="app.prefs.sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'"
          @click="app.prefs.sidebarCollapsed = !app.prefs.sidebarCollapsed"
        >
          <PanelLeftOpen v-if="app.prefs.sidebarCollapsed" />
          <PanelLeftClose v-else />
          <span v-if="!app.prefs.sidebarCollapsed" class="label">Collapse</span>
        </button>
      </li>
    </ul>
  </nav>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  padding: 14px 10px;
  background: var(--bg-raised);
  border-right: 1px solid var(--border);
  min-height: 0;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 2px 8px 18px;
}
.logo {
  width: 28px;
  height: 28px;
  flex: none;
}
.name {
  font-weight: 800;
  letter-spacing: 0.16em;
  font-size: 15px;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  height: 38px;
  padding: 0 12px;
  border-radius: 9px;
  border: none;
  background: transparent;
  color: var(--text-muted);
  text-decoration: none;
  font-weight: 550;
  cursor: pointer;
  position: relative;
}
.nav-item svg {
  width: 18px;
  height: 18px;
  flex: none;
}
.nav-item:hover {
  background: var(--bg-hover);
  color: var(--text);
}
.nav-item.active {
  background: var(--accent-soft);
  color: var(--text);
}
.nav-item.active::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 9px;
  bottom: 9px;
  width: 3px;
  border-radius: 0 3px 3px 0;
  background: var(--accent);
}
.nav-item.active svg {
  color: var(--accent-strong);
}
.label {
  flex: 1;
  text-align: left;
}
.nav-badge {
  min-width: 20px;
  height: 20px;
  padding: 0 6px;
  border-radius: 10px;
  background: var(--cyan-soft);
  color: var(--cyan);
  font-size: 11px;
  font-weight: 700;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.spacer {
  flex: 1;
}
</style>
