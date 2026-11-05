<script setup lang="ts">
// Claude Code setup (Portable Workspace P5): installed? which version? startable? — and, when missing, the official
// installation methods for this platform. NEXUS types a method into a terminal for you; you press Enter. Sign-in is
// Claude Code's own browser flow; NEXUS never sees passwords or tokens.
import { onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { CircleCheck, Copy, ExternalLink, RefreshCw, SquareTerminal, TriangleAlert } from '@lucide/vue'
import { openUrl } from '@tauri-apps/plugin-opener'
import { api, toIpcError } from '../lib/ipc'
import type { ClaudeSetupStatus } from '../lib/types'
import { typeIntoTerminal } from '../lib/terminal'
import { useAppStore } from '../stores/app'

const app = useAppStore()
const router = useRouter()
const st = ref<ClaudeSetupStatus | null>(null)
const busy = ref(false)

async function load() {
  busy.value = true
  try {
    st.value = await api.claudeSetupStatus()
  } catch (e) {
    app.toast('error', `Could not check Claude Code: ${toIpcError(e).message}`)
  } finally {
    busy.value = false
  }
}
onMounted(load)

async function copy(text: string) {
  await navigator.clipboard.writeText(text)
  app.toast('success', 'Copied')
}
</script>

<template>
  <div class="page" data-claude-setup>
    <div class="page-header">
      <div>
        <h1>Claude Code setup</h1>
        <p>NEXUS runs Claude Code in its own terminals. Install it with an official method — NEXUS types the command into a terminal for you to run; it never installs anything on its own and never handles your sign-in.</p>
      </div>
      <div class="page-actions">
        <button class="btn btn-ghost" :disabled="busy" data-setup-refresh @click="load"><RefreshCw :class="{ spin: busy }" /> Check again</button>
      </div>
    </div>

    <template v-if="st">
      <section class="card" data-setup-status>
        <div v-if="st.ready" class="state ok"><CircleCheck /> <strong>Claude Code {{ st.version ?? '' }}</strong> is installed and NEXUS can start it.</div>
        <div v-else class="state warn"><TriangleAlert /> <strong>Claude Code is not installed</strong> (or NEXUS cannot start it).</div>
        <ul class="next">
          <li v-for="n in st.next" :key="n">{{ n }}</li>
        </ul>
        <div class="row">
          <button class="btn btn-sm" @click="router.push({ name: 'profiles' })">Claude profiles ({{ st.profiles }})</button>
        </div>
      </section>

      <section v-if="!st.ready" class="card" data-setup-methods>
        <h2 class="card-title">Official installation methods</h2>
        <p class="faint">From <a href="#" @click.prevent="openUrl(st.docsUrl)">{{ st.docsUrl }} <ExternalLink class="ext" /></a> (verified {{ st.verified }}).</p>
        <div v-for="m in st.methods" :key="m.id" class="method" :data-method="m.id">
          <div class="method-head">
            <strong>{{ m.title }}</strong>
            <span v-if="m.recommended" class="badge badge-ok">recommended</span>
            <span class="badge">{{ m.autoUpdates ? 'updates itself' : 'update by hand' }}</span>
            <span class="faint">{{ m.shell === 'any' ? 'any terminal' : m.shell }}</span>
          </div>
          <code class="cmd selectable">{{ m.command }}</code>
          <p class="faint">{{ m.notes }}</p>
          <div class="row">
            <button class="btn btn-sm" :data-method-type="m.id" @click="typeIntoTerminal(m.command, { shell: m.shell })"><SquareTerminal /> Type into a terminal</button>
            <button class="btn btn-sm btn-ghost" @click="copy(m.command)"><Copy /> Copy</button>
          </div>
        </div>
      </section>

      <section v-if="st.installations.length" class="card">
        <h2 class="card-title">Found on this computer</h2>
        <ul class="installs">
          <li v-for="i in st.installations" :key="i.path"><span class="mono">{{ i.path }}</span> — {{ i.version ?? 'version unknown' }}<span v-if="!i.launchable" class="faint"> · {{ i.issue }}</span></li>
        </ul>
      </section>
    </template>
  </div>
</template>

<style scoped>
.state {
  display: flex;
  align-items: center;
  gap: 8px;
}
.state.ok :deep(svg) {
  color: var(--green);
}
.state.warn :deep(svg) {
  color: var(--amber);
}
.next {
  margin: 10px 0;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.6;
}
.method {
  padding: 10px 0;
  border-top: 1px solid var(--border);
}
.method-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.cmd {
  display: block;
  margin: 8px 0 4px;
  padding: 8px 10px;
  background: var(--bg-input);
  border: 1px solid var(--border);
  border-radius: var(--radius-xs);
  font-family: var(--font-mono);
  font-size: 12.5px;
}
.row {
  display: flex;
  gap: 8px;
  margin-top: 6px;
}
.installs {
  padding-left: 18px;
  font-size: 13px;
}
.ext {
  width: 12px;
  height: 12px;
  vertical-align: -1px;
}
</style>
