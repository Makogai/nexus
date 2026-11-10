<script setup lang="ts">
// "Needs you" (ADR-0048): what waits for the person; each row opens the place to act.
import { useRouter } from 'vue-router'
import { ArrowUp, BadgeCheck, BookOpenCheck, CircleAlert, ListChecks, MessageSquareWarning, Package, PenLine, TriangleAlert } from '@lucide/vue'
import type { NeedsYouItem } from '../lib/types'
import { NEEDS_YOU_LABELS, isUrgent, openNeedsYou } from '../lib/needsYou'
import { relativeTime } from '../lib/format'

withDefaults(defineProps<{ items: NeedsYouItem[]; showProject?: boolean }>(), { showProject: true })
const router = useRouter()

const ICONS = {
  session: MessageSquareWarning,
  'push-failed': TriangleAlert,
  push: ArrowUp,
  'workflow-review': BadgeCheck,
  'task-review': ListChecks,
  draft: PenLine,
  approval: BookOpenCheck,
  kit: Package,
} as const
</script>

<template>
  <div class="list needs-you" data-needs-you>
    <button v-for="i in items" :key="`${i.kind}:${i.id}`" class="list-row item" :class="{ urgent: isUrgent(i) }" :data-needs-you-item="i.kind" :data-needs-you-id="i.id" @click="openNeedsYou(i, router)">
      <component :is="ICONS[i.kind] ?? CircleAlert" class="ic" />
      <span class="badge" :class="{ 'badge-error': i.kind === 'push-failed', 'badge-warn': i.kind === 'session', 'badge-accent': i.kind === 'task-review' || i.kind === 'push' }">{{ NEEDS_YOU_LABELS[i.kind] }}</span>
      <span class="title truncate">{{ i.title }}</span>
      <span class="detail faint truncate">{{ i.detail }}</span>
      <span v-if="showProject && i.projectName" class="chip">{{ i.projectName }}</span>
      <span v-if="i.since" class="faint when">{{ relativeTime(i.since) }}</span>
    </button>
  </div>
</template>

<style scoped>
.item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  text-align: left;
}
.item.urgent {
  border-left: 3px solid var(--amber);
}
.ic {
  width: 15px;
  height: 15px;
  flex: none;
  color: var(--text-faint);
}
.title {
  font-weight: 550;
  min-width: 0;
  flex: 0 1 auto;
}
.detail {
  min-width: 0;
  flex: 1 1 auto;
}
.when {
  font-size: 12px;
  flex: none;
}
</style>
