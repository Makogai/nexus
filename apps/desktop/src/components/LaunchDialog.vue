<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { FolderOpen, ListTodo, Sparkles, TriangleAlert } from '@lucide/vue'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useAppStore } from '../stores/app'
import { useProjectStore } from '../stores/projects'
import { useProfileStore } from '../stores/profiles'
import { useSessionStore } from '../stores/sessions'
import { toIpcError } from '../lib/ipc'
import { shortPath } from '../lib/format'
import { initialLaunchSelection, profileDefaults } from '../lib/launch'
import BaseModal from './BaseModal.vue'

const app = useAppStore()
const projects = useProjectStore()
const profiles = useProfileStore()
const sessions = useSessionStore()
const router = useRouter()

const projectId = ref<string>('')
const customDir = ref<string | null>(null)
const profileId = ref<string>('')
const workProfile = ref<string>('')
const effort = ref<string>('')
const model = ref('')
const title = ref('')
const busy = ref(false)
const error = ref<string | null>(null)
const startTask = ref(true)
const kickoff = ref(false)
const task = computed(() => app.launchDialog.task ?? null)

const isOpen = computed(() => app.launchDialog.open)
const availableProjects = computed(() => projects.list.filter((p) => p.available))
const profile = computed(() => profiles.byId[profileId.value] ?? null)
const options = computed(() => profiles.launchOptions)
const selectedWork = computed(() => options.value?.workProfiles.find((w) => w.id === workProfile.value) ?? null)
const canLaunch = computed(() => !!profile.value && (!!projectId.value || !!customDir.value) && !busy.value)

watch(isOpen, (open) => {
  if (!open) return
  error.value = null
  title.value = ''
  model.value = ''
  customDir.value = null
  startTask.value = true
  // A task session starts working right away unless the person turned that off last time.
  kickoff.value = app.launchDialog.kickoff ?? app.prefs.lastLaunch?.taskKickoff ?? true
  projectId.value = app.launchDialog.projectId ?? availableProjects.value[0]?.id ?? ''
  const sel = initialLaunchSelection(
    profiles.list,
    app.prefs.lastLaunch,
    app.launchDialog.profileId,
    options.value?.defaultWorkProfile ?? 'balanced',
  )
  profileId.value = sel.profileId
  workProfile.value = sel.workProfile
  effort.value = sel.effort
})

function applyProfileDefaults() {
  const d = profileDefaults(profile.value, options.value?.defaultWorkProfile ?? 'balanced')
  workProfile.value = d.workProfile
  effort.value = d.effort
}

async function pickDir() {
  const dir = await openDialog({ directory: true, title: 'Choose a directory for this session' })
  if (typeof dir === 'string') {
    customDir.value = dir
    projectId.value = ''
  }
}

function close() {
  app.launchDialog = { open: false }
}

async function launch() {
  if (!canLaunch.value) return
  busy.value = true
  error.value = null
  try {
    await sessions.startClaude({
      profileId: profileId.value,
      projectId: projectId.value || null,
      cwd: projectId.value ? null : customDir.value,
      workProfile: workProfile.value || null,
      effort: effort.value || null,
      model: model.value.trim() || null,
      title: title.value.trim() || null,
      taskId: task.value?.id ?? null,
      startTask: task.value ? startTask.value : undefined,
      kickoff: task.value ? kickoff.value : undefined,
    })
    app.prefs.lastLaunch = {
      profileId: profileId.value,
      workProfile: workProfile.value,
      effort: effort.value || null,
      taskKickoff: task.value ? kickoff.value : app.prefs.lastLaunch?.taskKickoff,
    }
    close()
  } catch (e) {
    error.value = toIpcError(e).message
  } finally {
    busy.value = false
  }
}

function goToProfiles() {
  close()
  router.push('/profiles')
}
</script>

<template>
  <BaseModal :open="isOpen" title="Launch Claude Code" subtitle="Runs in the integrated terminal with the selected profile's own configuration." :width="640" @close="close">
    <div v-if="profiles.list.length === 0" class="notice notice-warn">
      <TriangleAlert />
      <div>
        No Claude profile is registered yet.
        <button class="link" @click="goToProfiles">Register your Claude configuration</button> first.
      </div>
    </div>
    <template v-else>
      <div v-if="task" class="notice notice-accent task-notice" data-launch-task>
        <ListTodo />
        <div>
          <div>Task: <strong>{{ task.title }}</strong></div>
          <label class="checkbox">
            <input v-model="startTask" type="checkbox" />
            <span>Move it to <em>In progress</em> if it is in Backlog or Ready</span>
          </label>
          <label class="checkbox" data-launch-kickoff>
            <input v-model="kickoff" type="checkbox" />
            <span>Start right away — NEXUS sends Claude the first message</span>
          </label>
          <div v-if="kickoff && options?.taskKickoff" class="hint kickoff-text">“{{ options.taskKickoff }}”</div>
          <div class="hint">Claude gets a short brief of the task (not its history) and records progress with the NEXUS task tools.</div>
        </div>
      </div>
      <div class="field">
        <label for="launch-project">Project</label>
        <div class="input-row">
          <select id="launch-project" v-model="projectId" class="select" :disabled="!!task?.projectId" @change="customDir = null">
            <option v-if="customDir" value="">{{ shortPath(customDir, app.info?.homeDir) }}</option>
            <option v-else-if="availableProjects.length === 0" value="" disabled>No projects on this machine</option>
            <option v-for="p in availableProjects" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
          <button class="btn" title="Use another directory" :disabled="!!task" @click="pickDir"><FolderOpen /></button>
        </div>
      </div>

      <div class="field">
        <label for="launch-profile">Claude profile</label>
        <select id="launch-profile" v-model="profileId" class="select" @change="applyProfileDefaults()">
          <option v-for="p in profiles.list" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
        <span v-if="profile" class="hint">
          <span class="pdot" :style="{ background: profile.color }" />
          <template v-if="profile.configDir">CLAUDE_CONFIG_DIR = <code>{{ shortPath(profile.configDir, app.info?.homeDir, 60) }}</code></template>
          <template v-else>Default location (<code>~/.claude</code>) — CLAUDE_CONFIG_DIR is removed for this session</template>
        </span>
      </div>

      <div class="field">
        <label>Work profile</label>
        <div class="segments" role="radiogroup" aria-label="Work profile">
          <button
            v-for="w in options?.workProfiles ?? []"
            :key="w.id"
            class="segment"
            role="radio"
            :aria-checked="workProfile === w.id"
            :class="{ selected: workProfile === w.id }"
            @click="workProfile = w.id"
          >
            {{ w.name }}
          </button>
        </div>
        <span v-if="selectedWork" class="hint">{{ selectedWork.summary }} {{ selectedWork.characteristics.join(' · ') }}.</span>
      </div>

      <div class="two">
        <div class="field">
          <label for="launch-effort">Reasoning effort</label>
          <select id="launch-effort" v-model="effort" class="select">
            <option value="">Claude default</option>
            <option v-for="e in options?.effortLevels ?? []" :key="e" :value="e">{{ e }}</option>
          </select>
        </div>
        <div class="field">
          <label for="launch-model">Model</label>
          <input id="launch-model" v-model="model" class="input" :placeholder="profile?.defaultModel ?? 'Claude default'" spellcheck="false" />
        </div>
      </div>

      <div class="field">
        <label for="launch-title">Session name <span class="faint">(optional)</span></label>
        <input id="launch-title" v-model="title" class="input" :placeholder="task?.title ?? 'e.g. Fix login redirect'" @keydown.enter="launch" />
      </div>

      <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    </template>

    <template #footer>
      <span class="faint footnote">Work profile and model are fixed for the conversation once it starts.</span>
      <button class="btn" @click="close">Cancel</button>
      <button class="btn btn-primary" :disabled="!canLaunch" @click="launch">
        <Sparkles /> {{ busy ? 'Launching…' : 'Launch' }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.segments {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: var(--bg-input);
  border: 1px solid var(--border-strong);
}
.segment {
  height: 30px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--text-muted);
  font-weight: 600;
  cursor: pointer;
}
.segment:hover {
  color: var(--text);
}
.segment.selected {
  background: var(--accent-soft);
  color: var(--accent-strong);
  box-shadow: inset 0 0 0 1px var(--accent-ring);
}
.task-notice {
  margin-bottom: 14px;
  align-items: flex-start;
}
.task-notice .checkbox {
  margin-top: 6px;
}
.kickoff-text {
  font-style: italic;
  margin: 2px 0 4px 24px;
}
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.pdot {
  display: inline-block;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  margin-right: 6px;
}
.link {
  border: none;
  background: none;
  color: var(--accent-strong);
  cursor: pointer;
  padding: 0;
  text-decoration: underline;
}
.footnote {
  flex: 1;
  font-size: 12px;
  align-self: center;
}
</style>
