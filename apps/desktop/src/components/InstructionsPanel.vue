<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { Layers, TriangleAlert } from '@lucide/vue'
import { api, toIpcError } from '../lib/ipc'
import { shortPath } from '../lib/format'
import type { InstructionLayer, InstructionsPreview } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'

// Effective-instructions inspector (ADR-0019): what Claude Code loads by itself for a profile and project, in its own
// order, and what NEXUS appends. Read-only — NEXUS never edits or reorders these files.
const props = defineProps<{ projectId: string }>()
const app = useAppStore()
const profiles = useProfileStore()

const profileId = ref('')
const preview = ref<InstructionsPreview | null>(null)
const error = ref<string | null>(null)
const loading = ref(false)

watch(
  () => profiles.list,
  (list) => {
    if (!profileId.value || !list.some((p) => p.id === profileId.value)) {
      profileId.value = app.prefs.lastLaunch?.profileId && list.some((p) => p.id === app.prefs.lastLaunch?.profileId) ? app.prefs.lastLaunch.profileId : (list[0]?.id ?? '')
    }
  },
  { immediate: true },
)

async function load() {
  if (!profileId.value) return
  loading.value = true
  try {
    preview.value = await api.instructionsInspect({ profileId: profileId.value, projectId: props.projectId })
    error.value = null
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    loading.value = false
  }
}
watch([profileId, () => props.projectId], load, { immediate: true })
defineExpose({ load })

const LOAD: Record<InstructionLayer['load'], string> = {
  'at-start': 'at start',
  'on-demand': 'on demand',
  excluded: 'excluded',
  'not-loaded': 'not loaded',
  fallback: 'fallback',
  'needs-approval': 'needs approval',
}
const KIND: Record<InstructionLayer['kind'], string> = {
  'claude-md': 'memory',
  rule: 'rule',
  import: 'import',
  'agents-md': 'AGENTS.md',
  'managed-setting': 'policy',
}
const rules = computed(() => preview.value?.nexus.rules ?? null)
</script>

<template>
  <section class="card instructions" data-instructions>
    <div class="head">
      <h2 class="card-title"><Layers /> Effective instructions</h2>
      <select v-model="profileId" class="select profile" aria-label="Claude profile">
        <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>
    <p class="muted intro">Loaded by Claude itself, in Claude's order (later entries are more specific). NEXUS only reads these files.</p>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template v-if="preview">
      <div v-for="w in preview.claude.warnings" :key="w" class="notice notice-warn warn"><TriangleAlert /> {{ w }}</div>
      <ol v-if="preview.claude.layers.length" class="layers">
        <li v-for="(l, n) in preview.claude.layers" :key="n" class="layer" :class="l.load" :data-load="l.load">
          <span class="badge scope">{{ l.scope }}</span>
          <span class="kind">{{ KIND[l.kind] }}</span>
          <span class="file mono selectable" :title="l.path ?? ''">{{ l.path ? shortPath(l.path, app.info?.homeDir, 70) : 'managed settings' }}</span>
          <span class="badge" :class="{ 'badge-ok': l.load === 'at-start', 'badge-warn': l.load === 'excluded' || l.load === 'fallback' }">{{ LOAD[l.load] }}</span>
          <span v-if="l.lines" class="faint size">{{ l.lines }} lines</span>
          <div v-if="l.reason || l.paths.length" class="reason faint">{{ l.reason ?? `when Claude touches ${l.paths.join(', ')}` }}</div>
        </li>
      </ol>
      <p v-else class="faint">No CLAUDE.md, rules or imports apply to this folder.</p>
      <p class="faint totals">{{ preview.claude.filesAtStart }} file(s), {{ preview.claude.charsAtStart.toLocaleString() }} characters at session start · AGENTS.md mode: {{ preview.claude.agentsMode }}</p>

      <h3 class="sub">NEXUS appends <span class="faint">({{ preview.nexus.appendedChars.toLocaleString() }} characters)</span></h3>
      <ul class="nexus">
        <li>Work profile: {{ preview.nexus.workProfile ?? 'none' }}</li>
        <li>Always-on rules: {{ rules?.included.length ? rules.included.join(', ') : 'none' }}</li>
        <li v-if="rules?.alreadyLoaded?.length" data-already>Skipped — already in Claude's files: {{ rules.alreadyLoaded.join(', ') }}</li>
        <li v-if="rules?.pendingApproval.length" class="warn-text">Waiting for approval: {{ rules.pendingApproval.join(', ') }}</li>
        <li v-if="rules?.overBudget.length">Over budget (Claude reads them on demand): {{ rules.overBudget.join(', ') }}</li>
        <li v-if="preview.nexus.isolationExcludes.length" data-isolation-on>Profile isolation: the default profile's personal instructions are excluded ({{ preview.nexus.isolationExcludes.length }} pattern(s))</li>
        <li v-if="preview.nexus.mcp">NEXUS tools: {{ preview.nexus.mcp }}</li>
        <li v-if="preview.nexus.hooks">Lifecycle hooks: {{ preview.nexus.hooks }}</li>
        <li class="faint">A task session gets only a pointer in the system prompt; the task text arrives as labelled data, never its history. Everything else is retrieved on demand.</li>
      </ul>
    </template>
    <p v-else-if="loading" class="faint">Inspecting…</p>
  </section>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.profile {
  width: auto;
  min-width: 160px;
  height: 30px;
}
.intro {
  margin: 4px 0 10px;
  font-size: 13px;
}
.layers {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.layer {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 8px;
  font-size: 13px;
}
.layer.excluded,
.layer.not-loaded {
  opacity: 0.65;
}
.kind {
  color: var(--text-muted);
  min-width: 60px;
}
.file {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.reason {
  flex-basis: 100%;
  font-size: 12px;
}
.totals {
  margin: 8px 0 0;
  font-size: 12px;
}
.sub {
  font-size: 14px;
  margin: 16px 0 6px;
}
.nexus {
  margin: 0;
  padding-left: 18px;
  font-size: 13px;
}
.warn {
  margin-bottom: 8px;
}
.warn-text {
  color: var(--warn, #d9a441);
}
</style>
