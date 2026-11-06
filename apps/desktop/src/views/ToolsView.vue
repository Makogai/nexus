<script setup lang="ts">
// What NEXUS gives Claude sessions, in one place (owner feedback: "where do I see the nexus:learn-project skill, and the
// NEXUS MCP server's status and tools?"): built-in skills (exactly as Claude reads them), the MCP tools as the real
// `nexus mcp` server lists them, and the hook events NEXUS subscribes to. Skills from AI Kits and the workspace are
// managed in AI Kits.
import { computed, onMounted, ref } from 'vue'
import { RouterLink } from 'vue-router'
import { Anchor, BookOpenCheck, Package, PlugZap, RefreshCw, TriangleAlert, Wrench } from '@lucide/vue'
import BaseModal from '../components/BaseModal.vue'
import MarkdownView from '../components/MarkdownView.vue'
import { api, toIpcError } from '../lib/ipc'
import type { NexusTools } from '../lib/types'

const data = ref<NexusTools | null>(null)
const error = ref<string | null>(null)
const busy = ref(false)
const openSkill = ref<string | null>(null)
const skill = computed(() => data.value?.skills.find((s) => s.id === openSkill.value) ?? null)

async function load() {
  busy.value = true
  error.value = null
  try {
    data.value = await api.nexusTools()
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
onMounted(load)

/** Tools that only read, and tools that write knowledge records (the only writes NEXUS's MCP server makes). */
const reads = computed(() => data.value?.tools.filter((t) => t.annotations?.readOnlyHint) ?? [])
const writes = computed(() => data.value?.tools.filter((t) => !t.annotations?.readOnlyHint) ?? [])
</script>

<template>
  <div class="page" data-tools-view>
    <div class="page-header">
      <div>
        <h1>NEXUS tools</h1>
        <p>What NEXUS gives the Claude sessions it starts: its own skills, the NEXUS MCP server's tools and the lifecycle hooks. Your workspace and project skills are written and edited in <RouterLink to="/skills">Skills</RouterLink>; plugins in <RouterLink to="/kits">AI Kits</RouterLink>.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="busy" @click="load"><RefreshCw :class="{ spin: busy }" /> Refresh</button>
      </div>
    </div>

    <p v-if="error" class="notice notice-error"><TriangleAlert /> <span>{{ error }}</span></p>

    <template v-if="data">
      <section class="card" data-tools-skills>
        <h2 class="card-title"><BookOpenCheck /> Built-in skills <span class="faint">({{ data.skills.length }})</span></h2>
        <p class="faint small">Shipped inside NEXUS, read-only, updated with NEXUS. Given only to the sessions that need them, so other sessions pay nothing for them.</p>
        <div class="list">
          <div v-for="s in data.skills" :key="s.id" class="list-row skill" :data-skill="s.id">
            <code>{{ s.id }}</code>
            <span class="desc">{{ s.description }}</span>
            <span class="badge">{{ s.givenTo }}</span>
            <button class="btn btn-sm" @click="openSkill = s.id">View</button>
          </div>
        </div>
      </section>

      <section class="card" data-tools-mcp>
        <h2 class="card-title"><PlugZap /> NEXUS MCP server <span class="faint">({{ data.tools.length }} tools)</span></h2>
        <p class="faint small">
          Each Claude session NEXUS starts gets its own NEXUS MCP server (<code>nexus mcp</code>), bound to that session and its workspace. The tools read the workspace and write only
          NEXUS's own records — knowledge files (tasks, notes, decisions, drafts) and requests for you in NEXUS (commit messages, pushes, attention); they never run commands,
          reach the network or open other files.
          <template v-if="data.cli"> Program: <span class="mono">{{ data.cli }}</span>.</template>
        </p>
        <p v-if="data.toolsError" class="notice notice-warn"><TriangleAlert /> <span>{{ data.toolsError }}</span></p>
        <h3 class="sub">Write knowledge records <span class="faint">({{ writes.length }})</span></h3>
        <div class="tools">
          <div v-for="t in writes" :key="t.name" class="tool" :data-tool="t.name"><code>{{ t.name }}</code><span>{{ t.description }}</span></div>
        </div>
        <h3 class="sub">Read <span class="faint">({{ reads.length }})</span></h3>
        <div class="tools">
          <div v-for="t in reads" :key="t.name" class="tool" :data-tool="t.name"><code>{{ t.name }}</code><span>{{ t.description }}</span></div>
        </div>
      </section>

      <section class="card" data-tools-hooks>
        <h2 class="card-title"><Anchor /> Lifecycle hooks <span class="faint">({{ data.hooks.length }})</span></h2>
        <p class="faint small">Claude runs <code>nexus hook</code> for these events, so NEXUS can keep the session's activity, attention and records. Hooks are cheap and deterministic: no model calls, no prompt text stored, and they never decide for Claude.</p>
        <div class="chips">
          <span v-for="h in data.hooks" :key="h" class="chip mono">{{ h }}</span>
        </div>
      </section>

      <p class="notice explain"><Wrench /> <span>A session lists what it actually got under its integrations; an organisation policy can block the MCP server, the hooks or extra plugin folders — the session then says why. Other skills: <RouterLink to="/kits"><Package class="inline-ic" /> AI Kits</RouterLink>.</span></p>
    </template>

    <BaseModal :open="!!skill" :title="skill?.id ?? ''" subtitle="Exactly as Claude reads it" :width="760" @close="openSkill = null">
      <MarkdownView v-if="skill" :source="skill.body" data-skill-body />
    </BaseModal>
  </div>
</template>

<style scoped>
.small {
  font-size: 12px;
}
.skill {
  display: flex;
  align-items: center;
  gap: 10px;
}
.skill .desc {
  flex: 1;
  min-width: 0;
  color: var(--text-muted);
  font-size: 13px;
}
.sub {
  font-size: 13px;
  margin: 14px 0 8px;
}
.tools {
  display: grid;
  gap: 6px;
}
.tool {
  display: grid;
  grid-template-columns: 220px 1fr;
  gap: 12px;
  font-size: 13px;
  color: var(--text-muted);
}
.tool code {
  color: var(--text);
}
.chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.inline-ic {
  width: 13px;
  height: 13px;
  vertical-align: -2px;
}
</style>
