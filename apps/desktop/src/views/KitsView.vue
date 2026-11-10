<script setup lang="ts">
import { computed, onActivated, onMounted, ref } from 'vue'
import { CloudDownload, Package, Plus, RefreshCw, ShieldCheck, Trash2, TriangleAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { KIT_STATUS, listingTokens } from '../lib/kits'
import type { KitSelection, KitValidation, KitView } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import { useAction } from '../composables/useAction'
import ActionButton from '../components/ActionButton.vue'
import KitProfileCard from '../components/KitProfileCard.vue'
import RegisterKitDialog from '../components/RegisterKitDialog.vue'

// AI Kit manager (ADR-0024): review a kit, approve exactly that content, enable it per profile with only the
// skills you need. Kits reach NEXUS sessions through --plugin-dir; no Claude configuration is changed.
const app = useAppStore()
const profiles = useProfileStore()
const kits = ref<KitView[]>([])
const selectedId = ref<string | null>(null)
const registering = ref(false)
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

// Every kit action reloads the list afterwards, whether it worked or not.
async function thenLoad<T>(call: Promise<T>): Promise<T> {
  try {
    return await call
  } finally {
    await load()
  }
}
const refreshing = useAction(load)
const approving = useAction(
  async (k: KitView) => {
    await thenLoad(api.kitApprove(k.id))
    return k.name
  },
  { failure: 'Could not approve', success: (name) => `Approved ${name} — this exact content` },
)
const updating = useAction((id: string) => thenLoad(api.kitUpdate(id)), { failure: 'Could not update' })
const validating = useAction(
  async (id: string) => {
    try {
      validation.value = await thenLoad(api.kitValidate(id))
    } catch (e) {
      validation.value = null
      throw e
    }
  },
  { failure: 'Could not run the validator' },
)
const removing = useAction((id: string) => thenLoad(api.kitRemove(id)), { failure: 'Could not remove' })
const saving = useAction((id: string, profileId: string, sel: KitSelection) => thenLoad(api.kitEnable(id, profileId, sel)), { failure: 'Could not save', success: 'Saved — applies to new sessions' })
const disabling = useAction((id: string, profileId: string) => thenLoad(api.kitDisable(id, profileId)), { failure: 'Could not disable' })
/** One kit action at a time: the others wait while one runs. */
const busy = computed(() => [approving, updating, validating, removing, saving, disabling].some((a) => a.pending.value))

const approve = () => kit.value && approving.run(kit.value)
const update = () => kit.value && updating.run(kit.value.id)
const validate = () => kit.value && validating.run(kit.value.id)
async function remove() {
  if (!kit.value) return
  const ok = await app.confirm({
    title: `Remove ${kit.value.name}?`,
    message: kit.value.sourceKind === 'git' ? 'NEXUS deletes its own clone of the repository.' : 'Your folder is not touched; NEXUS only forgets the kit.',
    confirmLabel: 'Remove kit',
    danger: true,
  })
  if (!ok || !kit.value) return
  await removing.run(kit.value.id)
  selectedId.value = null
  await load()
}
const save = (profileId: string, sel: KitSelection) => kit.value && saving.run(kit.value.id, profileId, sel)
const disable = (profileId: string) => kit.value && disabling.run(kit.value.id, profileId)
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
        <ActionButton class="btn-ghost" :pending="refreshing.pending.value" @click="refreshing.run()"><template #icon><RefreshCw /></template>Refresh</ActionButton>
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
            <ActionButton class="btn-sm" :pending="updating.pending.value" :pending-label="kit.sourceKind === 'git' ? 'Fetching the update…' : 'Reading…'" :disabled="busy" data-kit-update @click="update">
              <template #icon><CloudDownload /></template>{{ kit.sourceKind === 'git' ? 'Fetch update' : 'Re-read' }}
            </ActionButton>
            <ActionButton class="btn-sm" :pending="validating.pending.value" pending-label="Validating…" :disabled="busy" data-validate @click="validate">Validate with Claude</ActionButton>
            <ActionButton class="btn-sm btn-ghost" :pending="removing.pending.value" :disabled="busy" title="Remove kit" data-kit-remove @click="remove"><template #icon><Trash2 /></template></ActionButton>
          </div>
        </header>

        <div v-if="kit.status !== 'approved'" class="notice notice-warn approve" data-review>
          <TriangleAlert />
          <span>{{ kit.status === 'changed' ? 'The kit changed since you approved it. Sessions do not get it until you review the new content.' : 'Review the contents below. No session gets this kit until you approve this exact version.' }}</span>
          <ActionButton class="btn-sm btn-primary" :pending="approving.pending.value" pending-label="Approving…" :disabled="busy || kit.status === 'missing'" data-approve @click="approve">
            <template #icon><ShieldCheck /></template>Approve this version
          </ActionButton>
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
