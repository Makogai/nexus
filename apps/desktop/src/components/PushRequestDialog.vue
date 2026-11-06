<script setup lang="ts">
// Push requests (ADR-0043): a Claude session that may not push asked NEXUS. The person sees exactly what would travel —
// the outgoing commits, the files, the security inspection — and NEXUS pushes that commit (never forced) or declines.
import { ref, watch } from 'vue'
import { ArrowUp, GitBranch, ShieldAlert, TriangleAlert } from '@lucide/vue'
import BaseModal from './BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import { relativeTime } from '../lib/format'
import type { PushReview } from '../lib/types'
import { useAppStore } from '../stores/app'

const app = useAppStore()
const review = ref<PushReview | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)

async function load(id: string) {
  review.value = null
  error.value = null
  try {
    review.value = await api.pushRequestReview(id)
  } catch (e) {
    error.value = toIpcError(e).message
  }
}

watch(
  () => app.pushRequestId,
  (id) => {
    if (id) load(id)
  },
  { immediate: true },
)

function close() {
  app.pushRequestId = null
}

async function decide(approve: boolean) {
  const r = review.value
  if (!r) return
  busy.value = true
  try {
    const done = await api.pushRequestDecide(r.request.id, approve, r.token)
    app.toast(done.status === 'pushed' ? 'success' : 'info', done.status === 'pushed' ? (done.message ?? 'Pushed') : 'Push declined')
    app.pushRequestsChanged++
    close()
  } catch (e) {
    error.value = toIpcError(e).message
    // The branch may have moved: show what would be pushed now.
    await load(r.request.id)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="!!app.pushRequestId" :title="review ? `Push ${review.request.branch} of ${review.projectName}?` : 'Push request'" subtitle="Claude asked to push — NEXUS pushes only after you approve, never forced" :width="640" @close="close">
    <div v-if="review" data-push-review>
      <p v-if="review.request.reason" class="reason" data-push-reason><span class="faint">Claude says</span> “{{ review.request.reason }}”</p>
      <p class="target"><GitBranch /> <strong class="mono">{{ review.request.branch }}</strong> → <span class="mono">{{ review.remote }}/{{ review.remoteBranch }}</span> <span v-if="review.remoteUrl" class="faint mono">{{ review.remoteUrl }}</span> <span v-if="review.firstPush" class="badge">first push</span></p>
      <p v-if="review.moved" class="notice notice-warn" data-push-moved><TriangleAlert /> <span>The branch moved since Claude asked: you are reviewing its current commit.</span></p>
      <h3 class="small-title">{{ review.commits.length }} commit{{ review.commits.length === 1 ? '' : 's' }} to push</h3>
      <ul class="commits" data-push-commits>
        <li v-for="c in review.commits" :key="c.oid"><span class="mono faint">{{ c.oid.slice(0, 7) }}</span> {{ c.summary }} <span class="faint">· {{ c.author }} · {{ relativeTime(c.timestampMs) }}</span></li>
      </ul>
      <p class="faint small">{{ review.inspection.files }} file(s) inspected.</p>
      <ul v-if="review.inspection.findings.length" class="findings" data-push-findings>
        <li v-for="(f, i) in review.inspection.findings" :key="i" :class="f.severity === 'block' ? 'blocked' : 'warn'"><ShieldAlert /> <span><span class="mono">{{ f.path }}{{ f.line ? `:${f.line}` : '' }}</span> — {{ f.message }}</span></li>
      </ul>
      <p v-if="review.inspection.blocked" class="notice notice-error" data-push-blocked><ShieldAlert /> <span>The security inspection blocks this push. Fix the findings first, or push it yourself after checking.</span></p>
    </div>
    <p v-else-if="!error" class="faint">Reading the repository…</p>
    <p v-if="error" class="notice notice-error selectable"><TriangleAlert /> <span>{{ error }}</span></p>
    <template #footer>
      <button class="btn btn-ghost" :disabled="busy || !review" data-push-decline @click="decide(false)">Decline</button>
      <button class="btn btn-primary" :disabled="busy || !review || review.inspection.blocked || !review.commits.length" data-push-approve @click="decide(true)"><ArrowUp /> {{ busy ? 'Pushing…' : 'Push' }}</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.reason {
  margin: 0 0 10px;
}
.target {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin: 0 0 10px;
}
.target svg,
.findings svg {
  width: 15px;
  height: 15px;
  flex: none;
}
.small-title {
  font-size: 13px;
  margin: 12px 0 6px;
}
.commits,
.findings {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 4px;
  max-height: 220px;
  overflow: auto;
}
.findings li {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}
.findings .blocked {
  color: var(--danger, #f87171);
}
.findings .warn {
  color: var(--warn, #fbbf24);
}
.small {
  font-size: 12px;
}
</style>
