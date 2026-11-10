<script setup lang="ts">
// Skills (ADR-0046): every skill a NEXUS session can get, in one place — NEXUS's built-in skills, the workspace's own
// skills (synced, every project), each project's native Claude skills (in its repository) and AI Kit skills. The
// person writes and edits workspace and project skills here; built-in and kit skills are shown as Claude reads them.
import { computed, onActivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { FolderGit2, LoaderCircle, Package, Plus, RefreshCw, Save, ShieldCheck, Sparkles, Trash2, TriangleAlert, Undo2 } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import type { SkillEntry, SkillFile, SkillScope } from '../lib/types'
import { useAction } from '../composables/useAction'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'
import ActionButton from '../components/ActionButton.vue'
import BaseModal from '../components/BaseModal.vue'
import CodeEditor from '../components/CodeEditor.vue'

const app = useAppStore()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const router = useRouter()

const skills = ref<SkillEntry[]>([])
const error = ref<string | null>(null)
const filter = ref<'all' | SkillScope>('all')
const query = ref('')
const selectedKey = ref<string | null>(null)
const file = ref<SkillFile | null>(null)
const text = ref('')

const keyOf = (s: SkillEntry) => `${s.scope}:${s.projectId ?? s.kitId ?? ''}:${s.name}`
const dirty = computed(() => !!file.value?.entry.editable && text.value !== file.value.text)

const SCOPES: { id: 'all' | SkillScope; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'workspace', label: 'Workspace' },
  { id: 'project', label: 'Projects' },
  { id: 'kit', label: 'AI Kits' },
  { id: 'builtin', label: 'NEXUS' },
]
const GROUPS: Record<SkillScope, string> = {
  workspace: 'Workspace — every project, synced with your workspace',
  project: 'Projects — in each project’s repository',
  kit: 'AI Kits',
  builtin: 'Built into NEXUS',
}
const STATUS: Record<string, { label: string; tone: string }> = {
  'built-in': { label: 'built in', tone: '' },
  approved: { label: 'active', tone: 'badge-ok' },
  native: { label: 'in repository', tone: 'badge-accent' },
  'needs-review': { label: 'needs review', tone: 'badge-warn' },
  changed: { label: 'changed — review', tone: 'badge-warn' },
  'not-registered': { label: 'not reviewed', tone: 'badge-warn' },
  missing: { label: 'missing', tone: 'badge-error' },
}

const visible = computed(() => {
  const q = query.value.trim().toLowerCase()
  return skills.value.filter(
    (s) => (filter.value === 'all' || s.scope === filter.value) && (!q || `${s.invokeAs} ${s.description} ${s.projectName ?? ''} ${s.kitName ?? ''}`.toLowerCase().includes(q)),
  )
})
const groups = computed(() =>
  (['workspace', 'project', 'kit', 'builtin'] as SkillScope[]).map((scope) => ({ scope, title: GROUPS[scope], items: visible.value.filter((s) => s.scope === scope) })).filter((g) => g.items.length),
)
const selected = computed(() => skills.value.find((s) => keyOf(s) === selectedKey.value) ?? null)
const counts = computed(() => Object.fromEntries(SCOPES.map((s) => [s.id, s.id === 'all' ? skills.value.length : skills.value.filter((x) => x.scope === s.id).length])))

async function load() {
  const ws = workspaces.active
  if (!ws) return
  try {
    skills.value = await api.skillsList(ws.id)
    error.value = null
    if (!projects.list.length) await projects.load(ws.id).catch(() => undefined)
    if (!selected.value) {
      const first = skills.value.find((s) => s.editable) ?? skills.value[0]
      if (first) await open(first)
    }
  } catch (e) {
    error.value = toIpcError(e).message
  }
}
const refreshing = useAction(load)
onMounted(load)
onActivated(() => {
  if (!dirty.value) void load()
})
watch(() => workspaces.active?.id, () => {
  selectedKey.value = null
  file.value = null
  void load()
})

async function open(s: SkillEntry) {
  const ws = workspaces.active
  if (!ws) return
  if (dirty.value && keyOf(s) !== selectedKey.value) {
    const ok = await app.confirm({ title: 'Discard your changes?', message: `Your edits to ${file.value?.entry.invokeAs} are not saved.`, confirmLabel: 'Discard' })
    if (!ok) return
  }
  try {
    const f = await api.skillsRead(ws.id, { scope: s.scope, name: s.name, projectId: s.projectId, kitId: s.kitId })
    selectedKey.value = keyOf(s)
    file.value = f
    text.value = f.text
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}
const opening = useAction(open, { key: keyOf })

function editableScope(s: SkillEntry): 'workspace' | 'project' {
  return s.scope === 'project' ? 'project' : 'workspace'
}

const saving = useAction(async () => {
  const ws = workspaces.active
  const f = file.value
  if (!ws || !f || !dirty.value) return
  try {
    const r = await api.skillsSave(ws.id, editableScope(f.entry), f.entry.projectId, f.entry.name, text.value, f.hash)
    file.value = r.file
    text.value = r.file.text
    app.toast(r.outcome === 'needs-review' ? 'info' : 'success', r.message, 7000)
    skills.value = await api.skillsList(ws.id)
  } catch (e) {
    const err = toIpcError(e)
    app.toast('error', err.code === 'conflict' ? 'The skill changed on disk since you opened it — copy your text, open it again and apply your change.' : `Not saved: ${err.message}`)
  }
})

function revert() {
  if (file.value) text.value = file.value.text
}

const deleting = useAction(async () => {
  const ws = workspaces.active
  const f = file.value
  if (!ws || !f?.entry.editable) return
  const where = f.entry.scope === 'project' ? `${f.entry.projectName}’s repository (commit the removal there)` : 'the workspace (it travels with the next sync)'
  const ok = await app.confirm({ title: `Delete ${f.entry.invokeAs}?`, message: `The skill’s folder is removed from ${where}.`, confirmLabel: 'Delete skill', danger: true })
  if (!ok) return
  try {
    await api.skillsDelete(ws.id, editableScope(f.entry), f.entry.projectId, f.entry.name, f.hash)
    app.toast('success', `Deleted ${f.entry.invokeAs}`)
    selectedKey.value = null
    file.value = null
    text.value = ''
    await load()
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
})

function onKey(ev: KeyboardEvent) {
  if ((ev.ctrlKey || ev.metaKey) && ev.key.toLowerCase() === 's') {
    ev.preventDefault()
    void saving.run()
  }
}

// ---- New skill ----
const creating = ref(false)
const newScope = ref<'workspace' | 'project'>('workspace')
const newProject = ref<string | null>(null)
const newName = ref('')
const newDescription = ref('')
const availableProjects = computed(() => projects.list.filter((p) => p.available))
const nameOk = computed(() => /^[a-z0-9]([a-z0-9-]{0,62}[a-z0-9])?$/.test(newName.value) && !/anthropic|claude/.test(newName.value))

function startCreate(scope: 'workspace' | 'project' = 'workspace') {
  newScope.value = scope
  newProject.value = availableProjects.value[0]?.id ?? null
  newName.value = ''
  newDescription.value = ''
  createSkill.clearError()
  creating.value = true
}

function tidyName() {
  newName.value = newName.value.toLowerCase().replace(/[\s_]+/g, '-').replace(/[^a-z0-9-]/g, '')
}

// A failed create keeps the dialog open with the reason until the next attempt.
const createSkill = useAction(async () => {
  const ws = workspaces.active
  if (!ws || !nameOk.value || !newDescription.value.trim()) return
  const r = await api.skillsCreate(ws.id, newScope.value, newScope.value === 'project' ? newProject.value : null, newName.value, newDescription.value)
  creating.value = false
  app.toast(r.outcome === 'needs-review' ? 'info' : 'success', r.message, 7000)
  skills.value = await api.skillsList(ws.id)
  filter.value = 'all'
  await open(r.file.entry)
})
const busy = computed(() => saving.pending.value || deleting.pending.value || createSkill.pending.value)
</script>

<template>
  <div class="page" @keydown="onKey">
    <div class="page-header">
      <div>
        <h1>Skills</h1>
        <p>Every skill a NEXUS session can get. Write your own for the whole workspace (synced, every project) or for one project (in its repository) — Claude loads a skill when its description matches the work.</p>
      </div>
      <div class="page-actions">
        <ActionButton class="btn-ghost" :pending="refreshing.pending.value" data-skill-refresh @click="refreshing.run()"><template #icon><RefreshCw /></template> Refresh</ActionButton>
        <button class="btn btn-primary" data-skill-new @click="startCreate()"><Plus /> New skill</button>
      </div>
    </div>
    <div v-if="error" class="notice notice-error selectable">{{ error }}</div>

    <div class="toolbar">
      <div class="segments" role="radiogroup" aria-label="Show">
        <button v-for="s in SCOPES" :key="s.id" class="segment" role="radio" :aria-checked="filter === s.id" :class="{ selected: filter === s.id }" :data-skill-filter="s.id" @click="filter = s.id">
          {{ s.label }} <span class="faint">{{ counts[s.id] }}</span>
        </button>
      </div>
      <input v-model="query" class="input search" placeholder="Search skills" aria-label="Search skills" />
    </div>

    <div class="layout">
      <nav class="skill-list" aria-label="Skills" data-skill-list>
        <section v-for="g in groups" :key="g.scope" class="group">
          <h3 class="group-title">{{ g.title }}</h3>
          <button
            v-for="s in g.items"
            :key="keyOf(s)"
            class="skill-item"
            :class="{ active: keyOf(s) === selectedKey }"
            :disabled="opening.isPending(keyOf(s))"
            :aria-busy="opening.isPending(keyOf(s)) ? 'true' : undefined"
            :data-skill="s.invokeAs"
            :data-skill-project="s.projectName ?? undefined"
            @click="opening.run(s)"
          >
            <span class="row1"><code class="name">{{ s.invokeAs }}</code><LoaderCircle v-if="opening.isPending(keyOf(s))" class="spin pending-ic" aria-hidden="true" /><span class="badge" :class="STATUS[s.status]?.tone">{{ STATUS[s.status]?.label ?? s.status }}</span></span>
            <span v-if="s.projectName || s.kitName" class="faint small">{{ s.projectName ?? s.kitName }}</span>
            <span class="desc small">{{ s.description || 'No description — Claude cannot tell when to use it.' }}</span>
          </button>
        </section>
        <div v-if="!groups.length" class="empty small">
          <Sparkles class="empty-icon" />
          <p>{{ skills.length ? 'No skill matches.' : 'No skills yet.' }}</p>
          <button class="btn btn-sm" @click="startCreate()"><Plus /> New skill</button>
        </div>
      </nav>

      <section v-if="file && selected" class="detail" data-skill-detail>
        <header class="detail-head">
          <div class="min0">
            <h2><code>{{ file.entry.invokeAs }}</code> <span class="badge" :class="STATUS[file.entry.status]?.tone">{{ STATUS[file.entry.status]?.label ?? file.entry.status }}</span></h2>
            <p class="small">Reaches {{ file.entry.reaches }}.</p>
            <p v-if="file.entry.path" class="faint mono small selectable path">{{ file.entry.path }}</p>
          </div>
          <div class="actions">
            <template v-if="file.entry.editable">
              <button class="btn btn-sm" :disabled="!dirty || busy" @click="revert"><Undo2 /> Revert</button>
              <ActionButton class="btn-sm btn-primary" :pending="saving.pending.value" :disabled="!dirty || busy" pending-label="Saving…" data-skill-save @click="saving.run()"><template #icon><Save /></template> Save</ActionButton>
              <ActionButton class="btn-sm btn-ghost" :pending="deleting.pending.value" :disabled="busy" :aria-label="`Delete ${file.entry.invokeAs}`" title="Delete skill" data-skill-delete @click="deleting.run()"><template #icon><Trash2 /></template></ActionButton>
            </template>
          </div>
        </header>

        <div v-if="file.entry.scope === 'workspace' && file.entry.status !== 'approved'" class="notice notice-warn review" data-skill-review>
          <TriangleAlert />
          <span>Sessions do not get the workspace skills until this computer approves them — changes arrived that you have not reviewed (from a sync or from Claude).</span>
          <button class="btn btn-sm" @click="router.push('/kits')"><ShieldCheck /> Review in AI Kits</button>
        </div>
        <p v-else-if="file.entry.scope === 'project'" class="notice small"><FolderGit2 /> Lives in {{ file.entry.projectName }}’s repository: Claude Code loads it in every Claude session there, also outside NEXUS. Commit it with the project.</p>
        <p v-else-if="file.entry.scope === 'kit'" class="notice small"><Package /> From the AI Kit {{ file.entry.kitName }} — edit it at its source; review and choose profiles in <RouterLink to="/kits">AI Kits</RouterLink>.</p>
        <p v-else-if="file.entry.scope === 'builtin'" class="notice small"><Sparkles /> Part of NEXUS, exactly as Claude reads it — not editable.</p>

        <CodeEditor v-model="text" filename="SKILL.md" :readonly="!file.entry.editable" height="calc(100vh - 400px)" data-skill-editor />
        <p v-if="file.entry.editable" class="hint">The frontmatter's <code>description</code> decides when Claude loads the skill — say what it does and when to use it. Ctrl+S saves.</p>
      </section>
    </div>

    <BaseModal :open="creating" title="New skill" subtitle="A SKILL.md from a short template — you write the steps next." :width="560" @close="creating = false">
      <div class="segments" role="radiogroup" aria-label="Where">
        <button class="segment" role="radio" :aria-checked="newScope === 'workspace'" :class="{ selected: newScope === 'workspace' }" data-new-scope="workspace" @click="newScope = 'workspace'">Workspace</button>
        <button class="segment" role="radio" :aria-checked="newScope === 'project'" :class="{ selected: newScope === 'project' }" :disabled="!availableProjects.length" data-new-scope="project" @click="newScope = 'project'">One project</button>
      </div>
      <p class="hint">{{ newScope === 'workspace' ? 'Every session of this workspace gets it as workspace:<name>; it travels with your workspace repository.' : 'Saved in the project’s .claude/skills — Claude Code loads it there; you commit it with the project.' }}</p>
      <div v-if="newScope === 'project'" class="field">
        <label for="skill-project">Project</label>
        <select id="skill-project" v-model="newProject" class="input" data-new-project>
          <option v-for="p in availableProjects" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </div>
      <div class="field">
        <label for="skill-name">Name</label>
        <input id="skill-name" v-model="newName" class="input mono" placeholder="release-notes" maxlength="64" data-new-name @input="tidyName" />
        <span class="hint" :class="{ bad: newName && !nameOk }">Lowercase letters, digits and hyphens.{{ newName && !nameOk ? ' Not valid yet.' : '' }}</span>
      </div>
      <div class="field">
        <label for="skill-description">Description</label>
        <textarea id="skill-description" v-model="newDescription" class="input" rows="3" maxlength="1024" data-new-description placeholder="Draft release notes from merged changes. Use when preparing a release." />
        <span class="hint">What it does and when Claude should use it — Claude reads only this line to decide whether to load the skill.</span>
      </div>
      <div v-if="createSkill.error.value" class="notice notice-error selectable" data-new-error><TriangleAlert /> {{ createSkill.error.value }}</div>
      <template #footer>
        <button class="btn" @click="creating = false">Cancel</button>
        <ActionButton
          class="btn-primary"
          :pending="createSkill.pending.value"
          :disabled="!nameOk || !newDescription.trim() || busy || (newScope === 'project' && !newProject)"
          pending-label="Creating…"
          data-new-create
          @click="createSkill.run()"
        >
          Create skill
        </ActionButton>
      </template>
    </BaseModal>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
}
.search {
  max-width: 260px;
}
.layout {
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}
.skill-list {
  display: flex;
  flex-direction: column;
  gap: 14px;
  max-height: calc(100vh - 230px);
  overflow: auto;
  padding-right: 4px;
}
.group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.group-title {
  margin: 0;
  font-size: 11.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--text-faint);
}
.skill-item {
  text-align: left;
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 9px 10px;
  background: var(--bg-raised);
  color: var(--text);
  cursor: pointer;
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.skill-item.active {
  border-color: var(--accent-ring);
  background: var(--accent-soft);
}
.row1 {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 0;
}
.name {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.desc {
  color: var(--text-muted);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.detail {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}
.detail-head {
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.detail-head h2 {
  margin: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 17px;
}
.detail-head p {
  margin: 4px 0 0;
}
.min0 {
  min-width: 0;
}
.path {
  overflow-wrap: anywhere;
}
.actions {
  display: flex;
  gap: 6px;
  align-items: flex-start;
  flex-shrink: 0;
}
.review {
  align-items: center;
  gap: 10px;
}
.notice svg {
  width: 14px;
  height: 14px;
  vertical-align: -2px;
  margin-right: 4px;
}
.pending-ic {
  width: 13px;
  height: 13px;
  flex: none;
  margin-left: auto;
}
.hint.bad {
  color: var(--danger, #e5484d);
}
.empty {
  text-align: center;
  padding: 20px 8px;
}
@media (max-width: 1000px) {
  .layout {
    grid-template-columns: 1fr;
  }
  .skill-list {
    max-height: none;
  }
}
</style>
