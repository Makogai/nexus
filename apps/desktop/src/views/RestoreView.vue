<script setup lang="ts">
// Restore on this computer (Portable Workspace P8): the ordered steps from a cloned workspace to continuing work —
// Claude Code, schema, index, approvals, skills, projects, tools, profiles, gates — each with its state and the place
// (or reviewable repair) that resolves it; what came with the workspace, and what cannot come back from it.
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { CircleAlert, CircleCheck, CircleDashed, CircleMinus, PackageCheck, RefreshCw, Wrench } from '@lucide/vue'
import BaseModal from '../components/BaseModal.vue'
import { api, toIpcError } from '../lib/ipc'
import type { RepairPlan, Restoration } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useWorkspaceStore } from '../stores/workspaces'

const app = useAppStore()
const router = useRouter()
const workspaces = useWorkspaceStore()
const projects = useProjectStore()
const r = ref<Restoration | null>(null)
const busy = ref(false)
const plan = ref<RepairPlan | null>(null)

const ICON = { done: CircleCheck, todo: CircleDashed, blocked: CircleAlert, optional: CircleMinus } as const
const VIEW: Record<string, string> = { 'claude-setup': 'claude-setup', workspace: 'workspace', knowledge: 'knowledge', constitutions: 'constitutions', kits: 'kits', projects: 'projects', profiles: 'profiles', workflows: 'workflows' }
const restoredLine = computed(() => {
  const x = r.value?.restored
  if (!x) return ''
  return [
    `${x.constitutions} constitutions`, `${x.rules} rules`, `${x.workflows} workflows`, `${x.skills} skills`, `${x.projects} projects`,
    `${x.tasks} tasks`, `${x.decisions} decisions`, `${x.notes} notes`, `${x.developmentRecords} development records`,
  ].join(' · ')
})

async function load() {
  const ws = workspaces.active
  if (!ws) return
  busy.value = true
  try {
    r.value = await api.workspaceRestoration(ws.id)
  } catch (e) {
    app.toast('error', `Could not check the restoration: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
onMounted(load)

async function review(id: string) {
  const ws = workspaces.active
  if (!ws) return
  try {
    plan.value = await api.workspaceRepairPlan(ws.id, id)
  } catch (e) {
    app.toast('error', toIpcError(e).message)
  }
}

async function apply() {
  const ws = workspaces.active
  if (!ws || !plan.value) return
  busy.value = true
  try {
    const done = await api.workspaceRepair(ws.id, plan.value.id)
    plan.value = null
    app.toast('success', done.message, 6000)
    await Promise.all([workspaces.load(), projects.reload()])
  } catch (e) {
    app.toast('error', toIpcError(e).message, 9000)
  } finally {
    busy.value = false
  }
  await load()
}
</script>

<template>
  <div class="page" data-restore-view>
    <div class="page-header">
      <div>
        <h1>Restore on this computer</h1>
        <p>Your workspace came from Git. These steps bring this computer to where you can continue — each is done where it belongs, and repairs show what they change first.</p>
      </div>
      <div class="page-actions"><button class="btn btn-ghost" :disabled="busy" data-restore-refresh @click="load"><RefreshCw :class="{ spin: busy }" /> Check again</button></div>
    </div>

    <template v-if="r">
      <section class="card" data-restore-progress>
        <strong>{{ r.name }}</strong> — {{ r.done }} of {{ r.total }} steps done
        <div class="bar"><div class="fill" :style="{ width: `${r.total ? (100 * r.done) / r.total : 0}%` }" /></div>
      </section>

      <section class="card">
        <ol class="steps">
          <li v-for="s in r.steps" :key="s.id" :class="`step ${s.status}`" :data-restore-step="s.id" :data-restore-status="s.status">
            <component :is="ICON[s.status]" class="ic" />
            <div class="body">
              <strong>{{ s.title }}</strong>
              <p>{{ s.summary }}</p>
              <ul v-if="s.detail.length" class="detail"><li v-for="d in s.detail" :key="d">{{ d }}</li></ul>
            </div>
            <div class="actions">
              <button v-if="s.repair && s.status !== 'done'" class="btn btn-sm" :data-restore-repair="s.repair" @click="review(s.repair)"><Wrench /> Do it…</button>
              <button v-if="s.navigate && VIEW[s.navigate]" class="btn btn-sm btn-ghost" :data-restore-open="s.id" @click="router.push({ name: VIEW[s.navigate] })">Open</button>
            </div>
          </li>
        </ol>
      </section>

      <div class="grid grid-2">
        <section class="card" data-restore-restored>
          <h2 class="card-title"><PackageCheck /> Came with the workspace</h2>
          <p>{{ restoredLine }}</p>
          <p v-if="r.restored.template" class="faint">Template {{ r.restored.template }}</p>
        </section>
        <section class="card" data-restore-not-recoverable>
          <h2 class="card-title"><CircleAlert /> Not recoverable from the workspace</h2>
          <ul class="detail"><li v-for="n in r.notRecoverable" :key="n">{{ n }}</li></ul>
        </section>
      </div>
    </template>

    <BaseModal :open="!!plan" :title="plan?.title ?? ''" subtitle="What this changes" @close="plan = null">
      <template v-if="plan">
        <ul class="detail" data-restore-plan><li v-for="c in plan.changes" :key="c">{{ c }}</li></ul>
        <ul v-if="plan.notes.length" class="detail faint"><li v-for="n in plan.notes" :key="n">{{ n }}</li></ul>
      </template>
      <template #footer>
        <button class="btn btn-ghost" @click="plan = null">Cancel</button>
        <button class="btn btn-primary" :disabled="busy" data-restore-apply @click="apply">Apply</button>
      </template>
    </BaseModal>
  </div>
</template>

<style scoped>
.bar {
  height: 6px;
  margin-top: 8px;
  border-radius: 3px;
  background: var(--bg-input);
  overflow: hidden;
}
.fill {
  height: 100%;
  background: var(--accent);
}
.steps {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  gap: 8px;
}
.step {
  display: grid;
  grid-template-columns: 18px 1fr auto;
  gap: 10px;
  align-items: start;
  padding: 10px 12px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}
.step.done .ic {
  color: var(--green);
}
.step.todo .ic {
  color: var(--amber);
}
.step.blocked .ic {
  color: var(--red);
}
.step.optional .ic {
  color: var(--text-faint);
}
.ic {
  width: 16px;
  height: 16px;
  margin-top: 2px;
}
.body p {
  margin: 2px 0 0;
  font-size: 13px;
  color: var(--text-muted);
}
.detail {
  margin: 4px 0 0;
  padding-left: 16px;
  font-size: 12.5px;
  color: var(--text-muted);
}
.actions {
  display: flex;
  gap: 6px;
}
</style>
