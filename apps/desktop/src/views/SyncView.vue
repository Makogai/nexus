<script setup lang="ts">
// Sync Center (ADR-0031, M29): the workspace's Git repository — status, local changes, incoming and outgoing
// commits, diffs, a reviewed commit and a reviewed push (each confirmed by the person, after a security inspection),
// fast-forward-only pulls, history, and onboarding from Git. Nothing is published automatically.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { listen } from '@tauri-apps/api/event'
import { ArrowDown, ArrowUp, CircleAlert, CloudDownload, FolderSync, GitBranch, GitCommitHorizontal, Info, RefreshCw, ShieldAlert, ShieldCheck, TriangleAlert } from '@lucide/vue'
import { api, SYNC_EVENT, toIpcError } from '../lib/ipc'
import type { SyncCloned, SyncCommitReview, SyncFileDiff, SyncPulled, SyncPushReview, SyncRestore, SyncSide, SyncStatus } from '../lib/types'
import { relativeTime } from '../lib/format'
import { useAction } from '../composables/useAction'
import ActionButton from '../components/ActionButton.vue'
import BaseModal from '../components/BaseModal.vue'
import CloneWorkspaceForm from '../components/CloneWorkspaceForm.vue'
import CopyFindings from '../components/CopyFindings.vue'
import SyncPolicyCard from '../components/SyncPolicyCard.vue'
import { useAppStore } from '../stores/app'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const route = useRoute()
const router = useRouter()

const wsId = computed(() => workspaces.active?.id ?? null)
const st = ref<SyncStatus | null>(null)
const busy = ref<string | null>(null)
/** Files the person left out of the next commit (everything else that is not blocked by name is included). */
const excluded = ref<Set<string>>(new Set())
const blockedByPath = (w: { findings: { severity: string }[] }) => w.findings.some((f) => f.severity === 'block')
const selected = computed(() => new Set((st.value?.working ?? []).filter((w) => !blockedByPath(w) && !excluded.value.has(w.path)).map((w) => w.path)))
const diff = ref<(SyncFileDiff & { side: SyncSide }) | null>(null)
const commitReview = ref<SyncCommitReview | null>(null)
const commitMessage = ref('')
const pushReview = ref<SyncPushReview | null>(null)
const pulled = ref<SyncPulled | null>(null)
const restore = ref<SyncRestore | null>(null)
const cloning = ref(false)

const STATE: Record<SyncStatus['state'], { label: string; tone: 'ok' | 'warn' | 'error' | 'accent' | 'muted' }> = {
  'not-repo': { label: 'Not a Git repository', tone: 'muted' },
  conflicts: { label: 'Conflicts', tone: 'error' },
  'in-progress': { label: 'Git operation in progress', tone: 'warn' },
  'no-remote': { label: 'No remote', tone: 'muted' },
  'no-upstream': { label: 'Not pushed yet', tone: 'accent' },
  diverged: { label: 'Diverged', tone: 'error' },
  behind: { label: 'Incoming changes', tone: 'accent' },
  ahead: { label: 'Outgoing changes', tone: 'accent' },
  'up-to-date': { label: 'Up to date', tone: 'ok' },
}
const OP_LABEL: Record<string, string> = { fetch: 'Fetch', pull: 'Pull', commit: 'Commit', push: 'Push', clone: 'Clone', 'auto-sync': 'Automatic sync' }

async function load() {
  if (!wsId.value) return
  try {
    st.value = await api.syncStatus(wsId.value)
  } catch (e) {
    app.toast('error', `Could not read the repository: ${toIpcError(e).message}`)
  }
}

const refreshing = useAction(load)

/** One Git operation at a time; `busy` names it, so its button shows it runs and the others wait. */
async function run<T>(what: string, f: () => Promise<T>): Promise<T | null> {
  if (busy.value) return null
  busy.value = what
  try {
    return await f()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
    return null
  } finally {
    busy.value = null
    await load()
  }
}

async function fetchNow() {
  const r = await run('fetch', () => api.syncFetch(wsId.value!))
  if (r) app.toast('success', r.incoming ? `${r.incoming} incoming commit${r.incoming === 1 ? '' : 's'} from ${r.remote}` : `Nothing new on ${r.remote}`)
}

async function pull() {
  const n = st.value?.incoming.length ?? 0
  const ok = await app.confirm({
    title: `Fast-forward ${n} commit${n === 1 ? '' : 's'}?`,
    message: `Your workspace files are updated to ${st.value?.readiness.upstream ?? 'the remote branch'}. Synced rules, constitutions, workflows and AI Kits that changed stay inactive until you approve them.`,
    confirmLabel: 'Pull',
  })
  if (!ok) return
  const r = await run('pull', () => api.syncPull(wsId.value!))
  if (r) {
    pulled.value = r
    app.toast('success', r.commits ? `Fast-forwarded ${r.commits} commit${r.commits === 1 ? '' : 's'}` : 'Already up to date')
  }
}

/** The file whose diff is being read (`side:path`). */
const diffing = ref<string | null>(null)
async function showDiff(side: SyncSide, path: string) {
  diffing.value = `${side}:${path}`
  const r = await run('diff', () => api.syncDiff(wsId.value!, side, path))
  diffing.value = null
  if (r) diff.value = { ...r, side }
}
const diffBusy = (side: SyncSide, path: string) => (busy.value === 'diff' && diffing.value === `${side}:${path}` ? 'true' : undefined)
const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? '' : 's'}`

function toggle(path: string) {
  const s = new Set(excluded.value)
  if (s.has(path)) s.delete(path)
  else s.add(path)
  excluded.value = s
}

async function reviewCommit() {
  const paths = [...selected.value]
  const r = await run('review', () => api.syncReviewCommit(wsId.value!, paths))
  if (r) {
    commitReview.value = r
    // Claude's message for this work (or one NEXUS composed) — kept when you already wrote your own.
    if (!commitMessage.value.trim()) commitMessage.value = r.suggestedMessage
  }
}

async function commit() {
  const r = commitReview.value
  if (!r || r.inspection.blocked || !commitMessage.value.trim()) return
  const ok = await app.confirm({
    title: `Commit ${r.files.length} file${r.files.length === 1 ? '' : 's'} to ${r.branch ?? 'the branch'}?`,
    message: `Exactly the reviewed changes are committed with the message “${commitMessage.value.trim().split('\n')[0]}”. Nothing is pushed.`,
    confirmLabel: 'Commit',
  })
  if (!ok) return
  const paths = r.files.map((f) => f.path)
  const done = await run('commit', () => api.syncCommit(wsId.value!, commitMessage.value, paths, r.token))
  if (done) {
    commitReview.value = null
    commitMessage.value = ''
    app.toast('success', `Committed ${done.files} file${done.files === 1 ? '' : 's'} — nothing was pushed`)
  }
}

async function reviewPush() {
  const r = await run('review-push', () => api.syncReviewPush(wsId.value!))
  if (r) pushReview.value = r
}

async function push() {
  const r = pushReview.value
  if (!r || r.inspection.blocked || r.behind > 0 || !r.commits.length) return
  const ok = await app.confirm({
    title: `Push ${r.commits.length} commit${r.commits.length === 1 ? '' : 's'} to ${r.remote}/${r.branch}?`,
    message: r.firstPush
      ? 'This is the first push of this branch: its whole history goes to the remote. Anyone with access to the remote can read it.'
      : 'Exactly the reviewed commits go to the remote. Anyone with access to the remote can read them.',
    confirmLabel: 'Push',
    danger: r.firstPush,
  })
  if (!ok) return
  const done = await run('push', () => api.syncPush(wsId.value!, r.token))
  if (done) {
    pushReview.value = null
    app.toast('success', `Pushed ${done.commits} commit${done.commits === 1 ? '' : 's'} to ${done.remote}/${done.branch}`)
  }
}

async function showRestore() {
  restore.value = await run('restore', () => api.syncRestore(wsId.value!))
}

async function cloned(r: SyncCloned) {
  cloning.value = false
  restore.value = r.restore
  app.toast('success', `Workspace “${r.workspace.name}” opened from Git`)
  await load()
}

function lines(text: string) {
  return text.split('\n').map((l) => ({ l, cls: l.startsWith('+') && !l.startsWith('+++') ? 'add' : l.startsWith('-') && !l.startsWith('---') ? 'del' : l.startsWith('@@') ? 'hunk' : '' }))
}

const blockers = (findings: { severity: string }[]) => findings.filter((f) => f.severity === 'block').length
const canPull = computed(() => st.value?.state === 'behind')
const canPush = computed(() => !!st.value && (st.value.outgoing.length > 0) && ['ahead', 'no-upstream', 'up-to-date'].includes(st.value.state))

let unlisten: (() => void) | null = null
// Files change in editors and Git tools too: read the repository again when NEXUS gets the focus back.
const onFocus = () => {
  if (!busy.value) void load()
}
onMounted(async () => {
  window.addEventListener('focus', onFocus)
  try {
    unlisten = await listen(SYNC_EVENT, () => void load())
  } catch {
    unlisten = null
  }
  await load()
  if (route.query.restore && wsId.value) {
    await showRestore()
    void router.replace({ query: {} })
  }
})
onBeforeUnmount(() => {
  unlisten?.()
  window.removeEventListener('focus', onFocus)
})
watch(wsId, () => {
  st.value = null
  diff.value = null
  void load()
})
</script>

<template>
  <div class="page page-flex sync" data-sync-view :data-sync-state="st?.state ?? 'loading'">
    <div class="page-header">
      <div>
        <h1>Sync</h1>
        <p>Your workspace travels through its own Git repository — no sync service. Commits and pushes happen only when you confirm them, after a security inspection. Sessions, usage, notifications, gate commands, approvals, profiles and credentials never leave this machine.</p>
      </div>
      <div class="page-actions">
        <ActionButton class="btn-ghost" :pending="refreshing.pending.value" :disabled="!!busy" title="Read the repository again (local changes made outside NEXUS)" data-sync-refresh @click="refreshing.run()">
          <template #icon><RefreshCw /></template>Refresh
        </ActionButton>
        <ActionButton :pending="busy === 'fetch'" :pending-label="`Fetching from ${st?.remoteName ?? 'the remote'}…`" :disabled="!!busy || !st?.readiness.repo || st?.state === 'no-remote'" data-sync-fetch @click="fetchNow">
          <template #icon><CloudDownload /></template>Fetch
        </ActionButton>
        <ActionButton :pending="busy === 'pull'" :pending-label="`Pulling ${plural(st?.incoming.length ?? 0, 'commit')}…`" :disabled="!!busy || !canPull" data-sync-pull @click="pull">
          <template #icon><ArrowDown /></template>Pull
        </ActionButton>
        <ActionButton :pending="busy === 'review'" pending-label="Inspecting the changes…" :disabled="!!busy || !st?.working.length || !selected.size" data-sync-review @click="reviewCommit">
          <template #icon><GitCommitHorizontal /></template>Review &amp; commit
        </ActionButton>
        <ActionButton class="btn-primary" :pending="busy === 'review-push'" pending-label="Inspecting the commits…" :disabled="!!busy || !canPush" data-sync-push @click="reviewPush">
          <template #icon><ArrowUp /></template>Push…
        </ActionButton>
      </div>
    </div>

    <div v-if="!workspaces.active" class="empty">
      <FolderSync class="empty-icon" />
      <h3>No workspace yet</h3>
      <p>Open a workspace from Git to start on this machine.</p>
    </div>

    <template v-else-if="st">
      <SyncPolicyCard v-if="st.readiness.repo && wsId" :workspace-id="wsId" />

      <!-- Status -->
      <section class="card status" data-sync-status>
        <div class="status-row">
          <span class="badge" :class="`badge-${STATE[st.state].tone}`" data-sync-badge>{{ STATE[st.state].label }}</span>
          <span v-if="st.readiness.branch"><GitBranch class="ic" /> <strong class="mono">{{ st.readiness.branch }}</strong></span>
          <span v-if="st.readiness.upstream" class="faint">tracks <span class="mono">{{ st.readiness.upstream }}</span></span>
          <span v-if="st.readiness.remote" class="faint mono remote" :title="st.readiness.remote">{{ st.readiness.remote }}</span>
          <span class="faint">· last fetch {{ st.lastFetchAt ? relativeTime(st.lastFetchAt) : 'never' }}</span>
          <span class="spacer" />
          <ActionButton class="btn-sm btn-ghost" :pending="busy === 'restore'" :disabled="!!busy" data-sync-restore-open @click="showRestore">What does this machine need?</ActionButton>
          <button class="btn btn-sm btn-ghost" data-sync-clone-open @click="cloning = true">Open another from Git…</button>
        </div>
        <p v-if="st.state === 'not-repo'" class="notice"><Info /> <span>The workspace folder is not a Git repository of its own yet. Run <code>git init</code> in it and add a remote with your Git tool (for example a private repository) — then fetch, commit and push here.</span></p>
        <p v-if="st.inProgress" class="notice notice-warn" data-sync-in-progress><TriangleAlert /> {{ st.inProgress }}</p>
        <p v-if="st.readiness.repo && !st.identity" class="notice notice-warn"><TriangleAlert /> <span>Git has no name and e-mail for commits here — set them with <code>git config user.name</code> and <code>user.email</code>. NEXUS does not invent an identity.</span></p>
        <ul v-if="st.readiness.issues.length" class="issues">
          <li v-for="(i, n) in st.readiness.issues" :key="n" :class="`issue-${i.severity}`" :data-sync-issue="i.severity">
            <CircleAlert v-if="i.severity === 'error'" class="ic" /><TriangleAlert v-else-if="i.severity === 'warning'" class="ic" /><Info v-else class="ic" /> {{ i.message }}
          </li>
        </ul>
      </section>

      <div class="columns">
        <!-- Local changes -->
        <section class="card" data-sync-working>
          <div class="card-title">Local changes <span class="faint">{{ st.working.length }}</span></div>
          <p v-if="!st.working.length" class="faint small">Nothing changed since the last commit.</p>
          <ul class="files">
            <li v-for="w in st.working" :key="w.path" :data-sync-file="w.path" :data-sync-blocked="blockers(w.findings) ? 'yes' : 'no'">
              <input type="checkbox" :checked="selected.has(w.path)" :disabled="blockedByPath(w)" :title="blockedByPath(w) ? 'Never synced' : ''" :aria-label="`Include ${w.path}`" @change="toggle(w.path)" />
              <span class="st" :class="`st-${w.status}`">{{ w.status }}</span>
              <button class="link mono" :disabled="!!busy" :aria-busy="diffBusy('working', w.path)" @click="showDiff('working', w.path)">{{ w.path }}</button>
              <span v-for="f in w.findings" :key="f.kind" class="badge" :class="f.severity === 'block' ? 'badge-error' : 'badge-warn'" :title="f.message">{{ f.kind }}</span>
            </li>
          </ul>
        </section>

        <!-- Incoming -->
        <section class="card" data-sync-incoming>
          <div class="card-title"><ArrowDown /> Incoming <span class="faint">{{ st.incoming.length }}</span></div>
          <p v-if="!st.incoming.length" class="faint small">{{ st.readiness.upstream ? 'Nothing new as of the last fetch.' : 'No upstream yet.' }}</p>
          <ul class="commits">
            <li v-for="c in st.incoming" :key="c.oid" :data-sync-commit="c.oid"><span class="mono faint">{{ c.oid.slice(0, 8) }}</span> {{ c.summary }} <span class="faint small">· {{ c.author }} · {{ relativeTime(c.timestampMs) }}</span></li>
          </ul>
          <ul v-if="st.incomingFiles.length" class="files">
            <li v-for="f in st.incomingFiles" :key="f.path" :data-sync-incoming-file="f.path"><span class="st">{{ f.status }}</span><button class="link mono" :disabled="!!busy" :aria-busy="diffBusy('incoming', f.path)" @click="showDiff('incoming', f.path)">{{ f.path }}</button></li>
          </ul>
        </section>

        <!-- Outgoing -->
        <section class="card" data-sync-outgoing>
          <div class="card-title"><ArrowUp /> Outgoing <span class="faint">{{ st.outgoing.length }}</span></div>
          <p v-if="!st.outgoing.length" class="faint small">No commits waiting to be pushed.</p>
          <ul class="commits">
            <li v-for="c in st.outgoing" :key="c.oid" :data-sync-outgoing-commit="c.oid"><span class="mono faint">{{ c.oid.slice(0, 8) }}</span> {{ c.summary }} <span class="faint small">· {{ relativeTime(c.timestampMs) }}</span></li>
          </ul>
          <ul v-if="st.outgoingFiles.length" class="files">
            <li v-for="f in st.outgoingFiles" :key="f.path"><span class="st">{{ f.status }}</span><button class="link mono" :disabled="!!busy" :aria-busy="diffBusy('outgoing', f.path)" @click="showDiff('outgoing', f.path)">{{ f.path }}</button></li>
          </ul>
        </section>
      </div>

      <section v-if="diff" class="card" data-sync-diff>
        <div class="card-title">{{ diff.side === 'working' ? 'Local change' : diff.side === 'incoming' ? 'Incoming' : 'Outgoing' }} · <span class="mono">{{ diff.path }}</span><span class="spacer" /><button class="btn btn-sm btn-ghost" @click="diff = null">Close</button></div>
        <p v-if="diff.binary" class="faint">Binary file.</p>
        <pre v-else class="diff selectable"><span v-for="(x, i) in lines(diff.text)" :key="i" :class="x.cls">{{ x.l }}
</span></pre>
        <p v-if="diff.truncated" class="faint small">The diff is longer than shown — open the file in your editor or Git tool.</p>
      </section>

      <section v-if="pulled" class="card" data-sync-pulled>
        <div class="card-title">Last pull <span class="spacer" /><button class="btn btn-sm btn-ghost" @click="pulled = null">Close</button></div>
        <p>{{ pulled.commits }} commit{{ pulled.commits === 1 ? '' : 's' }}, {{ pulled.files.length }} file{{ pulled.files.length === 1 ? '' : 's' }}.</p>
        <div v-if="pulled.needsApproval.length || pulled.kitsChanged.length" class="notice notice-warn" data-sync-needs-review>
          <ShieldAlert /> Inactive until you review them:
          <ul>
            <li v-for="n in pulled.needsApproval" :key="n.id">{{ n.kind }} “{{ n.title }}” — <router-link :to="n.kind === 'workflow' ? '/workflows' : '/constitutions?pending=1'">review</router-link></li>
            <li v-for="k in pulled.kitsChanged" :key="k">AI Kit “{{ k }}” — <router-link to="/kits">review</router-link></li>
          </ul>
        </div>
        <ul v-if="pulled.warnings.length" class="issues">
          <li v-for="(w, i) in pulled.warnings" :key="i" class="issue-warning"><TriangleAlert class="ic" /> {{ w.path ? `${w.path}: ` : '' }}{{ w.message }}</li>
        </ul>
      </section>

      <section v-if="restore" class="card" data-sync-restore>
        <div class="card-title">Restoring “{{ restore.name }}” on this machine <span class="spacer" /><button class="btn btn-sm btn-ghost" @click="restore = null">Close</button></div>
        <ol class="steps">
          <li v-for="(s, i) in restore.steps" :key="i">{{ s }}</li>
        </ol>
        <p v-if="restore.projects.length" class="small faint">Projects: <span v-for="p in restore.projects" :key="p.id" class="proj">{{ p.name }} {{ p.available ? '✓' : '(not on this machine)' }}</span></p>
      </section>

      <!-- History -->
      <section class="card" data-sync-history>
        <div class="card-title">History</div>
        <table v-if="st.history.length" class="table">
          <thead><tr><th>When</th><th>Operation</th><th>Result</th><th>Detail</th></tr></thead>
          <tbody>
            <tr v-for="h in st.history" :key="h.id" :data-sync-history-row="`${h.op}:${h.status}`">
              <td class="nowrap">{{ relativeTime(h.startedAt) }}</td>
              <td>{{ OP_LABEL[h.op] ?? h.op }}</td>
              <td><span class="badge" :class="{ 'badge-ok': h.status === 'ok', 'badge-error': h.status === 'failed', 'badge-warn': h.status === 'refused' || h.status === 'interrupted', 'badge-live': h.status === 'running' }">{{ h.status }}</span> <span v-if="h.errorKind" class="faint small">{{ h.errorKind }}</span></td>
              <td class="small">{{ h.message }}</td>
            </tr>
          </tbody>
        </table>
        <p v-else class="faint small">No sync operations yet.</p>
      </section>
    </template>

    <!-- Commit review -->
    <BaseModal :open="!!commitReview" title="Review the commit" :width="720" @close="commitReview = null">
      <div v-if="commitReview" data-sync-commit-dialog>
        <p class="small">On <strong class="mono">{{ commitReview.branch }}</strong> — {{ commitReview.files.length }} file{{ commitReview.files.length === 1 ? '' : 's' }}. Nothing is pushed.</p>
        <ul class="files">
          <li v-for="f in commitReview.files" :key="f.path"><span class="st">{{ f.status }}</span> <span class="mono">{{ f.path }}</span></li>
        </ul>
        <div class="inspection" :class="{ blocked: commitReview.inspection.blocked }" data-sync-inspection>
          <div class="ins-head"><ShieldAlert v-if="commitReview.inspection.blocked" /><ShieldCheck v-else /> {{ commitReview.inspection.blocked ? 'The security inspection blocks this commit' : commitReview.inspection.findings.length ? 'Warnings to check' : 'Nothing found by the security inspection' }}<CopyFindings :findings="commitReview.inspection.findings" /></div>
          <ul class="selectable">
            <li v-for="(f, i) in commitReview.inspection.findings" :key="i" :data-sync-finding="`${f.kind}:${f.severity}`">
              <span class="badge" :class="f.severity === 'block' ? 'badge-error' : 'badge-warn'">{{ f.severity === 'block' ? 'blocks' : 'warning' }}</span>
              <span class="mono">{{ f.path }}{{ f.line ? `:${f.line}` : '' }}</span> — {{ f.message }}
            </li>
          </ul>
          <p v-if="commitReview.inspection.blocked" class="small">Leave the blocking files out (untick them) or remove what was found, then review again.</p>
        </div>
        <textarea v-model="commitMessage" class="textarea message" rows="5" placeholder="Commit message" aria-label="Commit message" data-sync-message />
      </div>
      <template #footer>
        <button class="btn" @click="commitReview = null">Cancel</button>
        <ActionButton
          class="btn-primary"
          :pending="busy === 'commit'"
          :pending-label="`Committing ${plural(commitReview?.files.length ?? 0, 'file')}…`"
          :disabled="!commitReview || commitReview.inspection.blocked || !commitReview.identity || !commitMessage.trim() || !!busy"
          data-sync-commit-confirm
          @click="commit"
        >
          Commit…
        </ActionButton>
      </template>
    </BaseModal>

    <!-- Push review -->
    <BaseModal :open="!!pushReview" title="Review the push" :width="720" @close="pushReview = null">
      <div v-if="pushReview" data-sync-push-dialog>
        <p class="small">To <strong class="mono">{{ pushReview.remote }}/{{ pushReview.branch }}</strong> — {{ pushReview.commits.length }} commit{{ pushReview.commits.length === 1 ? '' : 's' }}, {{ pushReview.files.length }} file{{ pushReview.files.length === 1 ? '' : 's' }}.</p>
        <p v-if="pushReview.firstPush" class="notice notice-warn small"><TriangleAlert /> First push of this branch: its whole history travels.</p>
        <p v-if="pushReview.behind" class="notice notice-error small"><CircleAlert /> The remote has {{ pushReview.behind }} commit(s) you do not have — fetch and pull first. NEXUS never force-pushes.</p>
        <ul class="commits">
          <li v-for="c in pushReview.commits" :key="c.oid"><span class="mono faint">{{ c.oid.slice(0, 8) }}</span> {{ c.summary }}</li>
        </ul>
        <div class="inspection" :class="{ blocked: pushReview.inspection.blocked }" data-sync-push-inspection>
          <div class="ins-head"><ShieldAlert v-if="pushReview.inspection.blocked" /><ShieldCheck v-else /> {{ pushReview.inspection.blocked ? 'The security inspection blocks this push' : pushReview.inspection.findings.length ? 'Warnings to check' : 'Nothing found by the security inspection' }}<CopyFindings :findings="pushReview.inspection.findings" /></div>
          <ul class="selectable">
            <li v-for="(f, i) in pushReview.inspection.findings" :key="i" :data-sync-finding="`${f.kind}:${f.severity}`">
              <span class="badge" :class="f.severity === 'block' ? 'badge-error' : 'badge-warn'">{{ f.severity === 'block' ? 'blocks' : 'warning' }}</span>
              <span class="mono">{{ f.path }}{{ f.line ? `:${f.line}` : '' }}</span> — {{ f.message }}
            </li>
          </ul>
          <p v-if="pushReview.inspection.blocked" class="small">The history contains something that must not travel — fix it in your Git tool before pushing.</p>
        </div>
      </div>
      <template #footer>
        <button class="btn" @click="pushReview = null">Cancel</button>
        <ActionButton
          class="btn-primary"
          :pending="busy === 'push'"
          :pending-label="`Pushing ${plural(pushReview?.commits.length ?? 0, 'commit')} to ${pushReview?.remote ?? 'the remote'}…`"
          :disabled="!pushReview || pushReview.inspection.blocked || pushReview.behind > 0 || !pushReview.commits.length || !!busy"
          data-sync-push-confirm
          @click="push"
        >
          Push…
        </ActionButton>
      </template>
    </BaseModal>

    <BaseModal :open="cloning" title="Open a workspace from Git" :width="620" @close="cloning = false">
      <CloneWorkspaceForm @cloned="cloned" @cancel="cloning = false" />
    </BaseModal>
  </div>
</template>

<style scoped>
.sync {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.status-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 10px;
  font-size: 13px;
}
.remote {
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.spacer {
  flex: 1;
}
.ic {
  width: 14px;
  height: 14px;
  vertical-align: -2px;
}
.badge-muted {
  color: var(--text-muted);
  background: var(--bg-hover);
}
.issues {
  margin: 10px 0 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
}
.issue-error {
  color: var(--red);
}
.issue-warning {
  color: var(--amber);
}
.issue-info {
  color: var(--text-muted);
}
.columns {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 14px;
}
.files,
.commits {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
}
.commits {
  margin-bottom: 8px;
}
.files li {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.st {
  min-width: 62px;
  font-size: 11px;
  text-transform: uppercase;
  color: var(--text-faint);
}
.st-added {
  color: var(--green);
}
.st-deleted,
.st-conflicted {
  color: var(--red);
}
.link {
  border: none;
  background: none;
  padding: 0;
  color: var(--text);
  cursor: pointer;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.link:hover:not(:disabled) {
  text-decoration: underline;
}
.link:disabled {
  cursor: default;
}
.link[aria-busy='true'] {
  cursor: progress;
  color: var(--text-muted);
}
.diff {
  max-height: 420px;
  overflow: auto;
  margin: 0;
  padding: 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-input);
  font-family: var(--font-mono);
  font-size: 12px;
  white-space: pre-wrap;
}
.diff .add {
  color: var(--green);
}
.diff .del {
  color: var(--red);
}
.diff .hunk {
  color: var(--cyan);
}
.inspection {
  margin: 12px 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border);
  font-size: 13px;
}
.inspection.blocked {
  border-color: var(--red);
}
.ins-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-weight: 600;
  margin-bottom: 6px;
}
.ins-head svg {
  width: 16px;
  height: 16px;
}
.ins-head .btn {
  margin-left: auto;
  font-weight: normal;
}
.inspection ul {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.message {
  width: 100%;
  font-family: var(--font-ui);
}
.steps {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
}
.proj {
  margin-right: 10px;
}
.table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.table th {
  text-align: left;
  font-size: 12px;
  font-weight: 500;
  color: var(--text-faint);
  padding: 6px;
}
.table td {
  padding: 6px;
  border-top: 1px solid var(--border);
  vertical-align: top;
}
.nowrap {
  white-space: nowrap;
}
.small {
  font-size: 12px;
}
.card-title {
  display: flex;
  align-items: center;
  gap: 6px;
}
</style>
