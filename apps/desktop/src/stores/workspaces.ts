import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api } from '../lib/ipc'
import type { Workspace } from '../lib/types'

export const useWorkspaceStore = defineStore('workspaces', () => {
  const list = ref<Workspace[]>([])
  const active = ref<Workspace | null>(null)
  const loaded = ref(false)

  async function load() {
    const [all, current] = await Promise.all([api.workspaceList(), api.workspaceActive()])
    list.value = all
    active.value = current
    loaded.value = true
  }

  async function create(path: string, name: string, description?: string, template?: string | null) {
    active.value = await api.workspaceCreate(path, name, description, template)
    list.value = await api.workspaceList()
    return active.value
  }

  /** A new workspace in an empty repository the person prepared (cloned into `folder`). */
  async function createFromRepository(url: string, folder: string, name: string, template?: string | null) {
    active.value = await api.syncCloneNew(url, folder, name, template ?? null)
    list.value = await api.workspaceList()
    return active.value
  }

  async function importExisting(path: string) {
    active.value = await api.workspaceImport(path)
    list.value = await api.workspaceList()
    return active.value
  }

  async function activate(id: string) {
    active.value = await api.workspaceActivate(id)
    list.value = await api.workspaceList()
  }

  async function forget(id: string) {
    await api.workspaceForget(id)
    await load()
  }

  return { list, active, loaded, load, create, createFromRepository, importExisting, activate, forget }
})
