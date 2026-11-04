import { defineStore } from 'pinia'
import { computed, reactive, ref } from 'vue'
import { api, toIpcError } from '../lib/ipc'
import type { GitStatus, Project } from '../lib/types'

interface GitEntry {
  status: GitStatus | null
  error: string | null
  loading: boolean
  at: number
}

const GIT_TTL_MS = 15_000

export const useProjectStore = defineStore('projects', () => {
  const list = ref<Project[]>([])
  const workspaceId = ref<string | null>(null)
  const loading = ref(false)
  const git = reactive<Record<string, GitEntry>>({})

  const byId = computed(() => Object.fromEntries(list.value.map((p) => [p.id, p])))

  async function load(wsId: string) {
    loading.value = true
    try {
      workspaceId.value = wsId
      list.value = await api.projectList(wsId)
    } finally {
      loading.value = false
    }
  }

  async function reload() {
    if (workspaceId.value) await load(workspaceId.value)
  }

  function upsert(p: Project) {
    const i = list.value.findIndex((x) => x.id === p.id)
    if (i === -1) list.value.unshift(p)
    else list.value[i] = p
  }

  async function register(path: string, name?: string) {
    if (!workspaceId.value) throw new Error('no active workspace')
    const p = await api.projectRegister(workspaceId.value, path, name)
    upsert(p)
    return p
  }

  async function unregister(id: string) {
    await api.projectUnregister(id)
    list.value = list.value.filter((p) => p.id !== id)
    delete git[id]
  }

  async function locate(id: string, path: string) {
    upsert(await api.projectLocate(id, path))
  }

  async function refresh(id: string) {
    upsert(await api.projectRefresh(id))
    await gitStatus(id, true)
  }

  async function gitStatus(id: string, force = false): Promise<GitEntry> {
    const existing = git[id]
    if (existing && (existing.loading || (!force && Date.now() - existing.at < GIT_TTL_MS))) return existing
    if (!git[id]) git[id] = { status: null, error: null, loading: false, at: 0 }
    // Mutate through the reactive proxy (not the raw object) so views update.
    const entry = git[id]!
    entry.loading = true
    try {
      entry.status = await api.projectGitStatus(id)
      entry.error = null
    } catch (e) {
      entry.error = toIpcError(e).message
    } finally {
      entry.loading = false
      entry.at = Date.now()
    }
    return entry
  }

  return { list, byId, loading, git, load, reload, register, unregister, locate, refresh, gitStatus }
})
