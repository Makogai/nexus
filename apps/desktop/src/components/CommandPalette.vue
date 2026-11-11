<script setup lang="ts">
import { computed, nextTick, ref, watch, type Component } from 'vue'
import { useRouter } from 'vue-router'
import {
  FolderGit2,
  House,
  Layers,
  Moon,
  PanelBottom,
  RotateCcw,
  Search,
  Settings,
  Sparkles,
  SquareTerminal,
  UserCog,
  History,
} from '@lucide/vue'
import { fuzzyFilter } from '../lib/fuzzy'
import { api } from '../lib/ipc'
import type { ShellInfo } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useSessionStore } from '../stores/sessions'
import { useWorkspaceStore } from '../stores/workspaces'

interface Command {
  id: string
  title: string
  group: string
  icon: Component
  hint?: string
  run: () => unknown
}

const app = useAppStore()
const projects = useProjectStore()
const sessions = useSessionStore()
const workspaces = useWorkspaceStore()
const router = useRouter()

const query = ref('')
const selected = ref(0)
const input = ref<HTMLInputElement>()
const shells = ref<ShellInfo[]>([])

watch(
  () => app.paletteOpen,
  async (open) => {
    if (!open) return
    query.value = ''
    selected.value = 0
    if (shells.value.length === 0) shells.value = await api.shellList().catch(() => [])
    await nextTick()
    input.value?.focus()
  },
)

const commands = computed<Command[]>(() => {
  const go = (path: string) => () => router.push(path)
  const list: Command[] = [
    { id: 'launch', title: 'Launch a session…', group: 'Actions', icon: Sparkles, hint: 'Ctrl+Shift+L', run: () => app.openLaunch() },
    { id: 'nav-home', title: 'Go to Home', group: 'Navigate', icon: House, run: go('/') },
    { id: 'nav-projects', title: 'Go to Projects', group: 'Navigate', icon: FolderGit2, run: go('/projects') },
    { id: 'nav-sessions', title: 'Go to Sessions', group: 'Navigate', icon: SquareTerminal, run: go('/sessions') },
    { id: 'nav-profiles', title: 'Go to Claude profiles', group: 'Navigate', icon: UserCog, run: go('/profiles') },
    { id: 'nav-settings', title: 'Open Settings', group: 'Navigate', icon: Settings, run: go('/settings') },
    {
      id: 'toggle-terminal',
      title: 'Toggle terminal panel',
      group: 'Actions',
      icon: PanelBottom,
      hint: 'Ctrl+`',
      run: () => (app.prefs.terminalOpen = !app.prefs.terminalOpen),
    },
    {
      id: 'toggle-theme',
      title: `Switch to ${app.resolvedTheme === 'dark' ? 'light' : 'dark'} theme`,
      group: 'Actions',
      icon: Moon,
      run: () => (app.prefs.theme = app.resolvedTheme === 'dark' ? 'light' : 'dark'),
    },
  ]
  for (const s of shells.value) {
    list.push({
      id: `shell-${s.id}`,
      title: `Open ${s.name}`,
      group: 'Terminal',
      icon: SquareTerminal,
      run: () => app.attempt(() => sessions.openShell({ shellId: s.id }), 'Could not open shell'),
    })
  }
  for (const p of projects.list) {
    list.push({ id: `project-${p.id}`, title: p.name, group: 'Projects', icon: FolderGit2, hint: p.stack.stacks.slice(0, 2).join(' · '), run: go(`/projects/${p.id}`) })
    if (p.available) {
      list.push({ id: `launch-${p.id}`, title: `Launch in ${p.name}`, group: 'Projects', icon: Sparkles, run: () => app.openLaunch(p.id) })
      list.push({ id: `resume-${p.id}`, title: `Resume ${p.name}`, group: 'Projects', icon: History, run: () => router.push(`/projects/${p.id}/resume`) })
    }
  }
  for (const s of sessions.resumable.slice(0, 20)) {
    list.push({
      id: `resume-${s.id}`,
      title: `Resume “${s.title}”`,
      group: 'Sessions',
      icon: RotateCcw,
      run: () => app.attempt(() => sessions.resume(s.id), 'Could not resume'),
    })
  }
  for (const w of workspaces.list) {
    if (w.id === workspaces.active?.id) continue
    list.push({ id: `ws-${w.id}`, title: `Switch to workspace ${w.name}`, group: 'Workspaces', icon: Layers, run: () => app.attempt(() => workspaces.activate(w.id), 'Could not switch workspace') })
  }
  return list
})

const results = computed(() => fuzzyFilter(commands.value, query.value, (c) => `${c.title} ${c.group}`, 40))

watch(query, () => (selected.value = 0))

function close() {
  app.paletteOpen = false
}
function run(c: Command | undefined) {
  if (!c) return
  close()
  c.run()
}
function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    selected.value = Math.min(results.value.length - 1, selected.value + 1)
    e.preventDefault()
  } else if (e.key === 'ArrowUp') {
    selected.value = Math.max(0, selected.value - 1)
    e.preventDefault()
  } else if (e.key === 'Enter') {
    run(results.value[selected.value])
    e.preventDefault()
  } else if (e.key === 'Escape') {
    close()
    e.preventDefault()
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="app.paletteOpen" class="backdrop" @mousedown.self="close">
      <div class="palette" role="dialog" aria-label="Command palette">
        <div class="search">
          <Search />
          <input
            ref="input"
            v-model="query"
            placeholder="Type a command, project or session…"
            spellcheck="false"
            aria-label="Search commands"
            @keydown="onKey"
          />
          <span class="kbd">Esc</span>
        </div>
        <ul class="results" role="listbox">
          <li
            v-for="(c, i) in results"
            :key="c.id"
            role="option"
            :aria-selected="i === selected"
            :class="{ selected: i === selected }"
            @mousemove="selected = i"
            @click="run(c)"
          >
            <component :is="c.icon" class="icon" />
            <span class="title truncate">{{ c.title }}</span>
            <span v-if="c.hint" class="faint hint">{{ c.hint }}</span>
            <span class="group">{{ c.group }}</span>
          </li>
          <li v-if="results.length === 0" class="none">No matches</li>
        </ul>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.backdrop {
  position: fixed;
  inset: 0;
  z-index: 55;
  background: rgba(5, 6, 10, 0.45);
  display: flex;
  justify-content: center;
  padding-top: 12vh;
}
.palette {
  width: 640px;
  max-width: calc(100vw - 48px);
  height: fit-content;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  border-radius: 16px;
  box-shadow: var(--shadow);
  overflow: hidden;
}
.search {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 16px;
  height: 54px;
  border-bottom: 1px solid var(--border);
}
.search svg {
  width: 18px;
  height: 18px;
  color: var(--text-faint);
}
.search input {
  flex: 1;
  border: none;
  outline: none;
  background: transparent;
  font-size: 15px;
}
.results {
  list-style: none;
  margin: 0;
  padding: 6px;
  max-height: 52vh;
  overflow: auto;
}
.results li {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border-radius: 9px;
  cursor: pointer;
}
.results li.selected {
  background: var(--accent-soft);
}
.icon {
  width: 16px;
  height: 16px;
  color: var(--text-muted);
  flex: none;
}
.selected .icon {
  color: var(--accent-strong);
}
.title {
  flex: 1;
}
.hint {
  font-size: 12px;
}
.group {
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--text-faint);
  width: 84px;
  text-align: right;
}
.none {
  color: var(--text-faint);
  justify-content: center;
}
</style>
