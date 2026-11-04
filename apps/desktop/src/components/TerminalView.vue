<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Terminal, type ITheme } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import { SearchAddon } from '@xterm/addon-search'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { openUrl } from '@tauri-apps/plugin-opener'
import { ArrowDown, ArrowUp, X } from '@lucide/vue'
import '@xterm/xterm/css/xterm.css'
import { api, toIpcError } from '../lib/ipc'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'

const props = defineProps<{ sessionId: string; active: boolean }>()

const app = useAppStore()
const sessions = useSessionStore()
const host = ref<HTMLDivElement>()
const searchOpen = ref(false)
const searchQuery = ref('')
const searchInput = ref<HTMLInputElement>()
const attachError = ref<string | null>(null)

let term: Terminal | null = null
let fit: FitAddon | null = null
let search: SearchAddon | null = null
let observer: ResizeObserver | null = null
let fitTimer: ReturnType<typeof setTimeout> | undefined
const noop = () => undefined

const DARK: ITheme = {
  background: '#0b0d12',
  foreground: '#dadded',
  cursor: '#9b8dff',
  cursorAccent: '#0b0d12',
  selectionBackground: 'rgba(124, 108, 242, 0.35)',
  black: '#1b1f2c',
  red: '#f7768e',
  green: '#7dd99a',
  yellow: '#f5c26b',
  blue: '#7aa2f7',
  magenta: '#bb9af7',
  cyan: '#56d4c8',
  white: '#c9cede',
  brightBlack: '#545c78',
  brightRed: '#ff8fa3',
  brightGreen: '#9cefb6',
  brightYellow: '#ffd68a',
  brightBlue: '#9ab8ff',
  brightMagenta: '#d0b5ff',
  brightCyan: '#7ee8de',
  brightWhite: '#f2f4fa',
}
const LIGHT: ITheme = {
  background: '#ffffff',
  foreground: '#1e2232',
  cursor: '#6a59ea',
  cursorAccent: '#ffffff',
  selectionBackground: 'rgba(106, 89, 234, 0.22)',
  black: '#1e2232',
  red: '#c9374f',
  green: '#1f8a4c',
  yellow: '#9a6a08',
  blue: '#2f5fd0',
  magenta: '#8241c7',
  cyan: '#0d8a80',
  white: '#a8adbd',
  brightBlack: '#6b7186',
  brightRed: '#e0485f',
  brightGreen: '#27a35c',
  brightYellow: '#b47d0c',
  brightBlue: '#4174e6',
  brightMagenta: '#9654de',
  brightCyan: '#10a397',
  brightWhite: '#e9ebf2',
}

function fitNow() {
  if (!term || !fit || !host.value) return
  if (host.value.offsetWidth < 20 || host.value.offsetHeight < 20) return // hidden tab
  try {
    fit.fit()
  } catch {
    /* renderer not ready */
  }
}
function fitSoon() {
  clearTimeout(fitTimer)
  fitTimer = setTimeout(fitNow, 30)
}

function copySelection(): boolean {
  if (!term?.hasSelection()) return false
  navigator.clipboard?.writeText(term.getSelection()).catch(noop)
  term.clearSelection()
  return true
}

function keyHandler(e: KeyboardEvent): boolean {
  if (e.type !== 'keydown') return true
  const mod = e.ctrlKey || e.metaKey
  const key = e.key.toLowerCase()
  // Copy: Ctrl+Shift+C, or Ctrl+C while text is selected (otherwise Ctrl+C is ^C for the program).
  if (mod && key === 'c' && (e.shiftKey || term?.hasSelection())) {
    copySelection()
    e.preventDefault()
    return false
  }
  // Paste: let the browser raise a paste event, which xterm turns into (bracketed) input.
  if (mod && key === 'v') return false
  if (mod && e.shiftKey && key === 'f') {
    openSearch()
    e.preventDefault()
    return false
  }
  // App-level shortcuts bubble to the window handler.
  if (mod && (key === '`' || (e.shiftKey && (key === 'p' || key === 'l')))) return false
  return true
}

async function openSearch() {
  searchOpen.value = true
  await nextTick()
  searchInput.value?.select()
}
function findNext(backwards = false) {
  if (!search || !searchQuery.value) return
  if (backwards) search.findPrevious(searchQuery.value)
  else search.findNext(searchQuery.value)
}
function closeSearch() {
  searchOpen.value = false
  search?.clearDecorations()
  term?.focus()
}

onMounted(async () => {
  term = new Terminal({
    fontFamily: getComputedStyle(document.documentElement).getPropertyValue('--font-mono') || 'monospace',
    fontSize: app.prefs.terminalFontSize,
    lineHeight: 1.15,
    cursorBlink: true,
    scrollback: 10_000,
    allowProposedApi: true,
    macOptionIsMeta: true,
    theme: app.resolvedTheme === 'light' ? LIGHT : DARK,
    windowsPty: app.info?.platform === 'windows' ? { backend: 'conpty' } : undefined,
  })
  fit = new FitAddon()
  search = new SearchAddon()
  term.loadAddon(fit)
  term.loadAddon(search)
  term.loadAddon(
    new WebLinksAddon((_event, uri) => {
      if (/^https?:\/\//i.test(uri)) openUrl(uri).catch(noop)
    }),
  )
  term.open(host.value!)
  term.attachCustomKeyEventHandler(keyHandler)
  term.onData((d) => void api.sessionWrite(props.sessionId, d).catch(noop))
  term.onBinary((d) => void api.sessionWrite(props.sessionId, d, true).catch(noop))
  term.onResize(({ cols, rows }) => {
    sessions.rememberSize(cols, rows)
    api.sessionResize(props.sessionId, cols, rows).catch(noop)
  })
  fitNow()
  try {
    await api.sessionAttach(props.sessionId, (bytes) => term?.write(bytes))
    if (term) api.sessionResize(props.sessionId, term.cols, term.rows).catch(noop)
  } catch (e) {
    attachError.value = toIpcError(e).message
    term.write(`\x1b[2m${attachError.value}\x1b[0m\r\n`)
  }
  observer = new ResizeObserver(fitSoon)
  observer.observe(host.value!)
  if (props.active) term.focus()
})

onBeforeUnmount(() => {
  observer?.disconnect()
  clearTimeout(fitTimer)
  term?.dispose()
  term = null
})

watch(
  () => props.active,
  async (active) => {
    if (!active) return
    await nextTick()
    fitNow()
    term?.focus()
  },
)
watch(
  () => app.resolvedTheme,
  (t) => {
    if (term) term.options.theme = t === 'light' ? LIGHT : DARK
  },
)
watch(
  () => app.prefs.terminalFontSize,
  (size) => {
    if (!term) return
    term.options.fontSize = size
    fitSoon()
  },
)
// Show a clear marker when the process ends.
watch(
  () => sessions.byId[props.sessionId]?.running,
  (running, wasRunning) => {
    const s = sessions.byId[props.sessionId]
    if (wasRunning && running === false && s && term) {
      const code = s.exitCode ?? '?'
      term.write(`\r\n\x1b[2m── process exited (code ${code}) ──\x1b[0m\r\n`)
    }
  },
)

defineExpose({ focus: () => term?.focus(), fit: fitNow })
</script>

<template>
  <div class="terminal-wrap">
    <div ref="host" class="terminal-host" />
    <div v-if="searchOpen" class="search" @keydown.esc.stop="closeSearch">
      <input
        ref="searchInput"
        v-model="searchQuery"
        class="input"
        placeholder="Find in terminal"
        @keydown.enter.exact.prevent="findNext()"
        @keydown.shift.enter.prevent="findNext(true)"
      />
      <button class="btn btn-ghost btn-icon btn-sm" title="Previous (Shift+Enter)" @click="findNext(true)"><ArrowUp /></button>
      <button class="btn btn-ghost btn-icon btn-sm" title="Next (Enter)" @click="findNext()"><ArrowDown /></button>
      <button class="btn btn-ghost btn-icon btn-sm" title="Close (Esc)" @click="closeSearch"><X /></button>
    </div>
  </div>
</template>

<style scoped>
.terminal-wrap {
  position: relative;
  width: 100%;
  height: 100%;
  background: v-bind("app.resolvedTheme === 'light' ? '#ffffff' : '#0b0d12'");
}
.terminal-host {
  position: absolute;
  inset: 6px 4px 4px 12px;
}
.search {
  position: absolute;
  top: 8px;
  right: 16px;
  display: flex;
  gap: 4px;
  align-items: center;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-raised);
  border: 1px solid var(--border-strong);
  box-shadow: var(--shadow-sm);
  z-index: 5;
}
.search .input {
  width: 220px;
  height: 28px;
}
:deep(.xterm) {
  height: 100%;
}
:deep(.xterm-viewport) {
  background: transparent !important;
}
</style>
