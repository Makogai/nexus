import { defineStore } from 'pinia'
import { computed, reactive, ref } from 'vue'
import { api, toIpcError } from '../lib/ipc'
import type {
  AgentProfile,
  AgentProfileInput,
  AgentsOverview,
  AuthStatus,
  ClaudeProfile,
  DetectedProfile,
  Installation,
  LaunchOptions,
  ProfileDetails,
  ProfileInput,
} from '../lib/types'

interface Loadable<T> {
  value: T | null
  error: string | null
  loading: boolean
}

function loadable<T>(): Loadable<T> {
  return { value: null, error: null, loading: false }
}

export const useProfileStore = defineStore('profiles', () => {
  const list = ref<ClaudeProfile[]>([])
  const detected = ref<DetectedProfile[]>([])
  const installations = ref<Installation[] | null>(null)
  const installationsLoading = ref(false)
  const launchOptions = ref<LaunchOptions | null>(null)
  /** Per-profile facts; kept in memory only (auth contains identity and is never persisted). */
  const details = reactive<Record<string, Loadable<ProfileDetails>>>({})
  const auth = reactive<Record<string, Loadable<AuthStatus>>>({})

  const byId = computed(() => Object.fromEntries(list.value.map((p) => [p.id, p])))
  const unregistered = computed(() => detected.value.filter((d) => !d.registeredProfileId))

  async function load() {
    const [profiles, options] = await Promise.all([api.profileList(), api.launchOptions()])
    list.value = profiles
    launchOptions.value = options
  }

  async function detect() {
    detected.value = await api.profileDetect()
  }

  async function loadInstallations() {
    installationsLoading.value = true
    try {
      installations.value = await api.claudeInstallations()
    } finally {
      installationsLoading.value = false
    }
  }

  async function fetchInto<T>(store: Record<string, Loadable<T>>, id: string, fn: () => Promise<T>) {
    if (!store[id]) store[id] = loadable<T>()
    // Mutate through the reactive proxy (not the raw object) so views update.
    const entry = store[id]!
    if (entry.loading) return
    entry.loading = true
    try {
      entry.value = await fn()
      entry.error = null
    } catch (e) {
      entry.error = toIpcError(e).message
    } finally {
      entry.loading = false
    }
  }

  const loadDetails = (id: string) => fetchInto(details, id, () => api.profileDetails(id))
  const loadAuth = (id: string) => fetchInto(auth, id, () => api.profileAuthStatus(id))

  async function register(input: ProfileInput) {
    const p = await api.profileRegister(input)
    await Promise.all([load(), detect()])
    return p
  }

  async function create(input: ProfileInput) {
    const p = await api.profileCreate(input)
    await Promise.all([load(), detect()])
    return p
  }

  async function update(id: string, input: ProfileInput) {
    const p = await api.profileUpdate(id, input)
    await load()
    delete details[id]
    return p
  }

  async function archive(id: string) {
    await api.profileSetArchived(id, true)
    await Promise.all([load(), detect()])
  }

  async function forget(id: string) {
    await api.profileForget(id)
    await Promise.all([load(), detect()])
    delete details[id]
    delete auth[id]
  }

  // ---- other agents (ADR-0052): their profiles live apart from Claude profiles ----
  const agents = ref<AgentsOverview | null>(null)
  const agentProfiles = computed<AgentProfile[]>(() => agents.value?.profiles.filter((p) => !p.archivedAt) ?? [])
  const agentProfilesById = computed(() => Object.fromEntries((agents.value?.profiles ?? []).map((p) => [p.id, p])))

  async function loadAgents() {
    agents.value = await api.agentsOverview()
  }
  async function registerAgent(input: AgentProfileInput) {
    const p = await api.agentProfileRegister(input)
    await loadAgents()
    return p
  }
  async function updateAgent(id: string, input: AgentProfileInput) {
    const p = await api.agentProfileUpdate(id, input)
    await loadAgents()
    return p
  }
  async function forgetAgent(id: string) {
    await api.agentProfileForget(id)
    await loadAgents()
  }

  return {
    list,
    byId,
    detected,
    unregistered,
    installations,
    installationsLoading,
    launchOptions,
    details,
    auth,
    load,
    detect,
    loadInstallations,
    loadDetails,
    loadAuth,
    register,
    create,
    update,
    archive,
    forget,
    agents,
    agentProfiles,
    agentProfilesById,
    loadAgents,
    registerAgent,
    updateAgent,
    forgetAgent,
  }
})
