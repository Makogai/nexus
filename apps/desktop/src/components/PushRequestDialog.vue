<script setup lang="ts">
// Push requests (ADR-0043, ADR-0047): a Claude session that may not push asked NEXUS. The review fetches the remote
// branch first, so a remote that moved on is visible before the person decides; Push stays disabled while it is ahead.
// The outcome stays on screen — pushed (with a link onward) or failed with guidance by kind — and an error is never
// cleared by a reload.
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ArrowUp, CircleCheck, Clipboard, GitBranch, MessageSquareText, RefreshCw, ShieldAlert, TriangleAlert, WifiOff } from '@lucide/vue'
import BaseModal from './BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import { relativeTime } from '../lib/format'
import { behindExplanation, failureGuidance, failureTitle, tellClaudeText } from '../lib/push'
import type { PushRequest, PushReview } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'

const app = useAppStore()
const sessions = useSessionStore()
const router = useRouter()
const review = ref<PushReview | null>(null)
/** Reading the repository failed (the review itself). Replaced by the next successful read. */
const loadError = ref<string | null>(null)
/** A decision that failed with an error (not a recorded outcome). Stays until the person closes the dialog. */
const error = ref<string | null>(null)
/** The recorded outcome of a decision in this dialog (or the failed request it was opened for). */
const outcome = ref<PushRequest | null>(null)
const busy = ref(false)
const loading = ref(false)

async function load(id: string, showFailure = true) {
  loading.value = true
  try {
    review.value = await api.pushRequestReview(id)
    loadError.value = null
    // Opened for a request that already failed: show why, above the fresh review.
    if (showFailure && !outcome.value && review.value.request.status === 'failed') outcome.value = review.value.request
  } catch (e) {
    loadError.value = toIpcError(e).message
  } finally {
    loading.value = false
  }
}

watch(
  () => app.pushRequestId,
  (id) => {
    review.value = null
    loadError.value = null
    error.value = null
    outcome.value = null
    if (id) load(id)
  },
  { immediate: true },
)

function close() {
  app.pushRequestId = null
}

const failed = computed(() => outcome.value?.status === 'failed')
const pushed = computed(() => outcome.value?.status === 'pushed')
/** Push again only from a failed or pending request; never while the remote is ahead or the inspection blocks. */
const canPush = computed(() => {
  const r = review.value
  return !!r && !pushed.value && outcome.value?.status !== 'declined' && !r.inspection.blocked && r.commits.length > 0 && r.behind === 0
})

async function decide(approve: boolean) {
  const r = review.value
  if (!r || busy.value) return
  busy.value = true
  error.value = null
  try {
    const done = await api.pushRequestDecide(r.request.id, approve, r.token)
    outcome.value = done
    app.pushRequestsChanged++
    if (done.status === 'declined') {
      app.toast('info', 'Push declined')
      close()
      return
    }
    // A failure: read again so the panel shows what the remote has now (incoming commits, counts).
    if (done.status === 'failed') await load(done.id)
  } catch (e) {
    error.value = toIpcError(e).message
    // The branch may have moved: show what would be pushed now (the error stays).
    await load(r.request.id)
  } finally {
    busy.value = false
  }
}

async function reviewAgain() {
  const id = app.pushRequestId
  if (!id) return
  error.value = null
  outcome.value = null
  await load(id, false) // reviewing again: the dialog shows the fresh state, Push tries again
}

async function dismiss() {
  const id = outcome.value?.id
  if (!id || busy.value) return
  busy.value = true
  try {
    await api.pushRequestDismiss(id)
    app.pushRequestsChanged++
    close()
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

function openCommits() {
  const pid = review.value?.request.projectId
  close()
  if (pid) router.push({ name: 'project', params: { id: pid }, query: { tab: 'commits' } })
}

/** The requesting session, when it still runs in this NEXUS. */
const requester = computed(() => {
  const sid = outcome.value?.sessionId ?? review.value?.request.sessionId
  const s = sid ? sessions.byId[sid] : null
  return s && s.running && !s.liveElsewhere ? s : null
})

/**
 * Tell Claude (owner decision 2026-11-10): type the fixed NEXUS message into the requesting session's terminal without
 * pressing Enter — the person sends it. Without a running session, the text goes to the clipboard instead.
 */
async function tellClaude() {
  if (!outcome.value) return
  const text = tellClaudeText(outcome.value, review.value)
  const s = requester.value
  try {
    if (s) {
      sessions.openTab(s.id)
      await api.sessionWrite(s.id, text)
      app.toast('info', `Typed into “${s.title}” — read it and press Enter to send it to Claude.`, 7000)
      close()
    } else {
      await navigator.clipboard.writeText(text)
      app.toast('info', 'The session is not running — the message is on the clipboard.', 7000)
    }
  } catch (e) {
    error.value = toIpcError(e).message
  }
}
</script>

<template>
  <BaseModal :open="!!app.pushRequestId" :title="review ? `Push ${review.request.branch} of ${review.projectName}?` : 'Push request'" subtitle="Claude asked to push — NEXUS pushes only after you approve, never forced" :width="660" @close="close">
    <!-- Outcome first: pushed, or failed with what to do next. -->
    <div v-if="pushed && outcome" class="notice notice-ok result" data-push-result="pushed">
      <CircleCheck />
      <div>
        <strong>Pushed</strong>
        <p class="faint">{{ outcome.message }}</p>
      </div>
    </div>
    <div v-else-if="failed && outcome" class="notice notice-error result" data-push-result="failed" :data-push-failure="outcome.failureKind ?? 'other'">
      <TriangleAlert />
      <div>
        <strong>{{ failureTitle(outcome.failureKind) }}</strong>
        <p v-if="outcome.message" class="mono selectable small">{{ outcome.message }}</p>
        <p>{{ failureGuidance(outcome.failureKind) }}</p>
      </div>
    </div>

    <div v-if="review" data-push-review>
      <p v-if="review.request.reason" class="reason" data-push-reason><span class="faint">Claude says</span> “{{ review.request.reason }}”</p>
      <p class="target"><GitBranch /> <strong class="mono">{{ review.request.branch }}</strong> → <span class="mono">{{ review.remote }}/{{ review.remoteBranch }}</span> <span v-if="review.remoteUrl" class="faint mono">{{ review.remoteUrl }}</span> <span v-if="review.firstPush" class="badge">first push</span></p>
      <p v-if="review.fetchError" class="notice notice-warn" data-push-fetch-error>
        <WifiOff />
        <span>Could not check {{ review.remote }} for new commits — {{ failureTitle(review.fetchError.kind).toLowerCase() }}<template v-if="review.fetchError.message"> ({{ review.fetchError.message }})</template>. The counts are as of the last fetch; you can still try the push.</span>
      </p>
      <p v-if="review.moved" class="notice notice-warn" data-push-moved><TriangleAlert /> <span>The branch moved since Claude asked: you are reviewing its current commit.</span></p>
      <div v-if="review.behind > 0" class="notice notice-warn behind" data-push-behind="">
        <TriangleAlert />
        <div>
          <p>{{ behindExplanation(review) }}</p>
          <ul class="commits incoming" data-push-incoming>
            <li v-for="c in review.incoming" :key="c.oid"><span class="mono faint">{{ c.oid.slice(0, 7) }}</span> {{ c.summary }} <span class="faint">· {{ c.author }} · {{ relativeTime(c.timestampMs) }}</span></li>
          </ul>
        </div>
      </div>
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
    <p v-else-if="loading" class="faint" aria-busy="true">Fetching {{ outcome ? '' : 'the remote and ' }}reading the repository…</p>
    <p v-if="loadError" class="notice notice-error selectable" data-push-load-error><TriangleAlert /> <span>{{ loadError }}</span></p>
    <p v-if="error" class="notice notice-error selectable" data-push-error><TriangleAlert /> <span>{{ error }}</span></p>
    <template #footer>
      <template v-if="pushed">
        <button class="btn btn-ghost" @click="close">Close</button>
        <button class="btn btn-primary" data-push-open-commits @click="openCommits">Open Commits</button>
      </template>
      <template v-else>
        <button v-if="failed && outcome?.failureKind === 'rejected'" class="btn" :disabled="busy" data-push-tell-claude :title="requester ? 'Types the message into the session without sending it' : 'Copies the message (the session is not running)'" @click="tellClaude">
          <MessageSquareText v-if="requester" /><Clipboard v-else /> {{ requester ? 'Tell Claude' : 'Copy message for Claude' }}
        </button>
        <button v-if="failed" class="btn btn-ghost" :disabled="busy" data-push-dismiss @click="dismiss">Dismiss</button>
        <button v-if="failed" class="btn btn-ghost" :disabled="busy || loading" data-push-review-again @click="reviewAgain"><RefreshCw :class="{ spin: loading }" /> Review again</button>
        <button v-if="!failed" class="btn btn-ghost" :disabled="busy || !review" data-push-decline @click="decide(false)">Decline</button>
        <button class="btn btn-primary" :disabled="busy || loading || !canPush" :aria-busy="busy" data-push-approve @click="decide(true)">
          <RefreshCw v-if="busy" class="spin" /><ArrowUp v-else /> {{ busy ? 'Pushing…' : failed ? 'Push again' : 'Push' }}
        </button>
      </template>
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
.result {
  margin-bottom: 12px;
  align-items: flex-start;
}
.result p,
.behind p {
  margin: 4px 0 0;
}
.behind {
  align-items: flex-start;
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
.incoming {
  margin-top: 6px;
  max-height: 120px;
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
