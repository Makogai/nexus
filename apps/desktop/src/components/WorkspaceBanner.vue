<script setup lang="ts">
// Workspace schema state (ADR-0036): a read-only workspace from a newer NEXUS says why; an older schema offers an
// explained, backed-up migration that changes nothing until the person confirms the plan.
import { computed, ref } from 'vue'
import { ArrowUpCircle, Lock } from '@lucide/vue'
import BaseModal from './BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import type { MigrationPlan } from '../lib/types'
import { useWorkspaceStore } from '../stores/workspaces'
import { useAppStore } from '../stores/app'

const workspaces = useWorkspaceStore()
const app = useAppStore()
const ws = computed(() => workspaces.active)
const compat = computed(() => ws.value?.compat ?? null)
const plan = ref<MigrationPlan | null>(null)
const busy = ref(false)

async function review() {
  if (!ws.value) return
  try {
    plan.value = await api.workspaceMigrationPlan(ws.value.id)
  } catch (e) {
    app.toast('error', `Cannot prepare the migration: ${toIpcError(e).message}`)
  }
}

async function migrate() {
  if (!ws.value || !plan.value) return
  busy.value = true
  try {
    const done = await api.workspaceMigrate(ws.value.id)
    plan.value = null
    await workspaces.load()
    app.toast('success', `Workspace is now on schema ${done.plan.to}. Previous files: ${done.backupDir ?? 'unchanged'}`, 7000)
  } catch (e) {
    app.toast('error', `Migration failed — nothing was changed: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div v-if="compat && !compat.writable" class="banner readonly" data-workspace-readonly role="status">
    <Lock :size="15" />
    <span><strong>Read-only workspace.</strong> {{ compat.reason }}</span>
  </div>
  <div v-else-if="compat?.needsMigration" class="banner migrate" data-workspace-migrate role="status">
    <ArrowUpCircle :size="15" />
    <span>This workspace uses schema {{ compat.schemaVersion }}. Migrating adds version and layout information — nothing is moved or deleted.</span>
    <button class="btn btn-sm" data-workspace-migrate-review @click="review">Review migration</button>
  </div>
  <BaseModal :open="!!plan" title="Migrate workspace" :subtitle="plan ? `Schema ${plan.from} → ${plan.to}` : ''" @close="plan = null">
    <template v-if="plan">
      <p class="muted">These files change. Their current versions are copied to the NEXUS data folder first and put back if anything fails.</p>
      <ul class="changes" data-migration-changes>
        <li v-for="c in plan.changes" :key="c.path"><code>{{ c.path }}</code> <span class="action">{{ c.action }}</span> — {{ c.description }}</li>
      </ul>
      <ul class="notes">
        <li v-for="n in plan.notes" :key="n">{{ n }}</li>
      </ul>
    </template>
    <template #footer>
      <button class="btn btn-ghost" @click="plan = null">Cancel</button>
      <button class="btn btn-primary" :disabled="busy" data-migration-apply @click="migrate">Migrate</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.banner {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.banner span {
  flex: 1;
}
.readonly {
  background: var(--amber-soft);
  color: var(--text);
}
.readonly :deep(svg) {
  color: var(--amber);
}
.migrate {
  background: var(--accent-soft);
}
.changes,
.notes {
  margin: 10px 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.55;
}
.notes {
  color: var(--text-muted);
}
.action {
  color: var(--text-faint);
}
</style>
