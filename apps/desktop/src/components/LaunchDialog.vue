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
/** Where a task's work happens: a new session, the task's own session (resume or open), or a running session of the
 * project that takes the task on — the cheaper choices reuse context Claude already has. */
type StartIn = 'new' | 'own' | 'continue'
const startIn = ref<StartIn>('new')
const continueId = ref('')
const task = computed(() => app.launchDialog.task ?? null)

const isOpen = computed(() => app.launchDialog.open)
/** The task's latest Claude session. */
const ownSession = computed(() => {
  const t = task.value
  if (!t) return null
  return sessions.list.filter((s) => s.kind === 'claude' && s.taskId === t.id && !s.archivedAt).sort((a, b) => b.startedAt - a.startedAt)[0] ?? null
})
const ownUsable = computed(() => !!ownSession.value && ((ownSession.value.running && ownSession.value.attached) || ownSession.value.resumable))
/** Running Claude sessions of the task's project in this window, with NEXUS tools (they read the task with get_task). */
const runningInProject = computed(() =>
  task.value
    ? sessions.list.filter(
        (s) => s.kind === 'claude' && s.running && s.attached && s.projectId === projectId.value && s.id !== ownSession.value?.id && s.integrations?.mcp?.state === 'enabled',
      )
    : [],
)
const startHint = computed(() => {
  if (startIn.value === 'own') {
    return ownSession.value?.running
      ? 'This task already has a running session — NEXUS opens its terminal.'
      : 'Continues this task’s own conversation — the cheapest way to pick the same task up again.'
  }
  if (startIn.value === 'continue') {
    const s = runningInProject.value.find((x) => x.id === continueId.value)
    return `Hands the task to “${s?.title ?? 'the session'}”, which already has this project’s context — cheapest for small, related tasks; its context keeps growing.`
  }
  return 'A clean context: Claude reads the project again. Best for unrelated work.'
})
const launchLabel = computed(() => (startIn.value === 'own' ? (ownSession.value?.running ? 'Open its terminal' : 'Resume') : startIn.value === 'continue' ? 'Hand over' : 'Launch'))
const availableProjects = computed(() => projects.list.filter((p) => p.available))
const profile = computed(() => profiles.byId[profileId.value] ?? null)
const options = computed(() => profiles.launchOptions)
const selectedWork = computed(() => options.value?.workProfiles.find((w) => w.id === workProfile.value) ?? null)
const canLaunch = computed(() => {
  if (busy.value) return false
  if (task.value && startIn.value === 'own') return ownUsable.value
  if (task.value && startIn.value === 'continue') return !!continueId.value
  return !!profile.value && (!!projectId.value || !!customDir.value)
})

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
  // The task's own session when it has one (cheapest); otherwise a new one. Handing over is always a deliberate choice.
  startIn.value = ownUsable.value ? 'own' : 'new'
  continueId.value = runningInProject.value[0]?.id ?? ''
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
    if (task.value && startIn.value === 'own' && ownSession.value) {
      const s = ownSession.value
      if (s.running) sessions.openTab(s.id)
      else await sessions.resume(s.id)
      close()
      return
    }
    if (task.value && startIn.value === 'continue' && continueId.value) {
      await sessions.continueTask(continueId.value, task.value.id)
      close()
      return
    }
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
      <div v-if="task && (ownUsable || runningInProject.length)" class="field" data-launch-start-in>
        <label>Start in</label>
        <div class="segments" role="radiogroup" aria-label="Start in">
          <button class="segment" role="radio" data-start-in="new" :aria-checked="startIn === 'new'" :class="{ selected: startIn === 'new' }" @click="startIn = 'new'">New session</button>
          <button v-if="ownUsable" class="segment" role="radio" data-start-in="own" :aria-checked="startIn === 'own'" :class="{ selected: startIn === 'own' }" @click="startIn = 'own'">
            {{ ownSession?.running ? 'Its running session' : 'Resume its session' }}
          </button>
          <button v-if="runningInProject.length" class="segment" role="radio" data-start-in="continue" :aria-checked="startIn === 'continue'" :class="{ selected: startIn === 'continue' }" @click="startIn = 'continue'">
            A running session
          </button>
        </div>
        <select v-if="startIn === 'continue' && runningInProject.length > 1" v-model="continueId" class="select" aria-label="Running session" data-continue-session>
          <option v-for="s in runningInProject" :key="s.id" :value="s.id">{{ s.title }}</option>
        </select>
        <span class="hint" data-start-in-hint>{{ startHint }}</span>
      </div>
      <div v-show="!task || startIn === 'new'" class="fresh">
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

      </div>
      <div v-if="error" class="notice notice-error selectable"><TriangleAlert /> {{ error }}</div>
    </template>

    <template #footer>
      <span class="faint footnote">Work profile and model are fixed for the conversation once it starts.</span>
      <button class="btn" @click="close">Cancel</button>
      <button class="btn btn-primary" :disabled="!canLaunch" @click="launch">
        <Sparkles /> {{ busy ? 'Launching…' : launchLabel }}
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
