<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { FolderOpen, TriangleAlert } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { toIpcError } from '../lib/ipc'
import type { ClaudeProfile, DetectedProfile, ProfileDialogMode, ProfileInput } from '../lib/types'
import { useAppStore } from '../stores/app'
import { useProfileStore } from '../stores/profiles'
import BaseModal from './BaseModal.vue'

const props = defineProps<{
  open: boolean
  mode: ProfileDialogMode
  profile?: ClaudeProfile | null
  detected?: DetectedProfile | null
}>()
const emit = defineEmits<{ close: [] }>()
const app = useAppStore()
const profiles = useProfileStore()

const COLORS = ['#7c6cf2', '#3ad1c5', '#f5b942', '#f87171', '#60a5fa', '#4ade80', '#e879f9', '#fb923c']

const form = ref<ProfileInput & { extraArgsText: string; useDefaultLocation: boolean }>(blank())
const busy = ref(false)
const error = ref<string | null>(null)

function blank() {
  return {
    name: '',
    description: '',
    color: COLORS[0]!,
    configDir: null as string | null,
    executable: null as string | null,
    extraArgsText: '',
    inheritProviderEnv: false,
    isolateInstructions: false,
    defaultWorkProfile: null as string | null,
    defaultEffort: null as string | null,
    defaultModel: null as string | null,
    useDefaultLocation: false,
  }
}

/** The default-location profile *is* the owner of ~/.claude — there is nothing to isolate it from. */
const isDefaultLocation = computed(() => (props.mode === 'edit' ? props.profile?.configDir === null : form.value.useDefaultLocation && props.mode === 'register'))

const title = computed(() =>
  props.mode === 'create' ? 'Create a new Claude profile' : props.mode === 'edit' ? 'Edit profile' : 'Register a Claude configuration',
)
const subtitle = computed(() =>
  props.mode === 'create'
    ? 'NEXUS creates an empty configuration folder. Claude Code asks you to sign in on first launch.'
    : props.mode === 'edit'
      ? 'The configuration folder of a profile cannot be changed.'
      : 'NEXUS only reads this configuration. Nothing in it is modified.',
)

watch(
  () => props.open,
  (o) => {
    if (!o) return
    error.value = null
    const f = blank()
    const p = props.profile
    if (props.mode === 'edit' && p) {
      Object.assign(f, {
        name: p.name,
        description: p.description,
        color: p.color,
        configDir: p.configDir,
        executable: p.executable,
        extraArgsText: p.extraArgs.join('\n'),
        inheritProviderEnv: p.inheritProviderEnv,
        isolateInstructions: p.isolateInstructions,
        defaultWorkProfile: p.defaultWorkProfile,
        defaultEffort: p.defaultEffort,
        defaultModel: p.defaultModel,
        useDefaultLocation: p.configDir === null,
      })
    } else if (props.detected) {
      f.name = props.detected.suggestedName
      f.configDir = props.detected.configDir
      f.useDefaultLocation = props.detected.configDir === null
      f.color = COLORS[profiles.list.length % COLORS.length]!
    } else {
      f.color = COLORS[profiles.list.length % COLORS.length]!
    }
    form.value = f
  },
)

async function pickDir() {
  const dir = await openDialog({
    directory: true,
    title: props.mode === 'create' ? 'Choose an empty folder for the new profile' : 'Choose a Claude configuration folder',
  })
  if (typeof dir === 'string') {
    form.value.configDir = dir
    form.value.useDefaultLocation = false
  }
}
async function pickExe() {
  const file = await openDialog({ directory: false, title: 'Choose the Claude Code executable' })
  if (typeof file === 'string') form.value.executable = file
}

async function save() {
  busy.value = true
  error.value = null
  const f = form.value
  const input: ProfileInput = {
    name: f.name,
    description: f.description,
    color: f.color,
    configDir: f.useDefaultLocation ? null : f.configDir,
    executable: f.executable || null,
    extraArgs: f.extraArgsText
      .split('\n')
      .map((a) => a.trim())
      .filter(Boolean),
    inheritProviderEnv: f.inheritProviderEnv,
    isolateInstructions: !isDefaultLocation.value && f.isolateInstructions,
    defaultWorkProfile: f.defaultWorkProfile || null,
    defaultEffort: f.defaultEffort || null,
    defaultModel: f.defaultModel?.trim() || null,
  }
  try {
    if (props.mode === 'edit' && props.profile) await profiles.update(props.profile.id, input)
    else if (props.mode === 'create') await profiles.create(input)
    else await profiles.register(input)
    app.toast('success', props.mode === 'edit' ? 'Profile saved' : `Profile “${f.name}” is ready`)
    emit('close')
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <BaseModal :open="open" :title="title" :subtitle="subtitle" :width="620" @close="emit('close')">
    <div class="two">
      <div class="field">
        <label for="pf-name">Name</label>
        <input id="pf-name" v-model="form.name" class="input" placeholder="Personal, Work, Experimental…" autofocus />
      </div>
      <div class="field">
        <label>Color</label>
        <div class="colors">
          <button
            v-for="c in COLORS"
            :key="c"
            class="swatch"
            :class="{ selected: form.color === c }"
            :style="{ background: c }"
            :aria-label="`Color ${c}`"
            @click="form.color = c"
          />
        </div>
      </div>
    </div>
    <div class="field">
      <label for="pf-desc">Description <span class="faint">(optional)</span></label>
      <input id="pf-desc" v-model="form.description" class="input" placeholder="Which account and kits this profile uses" />
    </div>

    <div v-if="mode !== 'edit'" class="field">
      <label>Configuration folder</label>
      <label v-if="mode === 'register'" class="checkbox">
        <input v-model="form.useDefaultLocation" type="checkbox" />
        <span>Default location (<code>~/.claude</code>) — launched without <code>CLAUDE_CONFIG_DIR</code></span>
      </label>
      <div v-if="!form.useDefaultLocation" class="input-row">
        <input v-model="form.configDir" class="input mono" :placeholder="mode === 'create' ? 'e.g. ~/.claude-experimental' : 'Folder with settings.json / .claude.json'" />
        <button class="btn" @click="pickDir"><FolderOpen /></button>
      </div>
    </div>
    <div v-else class="field">
      <label>Configuration folder</label>
      <code class="selectable">{{ profile?.configDir ?? '~/.claude (default location)' }}</code>
    </div>

    <details class="advanced">
      <summary>Launch defaults and advanced options</summary>
      <div class="two">
        <div class="field">
          <label for="pf-wp">Default work profile</label>
          <select id="pf-wp" v-model="form.defaultWorkProfile" class="select">
            <option :value="null">NEXUS default</option>
            <option v-for="w in profiles.launchOptions?.workProfiles ?? []" :key="w.id" :value="w.id">{{ w.name }}</option>
          </select>
        </div>
        <div class="field">
          <label for="pf-effort">Default effort</label>
          <select id="pf-effort" v-model="form.defaultEffort" class="select">
            <option :value="null">Claude default</option>
            <option v-for="e in profiles.launchOptions?.effortLevels ?? []" :key="e" :value="e">{{ e }}</option>
          </select>
        </div>
      </div>
      <div class="two">
        <div class="field">
          <label for="pf-model">Default model</label>
          <input id="pf-model" v-model="form.defaultModel" class="input" placeholder="Claude default" spellcheck="false" />
        </div>
        <div class="field">
          <label for="pf-exe">Claude executable</label>
          <div class="input-row">
            <input id="pf-exe" v-model="form.executable" class="input mono" placeholder="Auto-detect" />
            <button class="btn" @click="pickExe"><FolderOpen /></button>
          </div>
        </div>
      </div>
      <div class="field">
        <label for="pf-args">Extra launcher arguments <span class="faint">(one per line)</span></label>
        <textarea id="pf-args" v-model="form.extraArgsText" class="textarea" placeholder="--permission-mode&#10;acceptEdits" spellcheck="false" />
        <span class="hint">Session flags NEXUS manages (--session-id, --resume, --print…) and permission bypass flags are rejected.</span>
      </div>
      <label class="checkbox">
        <input v-model="form.inheritProviderEnv" type="checkbox" />
        <span>
          Pass <code>ANTHROPIC_API_KEY</code> / <code>ANTHROPIC_BASE_URL</code> from NEXUS's environment
          <span class="hint block">Off by default: an inherited key would silently override this profile's own sign-in.</span>
        </span>
      </label>
    </details>

    <div class="field isolation" data-isolation>
      <label class="checkbox">
        <input v-model="form.isolateInstructions" type="checkbox" :disabled="isDefaultLocation" />
        <span>
          Keep my default profile's personal instructions out of this profile's sessions
          <span class="hint block">
            Claude loads <code>~/.claude/CLAUDE.md</code> and <code>~/.claude/rules/</code> as project memory for every
            project inside your home folder — even under another profile. When this is on, NEXUS adds
            <code>claudeMdExcludes</code> for exactly those files to each session's own settings. Organisation-managed
            and project instructions still load; your Claude configuration is not changed; <code>claude</code> started
            outside NEXUS is not affected.
          </span>
          <span v-if="isDefaultLocation" class="hint block">Not available for the default-location profile — those files are its own memory.</span>
        </span>
      </label>
    </div>

    <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>

    <template #footer>
      <button class="btn" @click="emit('close')">Cancel</button>
      <button class="btn btn-primary" :disabled="busy || !form.name.trim()" @click="save">
        {{ mode === 'edit' ? 'Save' : mode === 'create' ? 'Create profile' : 'Register profile' }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.colors {
  display: flex;
  gap: 8px;
  align-items: center;
  height: 36px;
}
.swatch {
  width: 22px;
  height: 22px;
  border-radius: 50%;
  border: 2px solid transparent;
  cursor: pointer;
}
.swatch.selected {
  border-color: var(--text);
  box-shadow: 0 0 0 2px var(--bg-raised) inset;
}
.advanced {
  margin: 4px 0 12px;
  padding: 12px 14px;
  border: 1px solid var(--border);
  border-radius: 10px;
}
.advanced summary {
  cursor: pointer;
  color: var(--text-muted);
  font-weight: 600;
  margin-bottom: 4px;
}
.advanced[open] summary {
  margin-bottom: 14px;
}
.block {
  display: block;
}
</style>
