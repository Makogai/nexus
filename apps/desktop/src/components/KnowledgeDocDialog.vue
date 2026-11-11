<script setup lang="ts">
// A knowledge document (W2): read it formatted — Markdown, or a constitution's rule cards — and edit it in place. Edits
// are optimistic (the version that was read) and validated in the core; the saved version is approved on this machine.
import { Workflow as WorkflowIcon } from '@lucide/vue'
import UsageSummary from './UsageSummary.vue'
import { router as appRouter } from '../router'
import { computed, ref, watch } from 'vue'
import { FolderOpen, GitBranch, LoaderCircle, Pencil, Save, Scale, ShieldCheck, Sparkles, TriangleAlert, X } from '@lucide/vue'
import { revealItemInDir } from '@tauri-apps/plugin-opener'
import { api, toIpcError } from '../lib/ipc'
import { KIND_LABELS, STATUS_LABELS } from '../lib/knowledge'
import type { ConstitutionStructure, KnowledgeDoc, KnowledgeItem, TaskView } from '../lib/types'
import { useAction } from '../composables/useAction'
import { useAppStore } from '../stores/app'
import { useSessionStore } from '../stores/sessions'
import ActionButton from './ActionButton.vue'
import BaseModal from './BaseModal.vue'
import CodeEditor from './CodeEditor.vue'
import ConstitutionRules from './ConstitutionRules.vue'
import MarkdownView from './MarkdownView.vue'
import SessionRow from './SessionRow.vue'

const props = defineProps<{ item: KnowledgeItem | null; workspaceId: string }>()
const emit = defineEmits<{ close: []; approved: []; changed: []; launch: [item: KnowledgeItem] }>()
/** Start a workflow run on this task (ADR-0029). */
function runWorkflow() {
  const it = props.item
  emit('close')
  if (it?.id) appRouter.push({ name: 'workflows', query: { start: '1', task: it.id, project: it.projectId ?? undefined } })
}
const app = useAppStore()
const sessions = useSessionStore()

const doc = ref<KnowledgeDoc | null>(null)
const task = ref<TaskView | null>(null)
const structure = ref<ConstitutionStructure | null>(null)
const error = ref<string | null>(null)
const editing = ref(false)
const draftTitle = ref('')
const draftBody = ref('')
/** The document changed on disk while it was being edited (ADR-0049): saving it would be refused as a conflict. */
const changedOnDisk = ref(false)

const isConstitution = computed(() => doc.value?.fields?.format === 'constitution')
const dirty = computed(() => editing.value && !!doc.value && (draftBody.value !== doc.value.body || (!isConstitution.value && draftTitle.value.trim() !== doc.value.title)))

// A reload keeps the error of the action that caused it; only another document starts without one.
async function load(item: KnowledgeItem | null) {
  doc.value = null
  task.value = null
  structure.value = null
  editing.value = false
  changedOnDisk.value = false
  if (!item?.id) return
  try {
    if (item.kind === 'task') task.value = await api.knowledgeTask(props.workspaceId, item.id)
    else doc.value = await api.knowledgeRead(props.workspaceId, item.id)
    if (doc.value?.fields?.format === 'constitution') structure.value = (await api.constitutionGet(props.workspaceId, item.id)).structure
  } catch (e) {
    error.value = toIpcError(e).message
  }
}
watch(
  () => props.item,
  (item) => {
    error.value = null
    return load(item)
  },
  { immediate: true },
)

// Files change outside NEXUS (an editor, `git pull`, Claude); the watcher refreshes the index and `app.freshness` moves.
// Read mode shows the new version in place; an edit keeps its draft and says that saving would be refused.
watch(
  () => app.freshness,
  async () => {
    const item = props.item
    if (!item?.id || busy.value) return
    try {
      if (item.kind === 'task') {
        const t = await api.knowledgeTask(props.workspaceId, item.id)
        if (props.item === item && t.hash !== task.value?.hash) task.value = t
        return
      }
      const d = await api.knowledgeRead(props.workspaceId, item.id)
      if (props.item !== item || !doc.value || d.hash === doc.value.hash) return
      if (editing.value) {
        changedOnDisk.value = true
        return
      }
      if (d.fields?.format === 'constitution') structure.value = (await api.constitutionGet(props.workspaceId, item.id)).structure
      doc.value = d
    } catch {
      /* gone or unreadable: the list shows it, and the next action says why */
    }
  },
)

const hidden = new Set(['nexus', 'id', 'title', 'kind'])
const fields = computed(() =>
  Object.entries(doc.value?.fields ?? {})
    .filter(([k]) => !hidden.has(k))
    .map(([k, v]) => [k, Array.isArray(v) ? v.join(', ') : String(v ?? '')] as const),
)
/** The few facts worth a glance; everything else is under "Details". */
const meta = computed(() => {
  const d = doc.value ?? task.value
  const it = props.item
  if (!it) return []
  const out: string[] = [it.kind ? KIND_LABELS[it.kind] ?? it.kind : 'document']
  if (isConstitution.value) out.push('constitution')
  if (it.applies) out.push(it.applies === 'always' ? 'always on' : 'on demand')
  if (it.privacy === 'local-only') out.push('this computer only')
  const updated = (d && 'updated' in d ? d.updated : null) ?? it.updated
  if (updated) out.push(`updated ${updated.slice(0, 10)}`)
  return out
})
const taskSessions = computed(() => (task.value ? sessions.list.filter((s) => s.taskId === task.value!.id && !s.archivedAt) : []))
const prose = computed(() =>
  (task.value?.description ?? '')
    .split('\n')
    .filter((l) => !/^\s*[-*] \[( |x|X)\] /.test(l))
    .join('\n')
    .trim(),
)

function startEdit() {
  if (!doc.value) return
  draftTitle.value = doc.value.title
  draftBody.value = doc.value.body
  error.value = null
  editing.value = true
}

async function cancelEdit() {
  if (dirty.value && !(await app.confirm({ title: 'Discard changes?', message: 'Your edits to this document are not saved.', confirmLabel: 'Discard', danger: true }))) return
  editing.value = false
  if (changedOnDisk.value) await load(props.item)
}

const saving = useAction(async () => {
  if (!doc.value || !props.item?.id) return
  error.value = null
  try {
    const title = isConstitution.value ? null : draftTitle.value.trim() || null
    await api.knowledgeUpdate(props.workspaceId, props.item.id, draftBody.value, doc.value.hash, title)
    app.toast('success', 'Saved')
    emit('changed')
    await load(props.item)
  } catch (e) {
    const err = toIpcError(e)
    error.value = err.code === 'conflict' ? 'The file changed on disk since you opened it — copy your text, close and open the document again.' : err.message
  }
})

function openInConstitutions() {
  const id = props.item?.id
  emit('close')
  if (id) appRouter.push({ name: 'constitutions', query: { doc: id } })
}

async function close() {
  if (dirty.value && !(await app.confirm({ title: 'Discard changes?', message: 'Your edits to this document are not saved.', confirmLabel: 'Discard', danger: true }))) return
  emit('close')
}

const toggling = useAction(
  async (index: number, done: boolean) => {
    if (!task.value || !props.item?.id) return
    error.value = null
    try {
      await api.knowledgeUpdateTask(props.workspaceId, props.item.id, {
        check: done ? [index] : [],
        uncheck: done ? [] : [index],
        expectedHash: task.value.hash,
      })
      emit('changed')
    } catch (e) {
      const err = toIpcError(e)
      error.value = err.code === 'conflict' ? 'The task file changed on disk — reloaded, try again.' : err.message
    } finally {
      await load(props.item)
    }
  },
  { key: (index: number) => String(index) },
)

const approving = useAction(async () => {
  if (!props.item?.id) return
  error.value = null
  try {
    await api.knowledgeApprove(props.workspaceId, props.item.id)
    emit('approved')
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  }
})

const busy = computed(() => saving.pending.value || toggling.pending.value || approving.pending.value)
</script>

<template>
  <BaseModal :open="!!item" :title="editing ? 'Edit document' : (item?.title ?? '')" :subtitle="doc?.relativePath" :width="820" @close="close">
    <div v-if="meta.length && !editing" class="meta" data-doc-meta>
      <span v-for="m in meta" :key="m" class="meta-chip">{{ m }}</span>
    </div>
    <div v-if="item?.needsApproval && !editing" class="notice notice-warn">
      <TriangleAlert />
      <span>This always-on rule is not given to Claude until you approve this exact version on this machine. Read it first — workspace files can change through sync.</span>
    </div>
    <div v-if="editing && changedOnDisk" class="notice notice-warn" data-doc-changed-on-disk>
      <TriangleAlert />
      <span>This document changed on disk while you were editing it. Saving would be refused — copy your text, then Cancel to see the new version.</span>
    </div>

    <template v-if="task">
      <div class="task-head">
        <span class="badge">{{ STATUS_LABELS[task.status] ?? task.status }}</span>
        <span v-if="task.priority" class="badge">{{ task.priority }}</span>
        <span v-if="task.branch" class="chip mono row"><GitBranch class="ic" /> {{ task.branch }}<template v-if="task.commit"> @ {{ task.commit.slice(0, 10) }}</template></span>
      </div>
      <MarkdownView v-if="prose" class="reading" :source="prose" />
      <div v-if="task.checklist.length" class="checklist" :aria-busy="toggling.pending.value ? 'true' : undefined" data-checklist>
        <div class="label">Checklist <span class="faint">{{ task.checklist.filter((c) => c.done).length }}/{{ task.checklist.length }}</span></div>
        <label v-for="c in task.checklist" :key="c.index" class="checkbox">
          <input type="checkbox" :checked="c.done" :disabled="busy" @change="toggling.run(c.index, ($event.target as HTMLInputElement).checked)" />
          <span>{{ c.text }}</span>
          <LoaderCircle v-if="toggling.isPending(String(c.index))" class="spin ic" aria-hidden="true" />
        </label>
      </div>
      <UsageSummary v-if="item" :task-id="item.id" />
      <div v-if="task.recentLog.length" class="log">
        <div class="label">Progress <span class="faint">last {{ task.recentLog.length }} of {{ task.logEntries }}</span></div>
        <ul>
          <li v-for="(l, n) in task.recentLog" :key="n" class="selectable">{{ l }}</li>
        </ul>
      </div>
      <div v-if="taskSessions.length" class="sessions">
        <div class="label">Sessions for this task</div>
        <SessionRow v-for="s in taskSessions" :key="s.id" :session="s" />
      </div>
    </template>

    <template v-else-if="editing && doc">
      <div class="edit" data-doc-edit>
        <div v-if="!isConstitution" class="field">
          <label for="doc-title">Title</label>
          <input id="doc-title" v-model="draftTitle" class="input" data-doc-edit-title />
        </div>
        <p v-else class="hint">Raw Markdown: each rule is a <code>####</code> heading followed by its <code>&lt;!-- rule: id=… enforcement=… --&gt;</code> marker. The Constitution Manager edits the same document field by field.</p>
        <CodeEditor v-model="draftBody" language="markdown" wrap height="52vh" data-doc-edit-body />
      </div>
    </template>

    <template v-else>
      <div class="reading" data-doc-reading>
        <ConstitutionRules v-if="structure" :structure="structure" />
        <MarkdownView v-else-if="doc && doc.body.trim()" :source="doc.body" />
        <p v-else-if="doc" class="faint">(empty)</p>
      </div>
      <p v-if="doc?.truncated" class="hint">Showing the first 200,000 characters.</p>
      <details v-if="fields.length" class="details">
        <summary>Details</summary>
        <dl class="fields">
          <template v-for="[k, v] in fields" :key="k">
            <dt>{{ k }}</dt>
            <dd class="selectable mono">{{ v }}</dd>
          </template>
        </dl>
      </details>
    </template>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <template v-if="editing">
        <button class="btn btn-ghost" :disabled="busy" @click="cancelEdit"><X /> Cancel</button>
        <span class="spacer" />
        <ActionButton class="btn-primary" :pending="saving.pending.value" :disabled="busy || !dirty" pending-label="Saving…" data-doc-save @click="saving.run()"><template #icon><Save /></template> Save</ActionButton>
      </template>
      <template v-else>
        <button v-if="item" class="btn btn-ghost" @click="revealItemInDir(item.path)"><FolderOpen /> Show file</button>
        <span class="spacer" />
        <button v-if="isConstitution" class="btn" data-doc-open-constitution @click="openInConstitutions"><Scale /> Constitution Manager</button>
        <button v-if="doc && !doc.truncated" class="btn" data-doc-edit-start @click="startEdit"><Pencil /> Edit</button>
        <ActionButton v-if="item?.needsApproval" class="btn-primary" :pending="approving.pending.value" :disabled="busy || !doc" pending-label="Approving…" data-doc-approve @click="approving.run()"><template #icon><ShieldCheck /></template> Approve this version</ActionButton>
        <button v-if="task && task.status !== 'done' && item" class="btn" data-run-workflow @click="runWorkflow"><WorkflowIcon /> Run workflow</button>
        <button v-if="task && task.status !== 'done' && item" class="btn btn-primary" data-launch @click="emit('launch', item)"><Sparkles /> Launch</button>
        <button class="btn" @click="close">Close</button>
      </template>
    </template>
  </BaseModal>
</template>

<style scoped>
.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin: -4px 0 14px;
}
.meta-chip {
  font-size: 11.5px;
  padding: 2px 8px;
  border-radius: 999px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  color: var(--text-muted);
}
.reading {
  max-height: 58vh;
  overflow: auto;
  padding: 2px 4px 2px 0;
  margin: 0 0 12px;
}
.edit {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field label {
  font-size: 13px;
  font-weight: 600;
}
.hint {
  font-size: 12px;
  color: var(--text-faint);
  margin: 0;
}
.details {
  margin: 0 0 12px;
  font-size: 12.5px;
}
.details summary {
  cursor: pointer;
  color: var(--text-faint);
}
.fields {
  display: grid;
  grid-template-columns: 110px 1fr;
  gap: 6px 14px;
  margin: 10px 0 0;
  font-size: 12.5px;
}
dt {
  color: var(--text-muted);
}
dd {
  margin: 0;
  word-break: break-word;
}
.task-head {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 12px;
}
.checklist,
.log,
.sessions {
  margin-bottom: 14px;
}
.checklist .checkbox {
  display: flex;
  margin: 4px 0;
}
.log ul {
  margin: 4px 0 0;
  padding-left: 18px;
  font-size: 13px;
}
.ic {
  width: 13px;
  height: 13px;
}
</style>
