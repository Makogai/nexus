<script setup lang="ts">
import { ArrowDown, ArrowUp, GitBranch } from '@lucide/vue'
import type { GitStatus } from '../lib/types'

defineProps<{ status: GitStatus | null; loading?: boolean; error?: string | null; compact?: boolean }>()
</script>

<template>
  <div class="git" :class="{ compact }">
    <span v-if="loading && !status" class="skeleton" style="width: 140px" />
    <span v-else-if="error" class="faint" :title="error">git unavailable</span>
    <template v-else-if="status && status.isRepo">
      <span class="branch row">
        <GitBranch />
        <span class="truncate">{{ status.branch ?? (status.detached ? `detached @ ${status.headOid?.slice(0, 7)}` : '—') }}</span>
      </span>
      <span v-if="status.ahead" class="ab" title="Commits ahead of upstream"><ArrowUp />{{ status.ahead }}</span>
      <span v-if="status.behind" class="ab" title="Commits behind upstream"><ArrowDown />{{ status.behind }}</span>
      <span v-if="status.conflicted" class="badge badge-error">{{ status.conflicted }} conflicts</span>
      <span v-if="status.staged + status.unstaged + status.untracked > 0" class="changes" :title="`${status.staged} staged · ${status.unstaged} modified · ${status.untracked} untracked`">
        {{ status.staged + status.unstaged + status.untracked }} changed
      </span>
      <span v-else class="clean">clean</span>
    </template>
    <span v-else-if="status" class="faint">not a Git repository</span>
  </div>
</template>

<style scoped>
.git {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12.5px;
  color: var(--text-muted);
  min-width: 0;
}
.branch {
  gap: 5px;
  min-width: 0;
  font-family: var(--font-mono);
  font-size: 12px;
  color: var(--text);
}
.branch svg,
.ab svg {
  width: 13px;
  height: 13px;
  flex: none;
}
.ab {
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.changes {
  color: var(--amber);
}
.clean {
  color: var(--green);
}
</style>
