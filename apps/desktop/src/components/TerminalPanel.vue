<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { ChevronDown, ChevronUp, Maximize2, Minimize2, Plus, Sparkles, SquareTerminal, X } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { ShellInfo } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'
import TerminalView from './TerminalView.vue'

const props = defineProps<{ fill: boolean }>()
const app = useAppStore()
const sessions = useSessionStore()

const shells = ref<ShellInfo[]>([])
const newMenu = ref(false)
const newMenuRoot = ref<HTMLElement>()

const open = computed(() => app.prefs.terminalOpen)
const height = computed(() => (props.fill ? undefined : `${app.prefs.terminalHeight}px`))

onMounted(async () => {
  shells.value = await api.shellList().catch(() => [])
  document.addEventListener('mousedown', onDocClick)
})
onBeforeUnmount(() => document.removeEventListener('mousedown', onDocClick))

function onDocClick(e: MouseEvent) {
  if (newMenu.value && !newMenuRoot.value?.contains(e.target as Node)) newMenu.value = false
}

async function openShell(shellId?: string) {
  newMenu.value = false
  await app.attempt(() => sessions.openShell({ shellId: shellId ?? app.prefs.defaultShell }), 'Could not open a shell')
}

function startDrag(e: MouseEvent) {
  if (props.fill) return
  e.preventDefault()
  const startY = e.clientY
  const startH = app.prefs.terminalHeight
  const max = window.innerHeight - 180
  const move = (ev: MouseEvent) => {
    app.prefs.terminalHeight = Math.min(max, Math.max(140, startH + (startY - ev.clientY)))
  }
  const up = () => {
    window.removeEventListener('mousemove', move)
    window.removeEventListener('mouseup', up)
    document.body.style.cursor = ''
  }
  document.body.style.cursor = 'ns-resize'
  window.addEventListener('mousemove', move)
  window.addEventListener('mouseup', up)
}

function tone(id: string) {
  const s = sessions.byId[id]
  if (!s) return ''
  if (s.running) return 'dot-live'
  if (s.state === 'failed') return 'dot-error'
  return s.exitCode === 0 ? '' : 'dot-warn'
}
</script>

<template>
  <!-- With no terminal open the panel is a compact strip, so it does not take half of every view. -->
  <section class="panel" :class="{ open, fill, 'no-tabs': sessions.tabs.length === 0 }" :style="open && sessions.tabs.length ? { height } : undefined" aria-label="Terminals">
    <div v-if="open && !fill && sessions.tabs.length" class="drag" title="Drag to resize" @mousedown="startDrag" />
    <div class="bar">
      <div class="tabs" role="tablist">
        <button
          v-for="id in sessions.tabs"
          :key="id"
          class="tab"
          role="tab"
          :data-tab-session="id"
          :aria-selected="id === sessions.activeTab"
          :class="{ active: id === sessions.activeTab }"
          @click="(sessions.activeTab = id), (app.prefs.terminalOpen = true)"
          @auxclick.middle="sessions.closeTab(id)"
        >
          <span class="dot" :class="tone(id)" />
          <Sparkles v-if="sessions.byId[id]?.kind === 'claude'" class="kind" />
          <SquareTerminal v-else class="kind" />
          <span class="truncate title">{{ sessions.byId[id]?.title ?? 'Terminal' }}</span>
          <span class="close" role="button" aria-label="Close tab" @click.stop="sessions.closeTab(id)"><X /></span>
        </button>
        <span v-if="sessions.tabs.length === 0" class="faint empty-label">Terminal</span>
      </div>

      <div ref="newMenuRoot" class="new">
        <button class="btn btn-ghost btn-sm btn-icon" title="New terminal" @click="newMenu = !newMenu"><Plus /></button>
        <div v-if="newMenu" class="menu">
          <button class="menu-item" @click="(newMenu = false), app.openLaunch()"><Sparkles /> Launch Claude…</button>
          <div class="sep" />
          <button v-for="s in shells" :key="s.id" class="menu-item" @click="openShell(s.id)">
            <SquareTerminal /> {{ s.name }}
          </button>
          <div v-if="shells.length === 0" class="menu-item faint">No shells detected</div>
        </div>
      </div>
      <button
        v-if="sessions.tabs.length > 0"
        class="btn btn-ghost btn-sm btn-icon"
        :title="sessions.focusMode ? 'Restore layout' : 'Maximize terminal'"
        @click="(sessions.focusMode = !sessions.focusMode), (app.prefs.terminalOpen = true)"
      >
        <Minimize2 v-if="sessions.focusMode" />
        <Maximize2 v-else />
      </button>
      <button
        class="btn btn-ghost btn-sm btn-icon"
        :title="open ? 'Hide terminal (Ctrl+`)' : 'Show terminal (Ctrl+`)'"
        @click="app.prefs.terminalOpen = !open"
      >
        <ChevronDown v-if="open" />
        <ChevronUp v-else />
      </button>
    </div>

    <div v-show="open" class="body">
      <TerminalView
        v-for="id in sessions.tabs"
        v-show="id === sessions.activeTab"
        :key="id"
        :session-id="id"
        :active="open && id === sessions.activeTab"
      />
      <div v-if="sessions.tabs.length === 0" class="placeholder">
        <p>No terminals open.</p>
        <div class="row">
          <button class="btn btn-sm" @click="openShell()"><SquareTerminal /> Open shell</button>
          <button class="btn btn-sm btn-primary" @click="app.openLaunch()"><Sparkles /> Launch Claude</button>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.panel {
  position: relative;
  flex: none;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--border);
  background: var(--bg-raised);
  min-height: 36px;
}
.panel.fill {
  flex: 1;
}
.drag {
  position: absolute;
  top: -4px;
  left: 0;
  right: 0;
  height: 8px;
  cursor: ns-resize;
  z-index: 3;
}
.bar {
  display: flex;
  align-items: center;
  gap: 4px;
  height: 36px;
  padding: 0 8px 0 6px;
  border-bottom: 1px solid var(--border);
  flex: none;
}
.tabs {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.tab {
  display: flex;
  align-items: center;
  gap: 7px;
  height: 28px;
  max-width: 240px;
  padding: 0 6px 0 10px;
  border-radius: 7px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 12.5px;
  flex: none;
}
.tab:hover {
  background: var(--bg-hover);
}
.tab.active {
  background: var(--bg-panel);
  border-color: var(--border-strong);
  color: var(--text);
}
.tab .kind {
  width: 13px;
  height: 13px;
  flex: none;
}
.tab .title {
  min-width: 0;
}
.close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 4px;
  opacity: 0.55;
}
.close:hover {
  opacity: 1;
  background: var(--bg-active);
}
.close svg {
  width: 12px;
  height: 12px;
}
.empty-label {
  padding-left: 8px;
  font-size: 12.5px;
}
.new {
  position: relative;
}
.menu {
  position: absolute;
  bottom: 34px;
  right: 0;
  z-index: 30;
  width: 230px;
  padding: 6px;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 12px;
  box-shadow: var(--shadow);
}
.menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 10px;
  border-radius: 8px;
  border: none;
  background: transparent;
  text-align: left;
  cursor: pointer;
  font-size: 13px;
}
.menu-item:hover {
  background: var(--bg-hover);
}
.menu-item svg {
  width: 14px;
  height: 14px;
}
.sep {
  height: 1px;
  background: var(--border);
  margin: 4px;
}
.body {
  flex: 1;
  min-height: 0;
  position: relative;
}
.placeholder {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  color: var(--text-faint);
}
.placeholder p {
  margin: 0;
}
/* Empty and open: one line under the bar instead of a tall empty area. */
.panel.open.no-tabs:not(.fill) .placeholder {
  height: auto;
  flex-direction: row;
  justify-content: flex-start;
  padding: 8px 14px 10px;
  font-size: 12.5px;
}
</style>
