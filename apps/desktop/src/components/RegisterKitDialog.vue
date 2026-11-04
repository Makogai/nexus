<script setup lang="ts">
import { ref, watch } from 'vue'
import { FolderOpen, GitFork, TriangleAlert } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { api, toIpcError } from '../lib/ipc'
import type { KitView } from '../lib/types'
import BaseModal from './BaseModal.vue'

const props = defineProps<{ open: boolean }>()
const emit = defineEmits<{ close: []; registered: [kit: KitView] }>()

const mode = ref<'local' | 'git'>('local')
const source = ref('')
const gitRef = ref('')
const subdir = ref('')
const plugin = ref('')
const busy = ref(false)
const error = ref<string | null>(null)

watch(
  () => props.open,
  (o) => {
    if (!o) return
    mode.value = 'local'
    source.value = gitRef.value = subdir.value = plugin.value = ''
    error.value = null
  },
)

async function pick() {
  const dir = await openDialog({ directory: true, title: 'Choose a Claude Code plugin or a folder of skills' })
  if (typeof dir === 'string') source.value = dir
}

async function register() {
  busy.value = true
  error.value = null
  try {
    const kit = await api.kitRegister({
      source: source.value.trim(),
      git: mode.value === 'git',
      gitRef: gitRef.value.trim() || null,
      subdir: subdir.value.trim() || null,
      plugin: plugin.value.trim() || null,
    })
    emit('registered', kit)
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="open" title="Register an AI Kit" subtitle="A Claude Code plugin or a folder of skills. Nothing is enabled until you review it and choose profiles." :width="600" @close="emit('close')">
    <div class="segments" role="radiogroup" aria-label="Source">
      <button class="segment" role="radio" :aria-checked="mode === 'local'" :class="{ selected: mode === 'local' }" data-mode="local" @click="mode = 'local'"><FolderOpen /> Local folder</button>
      <button class="segment" role="radio" :aria-checked="mode === 'git'" :class="{ selected: mode === 'git' }" data-mode="git" @click="mode = 'git'"><GitFork /> Git repository</button>
    </div>
    <div v-if="mode === 'local'" class="field">
      <label for="kit-folder">Folder</label>
      <div class="input-row">
        <input id="kit-folder" v-model="source" class="input mono" placeholder="Folder with .claude-plugin/plugin.json or skills/" />
        <button class="btn" title="Choose folder" @click="pick"><FolderOpen /></button>
      </div>
      <span class="hint">Used in place — NEXUS reads it and never writes to it.</span>
    </div>
    <template v-else>
      <div class="field">
        <label for="kit-url">Repository URL</label>
        <input id="kit-url" v-model="source" class="input mono" placeholder="https://github.com/org/kit.git or git@host:org/kit.git" />
        <span class="hint">NEXUS makes a shallow clone in its own data folder (uses your Git credentials; nothing is stored).</span>
      </div>
      <div class="field">
        <label for="kit-ref">Branch or tag <span class="faint">(optional)</span></label>
        <input id="kit-ref" v-model="gitRef" class="input mono" placeholder="main" />
      </div>
    </template>
    <div class="two">
      <div class="field">
        <label for="kit-subdir">Subfolder <span class="faint">(optional)</span></label>
        <input id="kit-subdir" v-model="subdir" class="input mono" placeholder="plugins/my-kit" />
      </div>
      <div class="field">
        <label for="kit-plugin">Plugin <span class="faint">(marketplaces)</span></label>
        <input id="kit-plugin" v-model="plugin" class="input mono" placeholder="plugin name" />
      </div>
    </div>
    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <button class="btn btn-primary" :disabled="!source.trim() || busy" data-register @click="register">{{ busy ? 'Registering…' : 'Register and inspect' }}</button>
    </template>
  </BaseModal>
</template>

<style scoped>
.segments {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
  margin-bottom: 14px;
}
.segment {
  height: 32px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  font-weight: 600;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}
.segment.selected {
  background: var(--accent-soft);
  color: var(--accent-strong);
}
.segment svg {
  width: 15px;
  height: 15px;
}
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
</style>
