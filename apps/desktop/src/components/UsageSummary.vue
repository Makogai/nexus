<script setup lang="ts">
// Compact usage for one project, task or workflow run (ADR-0030), linking to the Usage dashboard with that filter.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { router } from '../router'
import { listen } from '@tauri-apps/api/event'
import { BarChart3 } from '@lucide/vue'
import { api, USAGE_EVENT } from '../lib/ipc'
import type { UsageFilter, UsageReport } from '../lib/types'
import { compact, costText, grouped, percent, totalTokens } from '../lib/usage'

const props = defineProps<{ projectId?: string | null; taskId?: string | null; workflowRunId?: string | null }>()
const report = ref<UsageReport | null>(null)

const filter = computed<UsageFilter>(() => ({ projectId: props.projectId ?? null, taskId: props.taskId ?? null, workflowRunId: props.workflowRunId ?? null }))
async function load() {
  try {
    report.value = (await api.usageReport(filter.value, 'project')).report
  } catch {
    report.value = null
  }
}
watch(filter, load, { immediate: true })
let unlisten: (() => void) | null = null
onMounted(async () => {
  try {
    unlisten = await listen(USAGE_EVENT, () => void load())
  } catch {
    unlisten = null // outside the desktop shell (tests): no live refresh
  }
})
onBeforeUnmount(() => unlisten?.())

function open() {
  const query: Record<string, string> = { period: 'all' }
  if (props.projectId) query.project = props.projectId
  if (props.taskId) query.task = props.taskId
  if (props.workflowRunId) query.workflow = props.workflowRunId
  void router.push({ path: '/usage', query })
}
const unavailable = computed(() => (report.value ? report.value.coverage.missing + report.value.coverage.unreadable : 0))
</script>

<template>
  <div v-if="report && report.coverage.sessions" class="usage-summary" data-usage-summary>
    <BarChart3 class="icon" />
    <span><strong>{{ compact(totalTokens(report.totals)) }}</strong> tokens <span class="kind">parsed</span></span>
    <span><strong>{{ grouped(report.totals.requests) }}</strong> requests</span>
    <span v-if="report.efficiency.cacheHit != null"><strong>{{ percent(report.efficiency.cacheHit) }}</strong> cache hit</span>
    <span>{{ costText(report.cost) }} <span class="kind">estimate</span></span>
    <span class="faint">{{ report.coverage.measured }} of {{ report.coverage.sessions }} sessions with usage<template v-if="unavailable"> · {{ unavailable }} unavailable</template></span>
    <button class="btn btn-sm btn-ghost" data-usage-open @click="open">Usage details</button>
  </div>
</template>

<style scoped>
.usage-summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px 14px;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--bg-panel);
  font-size: 12px;
  color: var(--text-muted);
}
.usage-summary strong {
  color: var(--text);
  font-variant-numeric: tabular-nums;
}
.icon {
  width: 14px;
  height: 14px;
  color: var(--series-1);
}
.kind {
  font-size: 11px;
  color: var(--text-faint);
}
.usage-summary .btn {
  margin-left: auto;
}
</style>
