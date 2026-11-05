<script setup lang="ts">
// Project explorer — commits (W3): the history of what was committed, page by page; a commit's changed files with line
// counts; a file's diff in the read-only viewer. Commit messages and paths come from the repository and are shown as
// text. Reading never runs anything configured in the repository (core: git::commit_files / commit_file_diff).
import { onMounted, ref, watch } from 'vue'
import { EyeOff, GitCommitHorizontal } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { relativeTime } from '../lib/format'
import type { ChangedFile, CommitDiff, GitLogEntry } from '../lib/types'
import CodeEditor from './CodeEditor.vue'

const props = defineProps<{ projectId: string }>()
const PAGE = 50

const commits = ref<GitLogEntry[]>([])
const more = ref(false)
const selected = ref<GitLogEntry | null>(null)
const files = ref<ChangedFile[] | null>(null)
const diff = ref<CommitDiff | null>(null)
const diffPath = ref<string | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

async function loadMore() {
  busy.value = true
  try {
    const page = await api.projectLog(props.projectId, commits.value.length, PAGE)
    commits.value = [...commits.value, ...page]
    more.value = page.length === PAGE
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

async function select(c: GitLogEntry) {
  selected.value = c
  files.value = null
  diff.value = null
  diffPath.value = null
  error.value = null
  try {
    files.value = await api.projectCommit(props.projectId, c.oid)
    const first = files.value[0]
    if (first) await openDiff(first)
  } catch (e) {
    error.value = toIpcError(e).message
  }
}

async function openDiff(f: ChangedFile) {
  if (!selected.value) return
  diffPath.value = f.path
  diff.value = null
  try {
    diff.value = await api.projectCommitDiff(props.projectId, selected.value.oid, f.path)
  } catch (e) {
    error.value = toIpcError(e).message
  }
}

const STATUS: Record<string, string> = { A: 'added', M: 'modified', D: 'deleted', R: 'renamed', C: 'copied', T: 'type changed' }

onMounted(loadMore)
watch(
  () => props.projectId,
  () => {
    commits.value = []
    selected.value = null
    files.value = null
    diff.value = null
    loadMore()
  },
)
</script>

<template>
  <div class="commits" data-project-commits>
    <aside class="list" aria-label="Commits">
      <button v-for="c in commits" :key="c.oid" class="commit" :class="{ selected: selected?.oid === c.oid }" :data-commit="c.oid" @click="select(c)">
        <span class="summary">{{ c.summary || '(no message)' }}</span>
        <span class="meta faint"><span class="mono">{{ c.oid.slice(0, 8) }}</span> · {{ c.author }} · {{ relativeTime(c.timestampMs) }}</span>
      </button>
      <button v-if="more" class="btn btn-sm btn-ghost load" :disabled="busy" @click="loadMore">Load older commits</button>
      <p v-if="!commits.length && !busy" class="faint small pad">No commits yet.</p>
    </aside>

    <section class="detail">
      <div v-if="error" class="notice notice-error selectable">{{ error }}</div>
      <div v-if="!selected" class="placeholder faint"><GitCommitHorizontal /> Choose a commit to see what it changed.</div>
      <template v-else>
        <header class="head">
          <h3 class="selectable">{{ selected.summary || '(no message)' }}</h3>
          <span class="faint small selectable"><span class="mono">{{ selected.oid }}</span> · {{ selected.author }} · {{ new Date(selected.timestampMs).toLocaleString() }}</span>
        </header>
        <div v-if="files" class="changed" data-commit-files>
          <button v-for="f in files" :key="f.path" class="file" :class="{ selected: diffPath === f.path }" :data-commit-file="f.path" @click="openDiff(f)">
            <span class="st" :class="f.status" :title="STATUS[f.status] ?? f.status">{{ f.status }}</span>
            <span class="mono fpath">{{ f.oldPath ? `${f.oldPath} → ${f.path}` : f.path }}</span>
            <span v-if="f.additions !== null" class="counts"><span class="add">+{{ f.additions }}</span> <span class="del">−{{ f.deletions }}</span></span>
            <span v-else class="faint small">binary</span>
          </button>
          <p v-if="!files.length" class="faint small pad">No file changes (for example a merge without changes of its own).</p>
        </div>
        <div v-if="diff?.hidden" class="notice notice-warn"><EyeOff /> {{ diff.hidden }}</div>
        <template v-else-if="diff">
          <p v-if="diff.truncated" class="hint">Showing the first 400 KB of this change.</p>
          <CodeEditor :model-value="diff.text ?? ''" filename="change.diff" readonly height="calc(100vh - 420px)" data-commit-diff />
        </template>
      </template>
    </section>
  </div>
</template>

<style scoped>
.commits {
  display: grid;
  grid-template-columns: minmax(260px, 360px) 1fr;
  gap: 14px;
  min-height: 420px;
}
.list {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  overflow: auto;
  max-height: calc(100vh - 280px);
  display: flex;
  flex-direction: column;
}
.commit {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 9px 12px;
  border: 0;
  border-bottom: 1px solid var(--border);
  background: transparent;
  color: var(--text);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.commit:hover {
  background: var(--bg-hover);
}
.commit.selected {
  background: var(--accent-soft);
}
.summary {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.meta {
  font-size: 11.5px;
}
.load {
  margin: 8px;
}
.pad {
  padding: 8px 12px;
  margin: 0;
}
.detail {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.head h3 {
  margin: 0 0 2px;
  font-size: 15px;
}
.changed {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  max-height: 180px;
  overflow: auto;
}
.file {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 10px;
  border: 0;
  border-bottom: 1px solid var(--border);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.file:last-child {
  border-bottom: 0;
}
.file:hover {
  background: var(--bg-hover);
}
.file.selected {
  background: var(--accent-soft);
}
.st {
  width: 18px;
  text-align: center;
  font-weight: 600;
  font-size: 11px;
  color: var(--text-muted);
}
.st.A {
  color: var(--green);
}
.st.D {
  color: var(--red);
}
.st.R,
.st.C {
  color: var(--blue);
}
.fpath {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.counts {
  font-size: 11.5px;
}
.add {
  color: var(--green);
}
.del {
  color: var(--red);
}
.placeholder {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: center;
  min-height: 300px;
  border: 1px dashed var(--border);
  border-radius: var(--radius);
  font-size: 13px;
}
.placeholder svg {
  width: 16px;
  height: 16px;
}
</style>
