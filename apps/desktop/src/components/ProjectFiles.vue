<script setup lang="ts">
// Project explorer — files (W3): a lazily expanded folder tree and a read-only viewer (CodeMirror). NEXUS only reads:
// nothing here writes to the project, and files that may hold secrets are not shown (core: project::explorer).
import { computed, onMounted, ref, watch } from 'vue'
import { ChevronDown, ChevronRight, EyeOff, File, FileWarning, Folder, FolderOpen, Link2 } from '@lucide/vue'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { api, toIpcError } from '../lib/ipc'
import type { ExplorerEntry, ExplorerFile } from '../lib/types'
import CodeEditor from './CodeEditor.vue'

const props = defineProps<{ projectId: string; root: string }>()

const listings = ref<Record<string, ExplorerEntry[]>>({})
const truncatedDirs = ref<Set<string>>(new Set())
const expanded = ref<Set<string>>(new Set(['']))
const selected = ref<string | null>(null)
const file = ref<ExplorerFile | null>(null)
const loading = ref(false)
const error = ref<string | null>(null)
const showIgnored = ref(false)

async function loadDir(dir: string) {
  try {
    const l = await api.projectTree(props.projectId, dir)
    listings.value = { ...listings.value, [dir]: l.entries }
    if (l.truncated) truncatedDirs.value = new Set([...truncatedDirs.value, dir])
  } catch (e) {
    error.value = toIpcError(e).message
  }
}

async function toggle(e: ExplorerEntry) {
  if (!e.dir) return open(e)
  const next = new Set(expanded.value)
  if (next.has(e.path)) next.delete(e.path)
  else {
    next.add(e.path)
    if (!listings.value[e.path]) await loadDir(e.path)
  }
  expanded.value = next
}

async function open(e: ExplorerEntry) {
  if (e.symlink) return
  selected.value = e.path
  loading.value = true
  error.value = null
  try {
    file.value = await api.projectFile(props.projectId, e.path)
  } catch (err) {
    file.value = null
    error.value = toIpcError(err).message
  } finally {
    loading.value = false
  }
}

/** The visible tree as flat rows (depth for indentation). */
const rows = computed(() => {
  const out: { entry: ExplorerEntry; depth: number }[] = []
  const walk = (dir: string, depth: number) => {
    for (const e of listings.value[dir] ?? []) {
      if (e.ignored && !showIgnored.value) continue
      out.push({ entry: e, depth })
      if (e.dir && expanded.value.has(e.path)) walk(e.path, depth + 1)
    }
  }
  walk('', 0)
  return out
})
const hiddenCount = computed(() => Object.values(listings.value).flat().filter((e) => e.ignored).length)

const size = (n: number) => (n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : n >= 1024 ? `${Math.round(n / 1024)} KB` : `${n} B`)
const sep = computed(() => (props.root.includes('\\') ? '\\' : '/'))
function reveal() {
  if (selected.value) revealItemInDir(`${props.root}${sep.value}${selected.value.replace(/\//g, sep.value)}`).catch(() => undefined)
}

onMounted(() => loadDir(''))
watch(
  () => props.projectId,
  () => {
    listings.value = {}
    expanded.value = new Set([''])
    selected.value = null
    file.value = null
    loadDir('')
  },
)
</script>

<template>
  <div class="files" data-project-files>
    <aside class="tree" aria-label="Project files">
      <div class="tree-head">
        <span class="faint small">Files</span>
        <label v-if="hiddenCount" class="toggle small faint"><input v-model="showIgnored" type="checkbox" /> ignored by Git</label>
      </div>
      <div class="tree-body">
        <button
          v-for="r in rows"
          :key="r.entry.path"
          class="node"
          :class="{ selected: selected === r.entry.path, ignored: r.entry.ignored }"
          :style="{ paddingLeft: `${8 + r.depth * 14}px` }"
          :title="r.entry.symlink ? 'A link — not followed' : r.entry.path"
          :data-file="r.entry.path"
          @click="toggle(r.entry)"
        >
          <template v-if="r.entry.dir">
            <ChevronDown v-if="expanded.has(r.entry.path)" class="chev" /><ChevronRight v-else class="chev" />
            <FolderOpen v-if="expanded.has(r.entry.path)" class="ic folder" /><Folder v-else class="ic folder" />
          </template>
          <template v-else>
            <span class="chev" />
            <Link2 v-if="r.entry.symlink" class="ic" /><File v-else class="ic" />
          </template>
          <span class="name">{{ r.entry.name }}</span>
        </button>
        <p v-if="truncatedDirs.size" class="faint small pad">Large folders show their first 2,000 entries.</p>
        <p v-if="!rows.length && listings['']" class="faint small pad">This folder is empty.</p>
      </div>
    </aside>

    <section class="viewer">
      <header v-if="selected" class="viewer-head">
        <span class="mono path selectable" data-file-path>{{ selected }}</span>
        <span v-if="file" class="faint small">{{ size(file.size) }}</span>
        <span class="spacer" />
        <button class="btn btn-sm btn-ghost" @click="reveal"><FolderOpen /> Show in folder</button>
      </header>
      <div v-if="error" class="notice notice-error selectable">{{ error }}</div>
      <div v-if="!selected" class="placeholder faint">Choose a file to look at it. NEXUS only reads your project.</div>
      <div v-else-if="loading" class="placeholder faint">Loading…</div>
      <template v-else-if="file">
        <div v-if="file.hidden" class="notice notice-warn" data-file-hidden><EyeOff /> {{ file.hidden }}</div>
        <div v-else-if="file.binary" class="notice" data-file-binary><FileWarning /> A binary file — nothing to show here.</div>
        <template v-else>
          <p v-if="file.truncated" class="hint">Showing the first 1 MB.</p>
          <CodeEditor :model-value="file.text ?? ''" :filename="file.path" readonly height="calc(100vh - 330px)" data-file-viewer />
        </template>
      </template>
    </section>
  </div>
</template>

<style scoped>
.files {
  display: grid;
  grid-template-columns: minmax(220px, 300px) 1fr;
  gap: 14px;
  min-height: 420px;
}
.tree {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  display: flex;
  flex-direction: column;
  min-height: 0;
  max-height: calc(100vh - 280px);
}
.tree-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border-bottom: 1px solid var(--border);
}
.toggle {
  display: flex;
  gap: 4px;
  align-items: center;
}
.tree-body {
  overflow: auto;
  padding: 4px 0;
}
.node {
  display: flex;
  align-items: center;
  gap: 5px;
  width: 100%;
  height: 26px;
  padding-right: 8px;
  border: 0;
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  white-space: nowrap;
}
.node:hover {
  background: var(--bg-hover);
}
.node.selected {
  background: var(--accent-soft);
}
.node.ignored {
  color: var(--text-faint);
}
.chev {
  width: 13px;
  height: 13px;
  flex: none;
  color: var(--text-faint);
}
.ic {
  width: 14px;
  height: 14px;
  flex: none;
  color: var(--text-muted);
}
.ic.folder {
  color: var(--accent-strong);
}
.name {
  overflow: hidden;
  text-overflow: ellipsis;
}
.pad {
  padding: 6px 10px;
  margin: 0;
}
.viewer {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.viewer-head {
  display: flex;
  align-items: center;
  gap: 10px;
  min-height: 30px;
}
.path {
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.spacer {
  flex: 1;
}
.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 300px;
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  font-size: 13px;
}
</style>
