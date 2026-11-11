<script setup lang="ts">
// Dev Environment (ADR-0051): the development tools on this computer next to what the active workspace's projects need.
// NEXUS runs only the tools' version commands; install commands and version-manager hints are typed into a new terminal
// for the person to read and run (`typeIntoTerminal`) — never run by NEXUS.
import { computed, onActivated, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { openUrl } from '@tauri-apps/plugin-opener'
import { CircleCheck, CircleAlert, CircleX, ExternalLink, RefreshCw, Search, SquareTerminal, TriangleAlert, Wrench, X } from '@lucide/vue'
import ActionButton from '../components/ActionButton.vue'
import { api } from '../lib/ipc'
import { typeIntoTerminal } from '../lib/terminal'
import { useAction } from '../composables/useAction'
import type { DevEnvironment, DevTool, ToolCategory, ToolNeed } from '../lib/types'

const data = ref<DevEnvironment | null>(null)
type Filter = 'all' | 'needed' | 'missing' | 'installed'
const filter = ref<Filter>('all')
const query = ref('')

const CATEGORY: Record<ToolCategory, string> = {
  'source-control': 'Source control',
  runtime: 'Runtimes and SDKs',
  'package-manager': 'Package managers',
  'version-manager': 'Version managers',
  build: 'Build tools',
  infrastructure: 'Containers and infrastructure',
  cloud: 'Cloud',
  database: 'Database clients',
  'ai-agent': 'AI agents',
  integration: 'Integrations',
}

const load = useAction(async (refresh: boolean) => {
  data.value = await api.devEnvironment(refresh)
  return data.value
})
const check = useAction(
  async (id: string) => {
    data.value = await api.devEnvironmentCheck(id)
    return data.value.tools.find((t) => t.id === id) ?? null
  },
  { key: (id) => id, failure: 'Could not check the tool', success: (t) => (t ? (t.path ? `${t.name} ${t.version ?? ''} found`.trim() : `${t.name} is still not on PATH`) : null) },
)

onMounted(() => load.run(false))
// Project needs can change while the page is kept alive; the inventory itself is only probed on request.
onActivated(() => {
  if (data.value) load.run(false)
})

const unmet = (t: DevTool) => t.neededBy.some((n) => n.status === 'missing' || n.status === 'mismatch')
const counts = computed(() => {
  const tools = data.value?.tools ?? []
  return { installed: tools.filter((t) => t.path).length, missing: tools.filter((t) => !t.path).length, unmet: tools.filter(unmet).length, needed: tools.filter((t) => t.neededBy.length).length }
})

const shown = computed(() => {
  const q = query.value.trim().toLowerCase()
  return (data.value?.tools ?? []).filter((t) => {
    if (filter.value === 'needed' && !t.neededBy.length) return false
    if (filter.value === 'missing' && t.path && !unmet(t)) return false
    if (filter.value === 'installed' && !t.path) return false
    return !q || t.name.toLowerCase().includes(q) || t.id.includes(q)
  })
})

const groups = computed(() => {
  const out: { category: ToolCategory; label: string; tools: DevTool[] }[] = []
  for (const t of shown.value) {
    let g = out.find((x) => x.category === t.category)
    if (!g) out.push((g = { category: t.category, label: CATEGORY[t.category] ?? t.category, tools: [] }))
    g.tools.push(t)
  }
  return out
})

const probedAt = computed(() => (data.value?.probedAt ? new Date(data.value.probedAt).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : null))

function needLabel(n: ToolNeed) {
  return n.constraint ? n.constraint : 'any version'
}
const STATUS_TEXT: Record<ToolNeed['status'], string> = { ok: 'fits', mismatch: 'other version', missing: 'missing', unknown: 'check by hand' }

// Opening the terminal is a backend call (a new shell session): pending per command, failures stay visible.
const typing = useAction<[string[], string?], unknown>((command, projectId) => typeIntoTerminal(command.join(' '), { projectId: projectId ?? null }), {
  key: (command) => command.join(' '),
  failure: 'Could not open a terminal',
})
</script>

<template>
  <div class="page" data-devenv-view>
    <div class="page-header">
      <div>
        <h1>Dev Environment</h1>
        <p>
          The development tools on this computer and what your projects need. NEXUS only asks each tool for its version; install commands are typed into a terminal for you to
          run.
        </p>
      </div>
      <div class="page-actions">
        <ActionButton class="btn-ghost" data-devenv-refresh :pending="load.pending.value" pending-label="Checking tools…" @click="load.run(true)">
          <template #icon><RefreshCw /></template>
          Check all again
        </ActionButton>
      </div>
    </div>

    <p v-if="load.error.value" class="notice notice-error" data-devenv-error>
      <TriangleAlert /> <span>Could not read the tools: {{ load.error.value }}</span>
      <button class="btn btn-sm btn-ghost" aria-label="Dismiss" @click="load.clearError()"><X /></button>
    </p>
    <p v-if="check.error.value" class="notice notice-error">
      <TriangleAlert /> <span>{{ check.error.value }}</span>
      <button class="btn btn-sm btn-ghost" aria-label="Dismiss" @click="check.clearError()"><X /></button>
    </p>

    <p v-if="!data && load.pending.value" class="faint loading" data-devenv-loading>Checking the tools on this computer… (each one is asked for its version; this takes a few seconds)</p>

    <template v-if="data">
      <div class="toolbar">
        <div class="segments" role="radiogroup" aria-label="Show">
          <button
            v-for="f in ['all', 'needed', 'missing', 'installed'] as Filter[]"
            :key="f"
            class="segment"
            role="radio"
            :data-devenv-filter="f"
            :aria-checked="filter === f"
            :class="{ selected: filter === f }"
            @click="filter = f"
          >
            {{ { all: 'All', needed: `Needed (${counts.needed})`, missing: `Missing (${counts.missing + counts.unmet})`, installed: `Installed (${counts.installed})` }[f] }}
          </button>
        </div>
        <label class="search"><Search /><input v-model="query" type="search" placeholder="Find a tool" aria-label="Find a tool" data-devenv-search /></label>
        <span class="faint small" data-devenv-summary>
          {{ counts.installed }} installed · {{ counts.missing }} not installed<template v-if="counts.unmet"> · <strong class="warn-text">{{ counts.unmet }} not as a project needs</strong></template>
          <template v-if="probedAt"> · checked {{ probedAt }}</template>
        </span>
      </div>

      <p v-if="!data.workspaceId" class="notice explain"><Wrench /> <span>No active workspace: project needs appear here once a workspace with projects is open.</span></p>
      <p v-else-if="data.projectsWithoutEnvironment.length" class="notice explain">
        <Wrench /> <span>Needs unknown for {{ data.projectsWithoutEnvironment.join(', ') }} — not on this computer and no environment record in the workspace yet.</span>
      </p>

      <p v-if="!groups.length" class="faint" data-devenv-empty>Nothing matches.</p>

      <section v-for="g in groups" :key="g.category" class="card" :data-devenv-category="g.category">
        <h2 class="card-title">{{ g.label }} <span class="faint">({{ g.tools.length }})</span></h2>
        <div class="tools">
          <div v-for="t in g.tools" :key="t.id" class="tool" :class="{ missing: !t.path, unmet: unmet(t) }" :data-devenv-tool="t.id" :data-installed="t.path ? 'yes' : 'no'">
            <div class="head">
              <CircleCheck v-if="t.path && !unmet(t)" class="ok-ic" aria-hidden="true" />
              <CircleAlert v-else-if="t.path" class="warn-ic" aria-hidden="true" />
              <CircleX v-else class="missing-ic" aria-hidden="true" />
              <span class="name">{{ t.name }}</span>
              <span v-if="t.path" class="badge" data-devenv-version>{{ t.version ?? 'version not reported' }}</span>
              <span v-else class="badge" data-devenv-version>not installed</span>
              <span class="grow" />
              <ActionButton
                v-if="!t.path && t.installCommand"
                class="btn-sm"
                :data-devenv-install="t.id"
                :title="t.installCommand.join(' ')"
                :pending="typing.isPending(t.installCommand.join(' '))"
                @click="typing.run(t.installCommand)"
              >
                <template #icon><SquareTerminal /></template>
                Type install command
              </ActionButton>
              <button class="btn btn-sm btn-ghost" :data-devenv-page="t.id" @click="openUrl(t.installUrl)">Install page <ExternalLink class="ext" /></button>
              <ActionButton class="btn-sm btn-ghost" :data-devenv-check="t.id" :pending="check.isPending(t.id)" :disabled="check.pending.value && !check.isPending(t.id)" pending-label="Checking…" @click="check.run(t.id)">
                Check again
              </ActionButton>
            </div>
            <div v-if="t.path" class="path mono" :title="t.path">{{ t.path }}</div>
            <div v-if="t.note" class="note faint small" data-devenv-note>{{ t.note }}</div>
            <details v-if="t.path && t.copies.length > 1" class="copies" :data-devenv-copies="t.copies.length">
              <summary>{{ t.copies.length }} copies on PATH — the first one runs</summary>
              <div v-for="c in t.copies" :key="c" class="mono small">{{ c }}</div>
            </details>
            <ul v-if="t.neededBy.length" class="needs">
              <li v-for="n in t.neededBy" :key="n.projectId + n.source" :data-devenv-need="n.status">
                <RouterLink :to="`/projects/${n.projectId}?tab=settings`" class="proj">{{ n.project }}</RouterLink>
                needs <strong>{{ needLabel(n) }}</strong> <span class="faint">({{ n.source }})</span>
                <span class="status" :class="n.status">{{ STATUS_TEXT[n.status] }}</span>
                <ActionButton v-if="n.hint" class="btn-sm btn-ghost" :data-devenv-hint="n.hint.manager" :title="n.hint.command.join(' ')" :pending="typing.isPending(n.hint.command.join(' '))" @click="typing.run(n.hint.command, n.projectId)">
                  <template #icon><SquareTerminal /></template>
                  Type {{ n.hint.command.join(' ') }}
                </ActionButton>
              </li>
            </ul>
          </div>
        </div>
      </section>

      <p class="notice explain">
        <Wrench />
        <span>
          After installing, use <em>Check again</em> on the tool. A terminal opened by NEXUS keeps the PATH NEXUS started with; restart NEXUS if a newly installed tool is still not
          found.
        </span>
      </p>
    </template>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  margin-bottom: 14px;
}
.segments {
  display: inline-grid;
  grid-auto-flow: column;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
}
.segment {
  height: 28px;
  padding: 0 12px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  font-weight: 600;
  cursor: pointer;
}
.segment:hover {
  color: var(--text);
}
.segment.selected {
  background: var(--accent-soft);
  color: var(--accent-strong);
  box-shadow: inset 0 0 0 1px var(--accent-ring);
}
.search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  height: 34px;
  border-radius: 8px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
}
.search svg {
  width: 15px;
  height: 15px;
  color: var(--text-muted);
}
.search input {
  border: none;
  background: transparent;
  color: var(--text);
  outline: none;
  width: 180px;
}
.small {
  font-size: 12px;
}
.loading {
  margin-top: 20px;
}
.tools {
  display: grid;
  gap: 10px;
}
.tool {
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid var(--border);
}
.tool.unmet {
  border-color: var(--amber);
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.head svg {
  width: 16px;
  height: 16px;
}
.ok-ic {
  color: var(--green);
}
.warn-ic {
  color: var(--amber);
}
.missing-ic {
  color: var(--text-faint);
}
.name {
  font-weight: 600;
}
.grow {
  flex: 1;
}
.path {
  margin: 4px 0 0 24px;
  font-size: 12px;
  color: var(--text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.note {
  margin: 4px 0 0 24px;
}
.copies {
  margin: 4px 0 0 24px;
  font-size: 12px;
  color: var(--text-muted);
}
.copies summary {
  cursor: pointer;
}
.needs {
  list-style: none;
  margin: 8px 0 0 24px;
  padding: 0;
  display: grid;
  gap: 4px;
  font-size: 13px;
}
.needs li {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.status {
  font-size: 11px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 6px;
  background: var(--bg-input);
}
.status.ok {
  color: var(--green);
}
.status.mismatch,
.status.missing {
  color: var(--amber);
}
.warn-text {
  color: var(--amber);
}
.ext {
  width: 12px;
  height: 12px;
}
</style>
