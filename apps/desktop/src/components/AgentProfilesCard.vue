<script setup lang="ts">
// Agents beyond Claude Code (ADR-0052): whether each is installed, what NEXUS gives its sessions (full / partial / not
// available, in words), and its profiles. Registering a profile records a home folder only — NEXUS never writes into it.
import { computed, onMounted, reactive, ref } from 'vue'
import { Bot, CircleCheck, CircleDashed, CircleMinus, Plus, TriangleAlert, X } from '@lucide/vue'
import ActionButton from './ActionButton.vue'
import { useAction } from '../composables/useAction'
import { useProfileStore } from '../stores/profiles'
import { useAppStore } from '../stores/app'
import { shortPath } from '../lib/format'
import type { AgentInfo, AgentProfile, AgentProfileInput } from '../lib/types'

const profiles = useProfileStore()
const app = useAppStore()

const load = useAction(() => profiles.loadAgents())
onMounted(() => load.run())

const others = computed<AgentInfo[]>(() => (profiles.agents?.agents ?? []).filter((a) => a.id !== 'claude'))
const profilesOf = (id: string) => (profiles.agents?.profiles ?? []).filter((p) => p.agent === id && !p.archivedAt)
const detected = (id: string) => profiles.agents?.detected.find((d) => d.agent === id) ?? null
const workProfiles = computed(() => profiles.launchOptions?.workProfiles ?? [])

/** The form for a new or edited profile. */
const editing = ref<string | null>(null)
const form = reactive({ agent: 'codex' as AgentProfileInput['agent'], name: '', homeDir: '', executable: '', extraArgs: '', turnNotifications: true, inheritProviderEnv: false, defaultWorkProfile: '', defaultModel: '' })

function openForm(agent: AgentInfo, p: AgentProfile | null) {
  editing.value = p?.id ?? `new:${agent.id}`
  Object.assign(form, {
    agent: agent.id,
    name: p?.name ?? (profilesOf(agent.id).length ? '' : agent.id === 'codex' ? 'Codex' : agent.name),
    homeDir: p?.homeDir ?? '',
    executable: p?.executable ?? '',
    extraArgs: (p?.extraArgs ?? []).join(' '),
    turnNotifications: p?.turnNotifications ?? true,
    inheritProviderEnv: p?.inheritProviderEnv ?? false,
    defaultWorkProfile: p?.defaultWorkProfile ?? '',
    defaultModel: p?.defaultModel ?? '',
  })
}

const save = useAction(
  async () => {
    const input: AgentProfileInput = {
      agent: form.agent,
      name: form.name.trim(),
      homeDir: form.homeDir.trim() || null,
      executable: form.executable.trim() || null,
      extraArgs: form.extraArgs.split(/\s+/).filter(Boolean),
      turnNotifications: form.turnNotifications,
      inheritProviderEnv: form.inheritProviderEnv,
      defaultWorkProfile: form.defaultWorkProfile || null,
      defaultModel: form.defaultModel.trim() || null,
    }
    const id = editing.value
    const p = id && !id.startsWith('new:') ? await profiles.updateAgent(id, input) : await profiles.registerAgent(input)
    editing.value = null
    return p
  },
  { success: (p) => `Saved the ${p.name} profile` },
)

const forget = useAction((p: AgentProfile) => profiles.forgetAgent(p.id), { key: (p) => p.id, failure: 'Could not remove the profile', success: 'Profile removed — its home folder was not touched' })

const SUPPORT = { full: CircleCheck, partial: CircleDashed, unavailable: CircleMinus }
</script>

<template>
  <section class="card" data-agent-profiles>
    <h2 class="card-title"><Bot /> Other agents</h2>
    <p class="faint small">
      NEXUS can also start the OpenAI Codex CLI in its terminal, with the NEXUS tools, your approved rules and the task pointer — given per session, never written into the
      agent's own configuration. For NEXUS sessions, NEXUS's developer instructions take the place of a <code>developer_instructions</code> entry in your
      <code>config.toml</code> (your AGENTS.md files still apply).
    </p>
    <p v-if="load.error.value" class="notice notice-error"><TriangleAlert /> <span>{{ load.error.value }}</span></p>
    <div v-for="a in others" :key="a.id" class="agent" :data-agent="a.id">
      <div class="head">
        <strong>{{ a.name }}</strong>
        <span v-if="a.installed" class="badge badge-ok" data-agent-installed>{{ a.installed.version ?? 'installed' }}</span>
        <span v-else class="badge badge-warn">not installed</span>
        <span class="spacer" />
        <button class="btn btn-sm" :data-agent-add="a.id" @click="openForm(a, null)"><Plus /> Add profile</button>
      </div>
      <div v-if="!a.installed" class="faint small">
        Install it first — see <RouterLink to="/environment">Dev Environment</RouterLink> for the official install command.
      </div>
      <div v-else-if="detected(a.id) && !detected(a.id)!.registeredProfileId && !profilesOf(a.id).length" class="faint small" data-agent-suggest>
        Found at <code>{{ shortPath(a.installed.path, app.info?.homeDir, 60) }}</code>. Add a profile to use your own Codex login (<code>~/.codex</code>).
      </div>

      <div class="caps" data-agent-capabilities>
        <div v-for="c in a.capabilities" :key="c.id" class="cap" :data-capability="c.id" :data-support="c.support" :title="c.detail">
          <component :is="SUPPORT[c.support]" class="cap-ic" :class="c.support" aria-hidden="true" />
          <span>{{ c.label }}</span>
          <span class="faint small detail">{{ c.detail }}</span>
        </div>
      </div>

      <div class="list">
        <div v-for="p in profilesOf(a.id)" :key="p.id" class="list-row" :data-agent-profile="p.name">
          <strong>{{ p.name }}</strong>
          <code class="faint truncate">{{ p.homeDir ? shortPath(p.homeDir, app.info?.homeDir, 50) : '~/.codex (your own)' }}</code>
          <span v-if="!p.turnNotifications" class="badge">turn notifications off</span>
          <span v-if="p.defaultWorkProfile" class="badge">{{ p.defaultWorkProfile }}</span>
          <span class="spacer" />
          <button class="btn btn-sm btn-ghost" :data-agent-edit="p.name" @click="openForm(a, p)">Edit</button>
          <ActionButton class="btn-sm btn-ghost" :data-agent-forget="p.name" :pending="forget.isPending(p.id)" @click="forget.run(p)">Remove</ActionButton>
        </div>
      </div>

      <form v-if="editing && form.agent === a.id" class="form" data-agent-form @submit.prevent="save.run()">
        <div class="two">
          <label class="field">Name <input v-model="form.name" class="input" data-agent-field="name" required /></label>
          <label class="field">
            Home folder <span class="faint">(empty = ~/.codex)</span>
            <input v-model="form.homeDir" class="input mono" placeholder="An existing CODEX_HOME folder" data-agent-field="home" spellcheck="false" />
          </label>
        </div>
        <div class="two">
          <label class="field">
            Work profile
            <select v-model="form.defaultWorkProfile" class="select" data-agent-field="work-profile">
              <option value="">NEXUS default</option>
              <option v-for="w in workProfiles" :key="w.id" :value="w.id">{{ w.name }}</option>
            </select>
          </label>
          <label class="field">Model <input v-model="form.defaultModel" class="input" placeholder="Codex default" spellcheck="false" /></label>
        </div>
        <label class="field">
          Extra options <span class="faint">(allowed: --profile, --sandbox, --ask-for-approval, --search, --oss, --local-provider, --enable, --disable, --no-alt-screen)</span>
          <input v-model="form.extraArgs" class="input mono" placeholder="--search" data-agent-field="args" spellcheck="false" />
        </label>
        <label class="field">Executable <span class="faint">(empty = codex.exe on PATH)</span> <input v-model="form.executable" class="input mono" data-agent-field="executable" spellcheck="false" /></label>
        <label class="checkbox">
          <input v-model="form.turnNotifications" type="checkbox" data-agent-field="notify" />
          <span>Tell NEXUS when a turn finishes (needed for resume). For NEXUS sessions this takes the place of a <code>notify</code> program in your Codex config.</span>
        </label>
        <label class="checkbox">
          <input v-model="form.inheritProviderEnv" type="checkbox" />
          <span>Keep OPENAI_API_KEY and similar from NEXUS's environment (off: removed, so Codex uses its own login).</span>
        </label>
        <p v-if="save.error.value" class="notice notice-error" data-agent-error>
          <TriangleAlert /> <span>{{ save.error.value }}</span>
          <button type="button" class="btn btn-sm btn-ghost" aria-label="Dismiss" @click="save.clearError()"><X /></button>
        </p>
        <div class="row">
          <button type="button" class="btn" @click="editing = null">Cancel</button>
          <ActionButton type="submit" class="btn-primary" data-agent-save :pending="save.pending.value" pending-label="Saving…">Save profile</ActionButton>
        </div>
      </form>
    </div>
  </section>
</template>

<style scoped>
.small {
  font-size: 12px;
}
.agent {
  border-top: 1px solid var(--border);
  padding-top: 12px;
  margin-top: 12px;
}
.head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}
.spacer {
  flex: 1;
}
.caps {
  display: grid;
  gap: 4px;
  margin: 10px 0;
}
.cap {
  display: grid;
  grid-template-columns: 18px 200px 1fr;
  gap: 8px;
  align-items: start;
  font-size: 13px;
}
.cap-ic {
  width: 15px;
  height: 15px;
  margin-top: 2px;
}
.cap-ic.full {
  color: var(--green);
}
.cap-ic.partial {
  color: var(--amber);
}
.cap-ic.unavailable {
  color: var(--text-faint);
}
.detail {
  line-height: 1.4;
}
.form {
  display: grid;
  gap: 10px;
  margin-top: 10px;
  padding: 12px;
  border-radius: 10px;
  background: var(--bg-raised);
  border: 1px solid var(--border);
}
.two {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
}
.field {
  display: grid;
  gap: 4px;
  font-size: 13px;
}
.row {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.truncate {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 320px;
}
</style>
