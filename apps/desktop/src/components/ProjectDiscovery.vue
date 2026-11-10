<script setup lang="ts">
// Project discovery and restoration (Portable Workspace P4): projects missing on this computer (clone from their
// recorded remote), clones that belong to a project (link), and new repositories in the clone folder (review, then
// register — nothing in them is run). Shown only when there is something to do.
import { computed, onMounted, ref, watch } from 'vue'
import { CloudDownload, FolderSearch, Link2, Plus, RefreshCw } from '@lucide/vue'
import { useAction } from '../composables/useAction'
import { api, toIpcError } from '../lib/ipc'
import type { ProjectScan } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import ActionButton from './ActionButton.vue'

const app = useAppStore()
const projects = useProjectStore()
const workspaces = useWorkspaceStore()
const scan = ref<ProjectScan | null>(null)
const scanning = ref(false)
/** The clone that runs now (clones run one after the other). */
const cloning = ref<{ name: string; n: number; of: number } | null>(null)
const selected = ref<Set<string>>(new Set())

const known = computed(() => scan.value?.found.filter((f) => f.status === 'known') ?? [])
const fresh = computed(() => scan.value?.found.filter((f) => f.status === 'new') ?? [])
const missing = computed(() => scan.value?.missing ?? [])
const anything = computed(() => known.value.length + fresh.value.length + missing.value.length > 0)

async function load() {
  const ws = workspaces.active
  if (!ws) return
  scanning.value = true
  try {
    scan.value = await api.projectScan(ws.id)
    selected.value = new Set(missing.value.filter((m) => m.canClone).map((m) => m.id))
  } catch (e) {
    app.toast('error', `Could not scan the clone folder: ${toIpcError(e).message}`)
  } finally {
    scanning.value = false
  }
}
onMounted(load)
watch(() => projects.list.length, load)
// Folders change outside NEXUS: look again when the window regains focus or a session ended (ADR-0048).
watch(() => app.freshness, load)
defineExpose({ load })

function toggle(id: string) {
  const s = new Set(selected.value)
  if (s.has(id)) s.delete(id)
  else s.add(id)
  selected.value = s
}

async function cloneSelected() {
  const ws = workspaces.active
  if (!ws || cloning.value) return
  const list = missing.value.filter((m) => m.canClone && selected.value.has(m.id))
  try {
    for (const [i, m] of list.entries()) {
      cloning.value = { name: m.name, n: i + 1, of: list.length }
      try {
        await api.projectClone(ws.id, m.id, m.cloneTo)
        app.toast('success', `Cloned ${m.name}`)
      } catch (e) {
        app.toast('error', `Cloning ${m.name} failed: ${toIpcError(e).message}`)
      }
    }
  } finally {
    cloning.value = null
  }
  await projects.reload()
  await load()
}
const cloneLabel = computed(() => (cloning.value ? `Cloning ${cloning.value.name}${cloning.value.of > 1 ? ` (${cloning.value.n} of ${cloning.value.of})` : ''}… this can take a few minutes` : undefined))

const linking = useAction((projectId: string, path: string) => projects.locate(projectId, path), { failure: 'Could not link the clone', key: (projectId: string) => projectId })
async function link(projectId: string, path: string) {
  await linking.run(projectId, path)
  await load()
}

const registering = useAction((path: string) => projects.register(path), { failure: 'Could not register the repository', key: (path: string) => path })
async function register(path: string, rel: string, stacks: string[]) {
  const ok = await app.confirm({
    title: 'Register this repository?',
    message: `${rel}${stacks.length ? ` (${stacks.join(', ')})` : ''} becomes a project of this workspace and gets a portable context. NEXUS reads its manifest files only — nothing in the repository is run.`,
    confirmLabel: 'Register',
  })
  if (!ok) return
  await registering.run(path)
  await load()
}

/** A clone, link or registration runs: the other actions wait. */
const busy = computed(() => !!cloning.value || linking.pending.value || registering.pending.value)
</script>

<template>
  <section v-if="scan && anything" class="card discovery" data-discovery>
    <div class="head">
      <h2 class="card-title"><FolderSearch /> On this computer</h2>
      <ActionButton class="btn-sm btn-ghost" :pending="scanning" :disabled="busy" data-discovery-rescan @click="load"><template #icon><RefreshCw /></template>Rescan {{ scan.projectsDir }}/</ActionButton>
    </div>

    <div v-if="missing.length" class="group" data-discovery-missing>
      <h3>Not on this computer yet</h3>
      <label v-for="m in missing" :key="m.id" class="row" :data-missing="m.name">
        <input type="checkbox" :disabled="!m.canClone || busy" :checked="selected.has(m.id)" @change="toggle(m.id)" />
        <strong>{{ m.name }}</strong>
        <span class="mono faint">{{ m.remoteUrl ?? 'no remote recorded' }}</span>
        <span v-if="m.cloneTo" class="faint">→ <span class="mono">{{ m.cloneTo }}</span></span>
        <span v-else class="faint">{{ m.reason }}</span>
      </label>
      <ActionButton class="btn-sm btn-primary" :pending="!!cloning" :pending-label="cloneLabel" :disabled="busy || !selected.size" data-discovery-clone @click="cloneSelected">
        <template #icon><CloudDownload /></template>Clone selected
      </ActionButton>
    </div>

    <div v-if="known.length" class="group" data-discovery-known>
      <h3>Clones that belong to a project</h3>
      <div v-for="f in known" :key="f.rel" class="row">
        <span class="mono">{{ f.rel }}</span>
        <span class="faint">same remote as a project that is missing here</span>
        <ActionButton class="btn-sm" :pending="linking.isPending(f.projectId!)" pending-label="Linking…" :disabled="busy" :data-link="f.rel" @click="link(f.projectId!, f.path)"><template #icon><Link2 /></template>Link</ActionButton>
      </div>
    </div>

    <div v-if="fresh.length" class="group" data-discovery-new>
      <h3>New repositories in {{ scan.projectsDir }}/</h3>
      <div v-for="f in fresh" :key="f.rel" class="row">
        <span class="mono">{{ f.rel }}</span>
        <span class="faint">{{ f.stacks.join(', ') || 'no stack detected' }}</span>
        <span v-if="f.remoteUrl" class="mono faint">{{ f.remoteUrl }}</span>
        <ActionButton class="btn-sm" :pending="registering.isPending(f.path)" pending-label="Registering…" :disabled="busy" :data-register="f.rel" @click="register(f.path, f.rel, f.stacks)"><template #icon><Plus /></template>Register…</ActionButton>
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
