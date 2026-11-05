<script setup lang="ts">
// Project discovery and restoration (Portable Workspace P4): projects missing on this computer (clone from their
// recorded remote), clones that belong to a project (link), and new repositories in the clone folder (review, then
// register — nothing in them is run). Shown only when there is something to do.
import { computed, onMounted, ref, watch } from 'vue'
import { CloudDownload, FolderSearch, Link2, Plus, RefreshCw } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { ProjectScan } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const scan = ref<ProjectScan | null>(null)
const busy = ref<string | null>(null)
const selected = ref<Set<string>>(new Set())

const known = computed(() => scan.value?.found.filter((f) => f.status === 'known') ?? [])
const fresh = computed(() => scan.value?.found.filter((f) => f.status === 'new') ?? [])
const missing = computed(() => scan.value?.missing ?? [])
const anything = computed(() => known.value.length + fresh.value.length + missing.value.length > 0)

async function load() {
  const ws = workspaces.active
  if (!ws) return
  try {
    scan.value = await api.projectScan(ws.id)
    selected.value = new Set(missing.value.filter((m) => m.canClone).map((m) => m.id))
  } catch (e) {
    app.toast('error', `Could not scan the clone folder: ${toIpcError(e).message}`)
  }
}
onMounted(load)
watch(() => projects.list.length, load)
defineExpose({ load })

function toggle(id: string) {
  const s = new Set(selected.value)
  if (s.has(id)) s.delete(id)
  else s.add(id)
  selected.value = s
}

async function cloneSelected() {
  const ws = workspaces.active
  if (!ws) return
  const ids = missing.value.filter((m) => m.canClone && selected.value.has(m.id))
  for (const m of ids) {
    busy.value = m.id
    try {
      await api.projectClone(ws.id, m.id, m.cloneTo)
      app.toast('success', `Cloned ${m.name}`)
    } catch (e) {
      app.toast('error', `Cloning ${m.name} failed: ${toIpcError(e).message}`, 9000)
    }
  }
  busy.value = null
  await projects.reload()
  await load()
}

async function link(projectId: string, path: string) {
  busy.value = projectId
  await app.attempt(() => projects.locate(projectId, path), 'Could not link the clone')
  busy.value = null
  await load()
}

async function register(path: string, rel: string, stacks: string[]) {
  const ok = await app.confirm({
    title: 'Register this repository?',
    message: `${rel}${stacks.length ? ` (${stacks.join(', ')})` : ''} becomes a project of this workspace and gets a portable context. NEXUS reads its manifest files only — nothing in the repository is run.`,
    confirmLabel: 'Register',
  })
  if (!ok) return
  busy.value = path
  await app.attempt(() => projects.register(path), 'Could not register the repository')
  busy.value = null
  await load()
}
</script>

<template>
  <section v-if="scan && anything" class="card discovery" data-discovery>
    <div class="head">
      <h2 class="card-title"><FolderSearch /> On this computer</h2>
      <button class="btn btn-sm btn-ghost" :disabled="!!busy" data-discovery-rescan @click="load"><RefreshCw /> Rescan {{ scan.projectsDir }}/</button>
    </div>

    <div v-if="missing.length" class="group" data-discovery-missing>
      <h3>Not on this computer yet</h3>
      <label v-for="m in missing" :key="m.id" class="row" :data-missing="m.name">
        <input type="checkbox" :disabled="!m.canClone || !!busy" :checked="selected.has(m.id)" @change="toggle(m.id)" />
        <strong>{{ m.name }}</strong>
        <span class="mono faint">{{ m.remoteUrl ?? 'no remote recorded' }}</span>
        <span v-if="m.cloneTo" class="faint">→ <span class="mono">{{ m.cloneTo }}</span></span>
        <span v-else class="faint">{{ m.reason }}</span>
      </label>
      <button class="btn btn-sm btn-primary" :disabled="!!busy || !selected.size" data-discovery-clone @click="cloneSelected">
        <CloudDownload :class="{ spin: !!busy }" /> Clone selected
      </button>
    </div>

    <div v-if="known.length" class="group" data-discovery-known>
      <h3>Clones that belong to a project</h3>
      <div v-for="f in known" :key="f.rel" class="row">
        <span class="mono">{{ f.rel }}</span>
        <span class="faint">same remote as a project that is missing here</span>
        <button class="btn btn-sm" :disabled="!!busy" :data-link="f.rel" @click="link(f.projectId!, f.path)"><Link2 /> Link</button>
      </div>
    </div>

    <div v-if="fresh.length" class="group" data-discovery-new>
      <h3>New repositories in {{ scan.projectsDir }}/</h3>
      <div v-for="f in fresh" :key="f.rel" class="row">
        <span class="mono">{{ f.rel }}</span>
        <span class="faint">{{ f.stacks.join(', ') || 'no stack detected' }}</span>
        <span v-if="f.remoteUrl" class="mono faint">{{ f.remoteUrl }}</span>
        <button class="btn btn-sm" :disabled="!!busy" :data-register="f.rel" @click="register(f.path, f.rel, f.stacks)"><Plus /> Register…</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.discovery {
  margin-bottom: 16px;
}
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.group {
  margin-top: 10px;
}
.group h3 {
  font-size: 12px;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-faint);
  margin: 0 0 6px;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 5px 0;
  font-size: 13px;
  flex-wrap: wrap;
}
.row .btn {
  margin-left: auto;
}
</style>
