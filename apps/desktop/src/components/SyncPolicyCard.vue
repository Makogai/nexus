<script setup lang="ts">
// Sync policy (Portable Workspace P7, ADR-0039): manual, automatic with review, or trusted automatic — chosen on this
// machine only. Trusted needs the person's explicit confirmation that the remote is private, publishes only the records
// NEXUS generates, and is unavailable when the organisation forbids it.
import { computed, onMounted, ref, watch } from 'vue'
import { ShieldCheck } from '@lucide/vue'
import BaseModal from './BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import type { SyncPolicyMode, SyncPolicyView } from '../lib/types'
import { useAppStore } from '../stores/app'

const props = defineProps<{ workspaceId: string }>()
const app = useAppStore()
const view = ref<SyncPolicyView | null>(null)
const confirming = ref(false)
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
  { id: 'trusted', title: 'Trusted automatic', text: 'NEXUS commits and pushes the records it writes to the remote you approve — only those files, after the security inspection, never forced.' },
]
const reviewOff = computed(() => view.value?.managed.reviewAllowed === false)

async function choose(mode: SyncPolicyMode) {
  if (mode === 'trusted') {
    isPrivate.value = false
    confirming.value = true
    return
  }
  await save(mode, false)
}

async function save(mode: SyncPolicyMode, remoteIsPrivate: boolean) {
  busy.value = true
  try {
    view.value = await api.syncPolicySet(props.workspaceId, mode, remoteIsPrivate)
    confirming.value = false
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
          :disabled="busy || (m.id === 'trusted' && !!view.trustedUnavailable) || (m.id === 'review' && reviewOff)"
          :data-sync-policy-choose="m.id"
          @change="choose(m.id)"
        />
        <div>
          <strong>{{ m.title }}</strong>
          <p class="faint">{{ m.text }}</p>
          <p v-if="m.id === 'trusted' && view.trustedUnavailable" class="faint unavailable" data-trusted-unavailable>Unavailable: {{ view.trustedUnavailable }}</p>
          <p v-if="m.id === 'trusted' && view.policy.trust" class="faint">Approved remote <span class="mono">{{ view.policy.trust.remote }}</span> on <span class="mono">{{ view.policy.trust.branch }}</span>.</p>
        </div>
      </label>
    </div>

    <BaseModal :open="confirming" title="Trusted automatic sync" subtitle="NEXUS will publish its own records without asking each time" :width="600" @close="confirming = false">
      <p>Only these files, written by NEXUS, are committed and pushed automatically:</p>
      <ul class="plan" data-trusted-allowlist>
        <li v-for="a in view.allowlist" :key="a" class="mono">{{ a }}</li>
      </ul>
      <p class="faint">To <span class="mono">{{ view.remote }}</span> on <span class="mono">{{ view.branch }}</span>. Tasks, decisions, memories, constitutions and everything you write still wait for your review. Any inspection finding, a changed remote, new commits on the remote or other unpushed commits stop the automatic path and NEXUS asks you instead. NEXUS never force-pushes.</p>
      <label class="confirm"><input v-model="isPrivate" type="checkbox" data-trusted-private /> This remote is private and approved for these records.</label>
      <template #footer>
        <button class="btn btn-ghost" @click="confirming = false">Cancel</button>
        <button class="btn btn-primary" :disabled="!isPrivate || busy" data-trusted-confirm @click="save('trusted', true)">Turn on trusted sync</button>
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
