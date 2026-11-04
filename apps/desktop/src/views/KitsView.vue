<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from 'vue'
import { CloudDownload, Package, Plus, RefreshCw, ShieldCheck, Trash2, TriangleAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { KIT_STATUS, listingTokens } from '../lib/kits'
import type { KitSelection, KitValidation, KitView } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import KitProfileCard from '../components/KitProfileCard.vue'
import RegisterKitDialog from '../components/RegisterKitDialog.vue'

// AI Kit manager (ADR-0024): review a kit, approve exactly that content, enable it per profile with only the
// skills you need. Kits reach NEXUS sessions through --plugin-dir; no Claude configuration is changed.
const app = useAppStore()
const profiles = useProfileStore()
const kits = ref<KitView[]>([])
const selectedId = ref<string | null>(null)
const registering = ref(false)
const busy = ref(false)
const validation = ref<KitValidation | null>(null)
const error = ref<string | null>(null)

const kit = computed(() => kits.value.find((k) => k.id === selectedId.value) ?? null)

async function load() {
  try {
    kits.value = await api.kitList()
    error.value = null
    if (!kit.value) selectedId.value = kits.value[0]?.id ?? null
  } catch (e) {
    error.value = toIpcError(e).message
  }
}
onMounted(() => {
  load()
  if (!profiles.list.length) profiles.load().catch(() => undefined)
  for (const p of profiles.list) profiles.loadDetails(p.id)
})
onActivated(load)

async function act<T>(fn: () => Promise<T>, failure: string, success?: string) {
  busy.value = true
  const r = await app.attempt(fn, failure)
  busy.value = false
  if (r !== undefined && success) app.toast('success', success)
  await load()
  return r
}
const approve = () => kit.value && act(() => api.kitApprove(kit.value!.id), 'Could not approve', `Approved ${kit.value.name} — this exact content`)
const update = () => kit.value && act(() => api.kitUpdate(kit.value!.id), 'Could not update')
async function validate() {
  if (!kit.value) return
  validation.value = (await act(() => api.kitValidate(kit.value!.id), 'Could not run the validator')) ?? null
}
async function remove() {
  if (!kit.value) return
  const ok = await app.confirm({
    title: `Remove ${kit.value.name}?`,
    message: kit.value.sourceKind === 'git' ? 'NEXUS deletes its own clone of the repository.' : 'Your folder is not touched; NEXUS only forgets the kit.',
    confirmLabel: 'Remove kit',
    danger: true,
  })
  if (!ok) return
  await act(() => api.kitRemove(kit.value!.id), 'Could not remove')
  selectedId.value = null
  await load()
}
const save = (profileId: string, sel: KitSelection) => kit.value && act(() => api.kitEnable(kit.value!.id, profileId, sel), 'Could not save', 'Saved — applies to new sessions')
const disable = (profileId: string) => kit.value && act(() => api.kitDisable(kit.value!.id, profileId), 'Could not disable')
function registered(k: KitView) {
  selectedId.value = k.id
  load()
}
</script>

<template>
  <div class="page">
    <div class="page-header">
      <div>
        <h1>AI Kits</h1>
        <p>Claude Code plugins and skill folders, enabled per profile for NEXUS sessions — only the skills you choose reach Claude's context.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" @click="load"><RefreshCw /> Refresh</button>
        <button class="btn btn-primary" data-register-kit @click="registering = true"><Plus /> Register kit</button>
      </div>
    </div>
    <div v-if="error" class="notice notice-error selectable">{{ error }}</div>

    <div v-if="!kits.length" class="empty">
      <Package class="empty-icon" />
      <h3>No kits yet</h3>
      <p>Register a local plugin folder or a Git repository. NEXUS shows exactly what it contains before anything is enabled.</p>
    </div>

    <div v-else class="layout">
      <nav class="kit-list" aria-label="Kits">
        <button v-for="k in kits" :key="k.id" class="kit-item" :class="{ active: k.id === selectedId }" :data-kit="k.name" @click="(selectedId = k.id), (validation = null)">
          <span class="kit-name">{{ k.name }}</span>
          <span class="badge" :class="KIT_STATUS[k.status]?.tone">{{ KIT_STATUS[k.status]?.label }}</span>
          <span class="faint small">{{ k.inventory.skills.length }} skills · {{ k.profiles.length }} profile(s)</span>
        </button>
      </nav>

      <section v-if="kit" class="detail" data-kit-detail>
        <header class="detail-head">
          <div>
            <h2>{{ kit.inventory.name }} <span class="faint">{{ kit.version }}</span></h2>
            <p class="faint mono small selectable">{{ kit.sourceKind === 'git' ? kit.location : kit.contentDir }}<template v-if="kit.gitRef"> @ {{ kit.gitRef }}</template></p>
            <p v-if="kit.inventory.description">{{ kit.inventory.description }}</p>
          </div>
          <div class="actions">
            <button class="btn btn-sm" :disabled="busy" @click="update"><CloudDownload /> {{ kit.sourceKind === 'git' ? 'Fetch update' : 'Re-read' }}</button>
            <button class="btn btn-sm" :disabled="busy" data-validate @click="validate">Validate with Claude</button>
            <button class="btn btn-sm btn-ghost" :disabled="busy" @click="remove"><Trash2 /></button>
          </div>
        </header>

        <div v-if="kit.status !== 'approved'" class="notice notice-warn approve" data-review>
          <TriangleAlert />
          <span>{{ kit.status === 'changed' ? 'The kit changed since you approved it. Sessions do not get it until you review the new content.' : 'Review the contents below. No session gets this kit until you approve this exact version.' }}</span>
          <button class="btn btn-sm btn-primary" :disabled="busy || kit.status === 'missing'" data-approve @click="approve"><ShieldCheck /> Approve this version</button>
        </div>
        <div v-for="p in kit.inventory.problems" :key="p" class="notice notice-error"><TriangleAlert /> {{ p }}</div>
        <pre v-if="validation" class="validation selectable" :class="{ ok: validation.ok }">{{ validation.ok ? '✔ claude plugin validate passed' : '✘ claude plugin validate failed' }}
{{ validation.output.trim() }}</pre>

        <div class="inventory">
          <div class="label">Contents · ~{{ listingTokens([...kit.inventory.skills, ...kit.inventory.agents, ...kit.inventory.commands]).toLocaleString() }} tokens if everything were loaded</div>
          <div v-for="s in kit.inventory.skills" :key="'s' + s.name" class="inv-row"><span class="badge">skill</span><code>{{ kit.inventory.name }}:{{ s.name }}</code><span class="faint grow">{{ s.description }}</span></div>
          <div v-for="a in kit.inventory.agents" :key="'a' + a.name" class="inv-row"><span class="badge">agent</span><code>{{ kit.inventory.name }}:{{ a.name }}</code><span class="faint grow">{{ a.description }}</span></div>
          <div v-for="c in kit.inventory.commands" :key="'c' + c.name" class="inv-row"><span class="badge">command</span><code>/{{ kit.inventory.name }}:{{ c.name }}</code><span class="faint grow">{{ c.description }}</span></div>
          <div v-for="h in kit.inventory.hooks" :key="'h' + h.event" class="inv-row warn"><span class="badge badge-warn">hook</span><code>{{ h.event }}</code><span class="grow">runs {{ h.programs.join(', ') || h.types.join(', ') }} — off unless enabled per profile</span></div>
          <div v-for="m in kit.inventory.mcpServers" :key="'m' + m.name" class="inv-row warn"><span class="badge badge-warn">MCP</span><code>{{ m.name }}</code><span class="grow">{{ m.transport }} · {{ m.target }} — off unless enabled per profile</span></div>
        </div>

        <div class="label profiles-label">Profiles</div>
        <div class="profiles">
          <KitProfileCard
            v-for="p in profiles.list"
            :key="p.id"
            :kit="kit"
            :profile="p"
            :row="kit.profiles.find((r) => r.profileId === p.id) ?? null"
            :policy="profiles.details[p.id]?.value?.policy ?? null"
            :busy="busy"
            @save="(sel) => save(p.id, sel)"
            @disable="disable(p.id)"
          />
        </div>
        <p class="faint small">Applies to sessions NEXUS starts from now on, through <code>--plugin-dir</code>. Your Claude configuration folders are not changed, and <code>claude</code> started elsewhere does not see the kit.</p>
      </section>
    </div>

    <RegisterKitDialog :open="registering" @close="registering = false" @registered="registered" />
  </div>
</template>

<style scoped>
.layout {
  display: grid;
  grid-template-columns: 260px 1fr;
  gap: 16px;
  align-items: start;
}
.kit-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.kit-item {
  text-align: left;
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px;
  background: var(--bg-raised);
  color: var(--text);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.kit-item.active {
  border-color: var(--accent-ring);
  background: var(--accent-soft);
}
.kit-name {
  font-weight: 650;
}
.detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.detail-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.detail-head h2 {
  margin: 0;
}
.actions {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}
.approve {
  align-items: center;
  gap: 10px;
}
.validation {
  white-space: pre-wrap;
  font-size: 12px;
  background: var(--bg-raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px;
  max-height: 220px;
  overflow: auto;
  margin: 0;
}
.inventory {
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 10px;
}
.inv-row {
  display: flex;
  gap: 8px;
  align-items: baseline;
  padding: 4px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.inv-row.warn .grow {
  color: var(--warn, #d9a441);
}
.grow {
  flex: 1;
  min-width: 0;
}
.profiles {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 10px;
}
.small {
  font-size: 12px;
}
</style>
