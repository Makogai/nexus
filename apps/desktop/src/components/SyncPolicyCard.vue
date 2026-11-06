<script setup lang="ts">
// Sync policy (Portable Workspace P7, ADR-0039; ADR-0042): manual, automatic with review, trusted automatic (NEXUS's
// records only) or autonomous (everything, after each Claude turn) — chosen on this machine only. Trusted and autonomous
// need the person's explicit confirmation that the remote is private, and are unavailable when the organisation forbids
// them.
import { computed, onMounted, ref, watch } from 'vue'
import { ShieldCheck } from '@lucide/vue'
import BaseModal from './BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import type { SyncPolicyMode, SyncPolicyView } from '../lib/types'
import { useAppStore } from '../stores/app'

const props = defineProps<{ workspaceId: string }>()
const app = useAppStore()
const view = ref<SyncPolicyView | null>(null)
/** The automatic mode waiting for the private-remote confirmation. */
const confirming = ref<'trusted' | 'autonomous' | null>(null)
const isPrivate = ref(false)
const busy = ref(false)

async function load() {
  try {
    view.value = await api.syncPolicyGet(props.workspaceId)
  } catch (e) {
    app.toast('error', `Could not read the sync policy: ${toIpcError(e).message}`)
  }
}
onMounted(load)
watch(() => props.workspaceId, load)
defineExpose({ load })

const MODES: { id: SyncPolicyMode; title: string; text: string }[] = [
  { id: 'manual', title: 'Manual', text: 'Records are saved in the workspace folder. Nothing is committed or published until you review a commit and a push.' },
  { id: 'review', title: 'Automatic with review', text: 'When NEXUS writes development records it asks you to review the commit and the push. Publishing still needs your confirmation.' },
  { id: 'trusted', title: 'Records only, automatically', text: 'NEXUS commits and pushes the records it writes (development records, current state, project facts) to the remote you approve — only those files.' },
  { id: 'autonomous', title: 'Everything, automatically', text: 'When a Claude session finishes a turn or ends, NEXUS commits every workspace change with the message Claude proposed and pushes it — after the security inspection, never forced. A problem stops it and asks you once.' },
]
const reviewOff = computed(() => view.value?.managed.reviewAllowed === false)
function unavailable(id: SyncPolicyMode): string | null {
  if (id === 'trusted') return view.value?.trustedUnavailable ?? null
  if (id === 'autonomous') return view.value?.autonomousUnavailable ?? null
  return id === 'review' && reviewOff.value ? 'turned off by your organisation' : null
}

async function choose(mode: SyncPolicyMode) {
  if (mode === 'trusted' || mode === 'autonomous') {
    isPrivate.value = false
    confirming.value = mode
    return
  }
  await save(mode, false)
}

async function save(mode: SyncPolicyMode, remoteIsPrivate: boolean) {
  busy.value = true
  try {
    view.value = await api.syncPolicySet(props.workspaceId, mode, remoteIsPrivate)
    confirming.value = null
    app.toast('success', `Sync policy: ${MODES.find((m) => m.id === mode)?.title}`)
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <section v-if="view" class="card" data-sync-policy :data-sync-policy-mode="view.policy.mode">
    <h2 class="card-title"><ShieldCheck /> Sync policy <span class="faint">(this computer)</span></h2>
    <div class="modes">
      <label v-for="m in MODES" :key="m.id" class="mode" :class="{ active: view.policy.mode === m.id }">
        <input
          type="radio"
          name="sync-policy"
          :value="m.id"
          :checked="view.policy.mode === m.id"
          :disabled="busy || !!unavailable(m.id)"
          :data-sync-policy-choose="m.id"
          @change="choose(m.id)"
        />
        <div>
          <strong>{{ m.title }}</strong>
          <p class="faint">{{ m.text }}</p>
          <p v-if="m.id === 'trusted' && view.trustedUnavailable" class="faint unavailable" data-trusted-unavailable>Unavailable: {{ view.trustedUnavailable }}</p>
          <p v-if="m.id === 'autonomous' && view.autonomousUnavailable" class="faint unavailable" data-autonomous-unavailable>Unavailable: {{ view.autonomousUnavailable }}</p>
          <p v-if="m.id === view.policy.mode && view.policy.trust" class="faint">Approved remote <span class="mono">{{ view.policy.trust.remote }}</span> on <span class="mono">{{ view.policy.trust.branch }}</span>.</p>
        </div>
      </label>
    </div>

    <BaseModal :open="confirming === 'trusted'" title="Records only, automatically" subtitle="NEXUS will publish its own records without asking each time" :width="600" @close="confirming = null">
      <p>Only these files, written by NEXUS, are committed and pushed automatically:</p>
      <ul class="plan" data-trusted-allowlist>
        <li v-for="a in view.allowlist" :key="a" class="mono">{{ a }}</li>
      </ul>
      <p class="faint">To <span class="mono">{{ view.remote }}</span> on <span class="mono">{{ view.branch }}</span>. Tasks, decisions, memories, constitutions and everything you write still wait for your review. Any inspection finding, a changed remote, new commits on the remote or other unpushed commits stop the automatic path and NEXUS asks you instead. NEXUS never force-pushes.</p>
      <label class="confirm"><input v-model="isPrivate" type="checkbox" data-trusted-private /> This remote is private and approved for these records.</label>
      <template #footer>
        <button class="btn btn-ghost" @click="confirming = null">Cancel</button>
        <button class="btn btn-primary" :disabled="!isPrivate || busy" data-trusted-confirm @click="save('trusted', true)">Turn on</button>
      </template>
    </BaseModal>

    <BaseModal :open="confirming === 'autonomous'" title="Everything, automatically" subtitle="NEXUS will commit and push this workspace on its own" :width="600" @close="confirming = null">
      <p>When a Claude session finishes a turn or ends, NEXUS commits <strong>every change in the workspace repository</strong> — tasks, notes, decisions, drafts, rules, records and your own edits — and pushes it to <span class="mono">{{ view.remote }}</span> on <span class="mono">{{ view.branch }}</span>.</p>
      <ul class="plan">
        <li>The commit message is the one Claude proposed for its work; otherwise NEXUS composes one from the changed files.</li>
        <li>Every commit passes the same security inspection as a reviewed one. A finding (a secret, a private file, a nested repository), a changed remote or branch, or new commits on the remote stop it — NEXUS then asks you once in a notification.</li>
        <li>NEXUS never force-pushes, and machine-local state (sessions, usage, profiles, approvals) never travels.</li>
        <li>Other computers still approve synced rules, constitutions, workflows and kits before their sessions use them.</li>
      </ul>
      <label class="confirm"><input v-model="isPrivate" type="checkbox" data-autonomous-private /> This remote is private and approved for everything in this workspace.</label>
      <template #footer>
        <button class="btn btn-ghost" @click="confirming = null">Cancel</button>
        <button class="btn btn-primary" :disabled="!isPrivate || busy" data-autonomous-confirm @click="save('autonomous', true)">Turn on</button>
      </template>
    </BaseModal>
  </section>
</template>

<style scoped>
.modes {
  display: grid;
  gap: 8px;
  margin-top: 6px;
}
.mode {
  display: flex;
  gap: 10px;
  align-items: flex-start;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.mode.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.mode p {
  margin: 2px 0 0;
  font-size: 12.5px;
}
.unavailable {
  color: var(--amber);
}
.plan {
  padding-left: 18px;
  font-size: 12.5px;
}
.confirm {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-top: 10px;
  font-weight: 500;
}
</style>
