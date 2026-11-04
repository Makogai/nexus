<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { Check, ChevronDown, Layers, Plus, Search, Sparkles } from '@lucide/vue'
import { useAppStore } from '../stores/app'
import { useWorkspaceStore } from '../stores/workspaces'
import { useProfileStore } from '../stores/profiles'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const profiles = useProfileStore()
const router = useRouter()

const menuOpen = ref(false)
const menuRoot = ref<HTMLElement>()

const lastProfile = computed(() => {
  const id = app.prefs.lastLaunch?.profileId
  return (id && profiles.byId[id]) || profiles.list[0] || null
})
const lastWorkProfile = computed(() => {
  const id = app.prefs.lastLaunch?.workProfile ?? profiles.launchOptions?.defaultWorkProfile
  return profiles.launchOptions?.workProfiles.find((w) => w.id === id) ?? null
})

async function switchTo(id: string) {
  menuOpen.value = false
  if (id === workspaces.active?.id) return
  await app.attempt(() => workspaces.activate(id), 'Could not switch workspace')
}

function onDocClick(e: MouseEvent) {
  if (menuOpen.value && !menuRoot.value?.contains(e.target as Node)) menuOpen.value = false
}
onMounted(() => document.addEventListener('mousedown', onDocClick))
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))
</script>

<template>
  <header class="topbar">
    <div ref="menuRoot" class="ws">
      <button class="ws-button" :aria-expanded="menuOpen" @click="menuOpen = !menuOpen">
        <Layers />
        <span class="ws-name truncate">{{ workspaces.active?.name }}</span>
        <ChevronDown class="chev" />
      </button>
      <div v-if="menuOpen" class="menu" role="menu">
        <div class="menu-label">Workspaces</div>
        <button v-for="w in workspaces.list" :key="w.id" class="menu-item" role="menuitem" @click="switchTo(w.id)">
          <span class="truncate">{{ w.name }}</span>
          <span class="faint small">{{ w.projectCount }} projects</span>
          <Check v-if="w.id === workspaces.active?.id" class="check" />
        </button>
        <div class="menu-sep" />
        <button class="menu-item" role="menuitem" @click="(menuOpen = false), router.push('/settings')">
          <Plus /> <span>New or import workspace…</span>
        </button>
      </div>
    </div>

    <button class="palette-trigger" @click="app.paletteOpen = true">
      <Search />
      <span>Search projects, sessions, commands…</span>
      <span class="kbd">Ctrl K</span>
    </button>

    <div class="spacer" />

    <button v-if="lastProfile" class="context" title="Change launch defaults" @click="app.openLaunch()">
      <span class="pdot" :style="{ background: lastProfile.color }" />
      <span class="truncate">{{ lastProfile.name }}</span>
      <span v-if="lastWorkProfile" class="faint">· {{ lastWorkProfile.name }}</span>
    </button>
    <button class="btn btn-primary" @click="app.openLaunch()">
      <Sparkles /> Launch Claude
    </button>
  </header>
</template>

<style scoped>
.topbar {
  height: var(--topbar-h);
  flex: none;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 0 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
}
.ws {
  position: relative;
}
.ws-button {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  max-width: 260px;
  padding: 0 10px;
  border-radius: 9px;
  border: 1px solid var(--border);
  background: var(--bg-panel);
  cursor: pointer;
  font-weight: 600;
}
.ws-button:hover {
  border-color: var(--border-strong);
}
.ws-button svg {
  width: 16px;
  height: 16px;
  color: var(--accent-strong);
  flex: none;
}
.ws-button .chev {
  color: var(--text-faint);
}
.menu {
  position: absolute;
  top: 40px;
  left: 0;
  z-index: 40;
  width: 300px;
  padding: 6px;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 12px;
  box-shadow: var(--shadow);
}
.menu-label {
  padding: 6px 10px;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint);
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 10px;
  border-radius: 8px;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
}
.menu-item:hover {
  background: var(--bg-hover);
}
.menu-item span:first-child {
  flex: 1;
}
.menu-item svg {
  width: 15px;
  height: 15px;
}
.menu-item .check {
  color: var(--accent-strong);
}
.small {
  font-size: 12px;
}
.menu-sep {
  height: 1px;
  background: var(--border);
  margin: 6px 4px;
}
.palette-trigger {
  display: flex;
  align-items: center;
  gap: 10px;
  width: min(420px, 36vw);
  height: 34px;
  padding: 0 10px;
  border-radius: 9px;
  border: 1px solid var(--border);
  background: var(--bg-input);
  color: var(--text-faint);
  cursor: pointer;
}
.palette-trigger:hover {
  border-color: var(--border-strong);
}
.palette-trigger svg {
  width: 15px;
  height: 15px;
}
.palette-trigger span:nth-child(2) {
  flex: 1;
  text-align: left;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.spacer {
  flex: 1;
}
.context {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 260px;
  height: 34px;
  padding: 0 12px;
  border-radius: 9px;
  border: 1px solid var(--border);
  background: transparent;
  cursor: pointer;
  font-weight: 550;
}
.context:hover {
  background: var(--bg-hover);
}
.pdot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex: none;
}
</style>
