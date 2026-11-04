<script setup lang="ts">
import { ref, watch } from 'vue'
import { Info, Play, Plus, ShieldCheck, Trash2 } from '@lucide/vue'
import { api } from '../lib/ipc'
import type { Gate, GateInput, GateResult } from '../lib/types'
import { relativeTime } from '../lib/format'
import { joinArgs, splitArgs } from '../lib/workflow'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'

const app = useAppStore()
const projects = useProjectStore()
const projectId = ref(projects.list[0]?.id ?? '')
const gates = ref<[Gate, GateResult | null][]>([])
const suggestions = ref<GateInput[]>([])
const form = ref({ name: '', command: '', timeout: 900 })
const running = ref<string | null>(null)
const output = ref<GateResult | null>(null)

async function load() {
  if (!projectId.value) return
  const [g, s] = await Promise.all([api.gatesList(projectId.value), api.gateSuggest(projectId.value)])
  gates.value = g
  suggestions.value = s.filter((x) => !g.some(([y]) => y.name === x.name))
}
watch(projectId, load, { immediate: true })

async function add(input: GateInput) {
  const ok = await app.attempt(() => api.gateUpsert(projectId.value, input), 'Could not save the gate')
  if (ok) {
    app.toast(ok.approved ? 'success' : 'info', ok.approved ? `Gate ${ok.name} saved (command unchanged, still approved)` : `Gate ${ok.name} v${ok.version} saved — approve its command before it runs`)
    await load()
  }
}
async function approve(g: Gate) {
  const ok = await app.confirm({
    title: `Approve gate ${g.name} (version ${g.version})?`,
    message: `NEXUS will run exactly this command in the project folder, without a shell, whenever a workflow or a constitution check needs "${g.name}":

${joinArgs([g.program, ...g.args])}

If the command changes it becomes a new version and needs approval again. This does not approve any workflow.`,
    confirmLabel: 'Approve command',
  })
  if (!ok) return
  if (await app.attempt(() => api.gateApprove(projectId.value, g.name, g.commandHash), 'Could not approve (the command changed?)')) await load()
}
async function revoke(g: Gate) {
  await app.attempt(() => api.gateRevoke(projectId.value, g.name), 'Could not withdraw the approval')
  await load()
}
async function addForm() {
  const argv = splitArgs(form.value.command)
  if (!form.value.name || !argv.length) return
  await add({ name: form.value.name.trim(), program: argv[0]!, args: argv.slice(1), timeoutSecs: form.value.timeout })
  form.value = { name: '', command: '', timeout: 900 }
}
async function remove(g: Gate) {
  const ok = await app.confirm({ title: `Remove gate ${g.name}?`, message: 'Workflow steps that name this gate will be blocked until it is defined again.', confirmLabel: 'Remove', danger: true })
  if (ok) {
    await app.attempt(() => api.gateRemove(projectId.value, g.name), 'Could not remove')
    await load()
  }
}
async function run(g: Gate) {
  running.value = g.name
  output.value = (await app.attempt(() => api.gateRun(projectId.value, g.name), 'Could not run the gate')) ?? null
  running.value = null
  await load()
}
function edit(g: Gate) {
  form.value = { name: g.name, command: joinArgs([g.program, ...g.args]), timeout: g.timeoutSecs }
}
</script>

<template>
  <section class="gates" data-gates>
    <p class="notice"><Info /> Gates are how NEXUS <strong>verifies</strong> work: commands you define here, for this project, on this machine. Workflow files only name gates (test, lint …) and never carry commands. A gate runs only after you <strong>approve its command</strong>; changing the command makes a new version that needs approval again. Gates run without a shell, with your environment minus Claude's session variables, a timeout, and their output recorded.</p>
    <div class="filters">
      <select v-model="projectId" class="select" aria-label="Project" data-gates-project>
        <option v-for="p in projects.list" :key="p.id" :value="p.id">{{ p.name }}</option>
      </select>
    </div>
    <table v-if="gates.length" class="table">
      <thead><tr><th>Name</th><th>Command</th><th>Approval</th><th>Timeout</th><th>Last result</th><th /></tr></thead>
      <tbody>
        <tr v-for="[g, last] in gates" :key="g.id" :data-gate="g.name" :data-approved="g.approved ? 'yes' : 'no'" :data-version="g.version">
          <td class="mono">{{ g.name }}</td>
          <td><code>{{ joinArgs([g.program, ...g.args]) }}</code></td>
          <td>
            <span v-if="g.approved" class="badge badge-ok" :title="g.approvedAt ? `approved ${relativeTime(g.approvedAt)}` : ''">v{{ g.version }} approved</span>
            <span v-else class="badge badge-warn">v{{ g.version }} needs approval</span>
          </td>
          <td>{{ g.timeoutSecs }} s</td>
          <td><span v-if="last" class="badge" :class="{ 'badge-ok': last.status === 'passed', 'badge-error': last.status !== 'passed' }">{{ last.status }}</span> <span v-if="last" class="faint small">{{ relativeTime(last.startedAt) }}</span><span v-else class="faint small">never run</span></td>
          <td class="tools">
            <button v-if="!g.approved" class="btn btn-sm btn-primary" :data-approve-gate="g.name" @click="approve(g)"><ShieldCheck /> Approve</button>
            <button v-else class="btn btn-sm btn-ghost" :data-revoke-gate="g.name" title="Withdraw the approval; the gate stays defined but does not run" @click="revoke(g)">Revoke</button>
            <button class="btn btn-sm" :disabled="running === g.name || !g.approved" :title="g.approved ? '' : 'Approve the command first'" :data-run-gate="g.name" @click="run(g)"><Play /> {{ running === g.name ? 'Running…' : 'Run now' }}</button>
            <button class="btn btn-sm btn-ghost" @click="edit(g)">Edit</button>
            <button class="btn btn-sm btn-ghost btn-icon" title="Remove" @click="remove(g)"><Trash2 /></button>
          </td>
        </tr>
      </tbody>
    </table>
    <p v-else class="faint">No gates for this project yet.</p>
    <pre v-if="output" class="output selectable" data-gate-output>{{ output.gateName }} {{ output.status }} (exit {{ output.exitCode ?? '–' }}, {{ ((output.durationMs ?? 0) / 1000).toFixed(1) }} s)
{{ output.outputTail }}</pre>

    <div v-if="suggestions.length" class="card">
      <div class="card-title">Detected from the project's scripts</div>
      <div v-for="s in suggestions" :key="s.name" class="suggest" :data-suggest="s.name">
        <span class="mono">{{ s.name }}</span><code>{{ joinArgs([s.program, ...s.args]) }}</code>
        <button class="btn btn-sm" @click="add(s)"><Plus /> Add</button>
      </div>
    </div>

    <div class="card">
      <div class="card-title">Define a gate</div>
      <div class="form">
        <input v-model="form.name" class="input mono" placeholder="name (test, lint, typecheck…)" aria-label="Gate name" data-gate-name />
        <input v-model="form.command" class="input mono grow" placeholder="command, e.g. npm test -- --run" aria-label="Command" data-gate-command />
        <input v-model.number="form.timeout" class="input timeout" type="number" min="5" aria-label="Timeout seconds" />
        <button class="btn btn-primary" :disabled="!form.name || !form.command" data-gate-save @click="addForm">Save</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.gates {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.filters .select {
  min-width: 220px;
}
.table {
  width: 100%;
  border-collapse: collapse;
}
.table th {
  text-align: left;
  font-size: 12px;
  color: var(--text-faint);
  padding: 4px 6px;
}
.table td {
  padding: 6px;
  border-top: 1px solid var(--border);
}
.tools {
  display: flex;
  gap: 4px;
  justify-content: flex-end;
}
.small {
  font-size: 12px;
}
.output {
  max-height: 260px;
  overflow: auto;
  padding: 10px;
  border-radius: 8px;
  background: var(--bg-input);
  white-space: pre-wrap;
  font-size: 12px;
}
.suggest {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 0;
}
.form {
  display: flex;
  gap: 8px;
}
.form .grow {
  flex: 1;
}
.form .timeout {
  width: 90px;
}
.form .input:first-child {
  width: 180px;
}
</style>
